use std::any::Any;

// ── Lifecycle hook system (issue #965) ────────────────────────────────────────

/// The five core hook points plugins can subscribe to.
///
/// Hook payloads carry contextual data about the operation that fired them.
/// All payloads include a `schema_version` field so the host and plugin can
/// negotiate forward-compatibility as fields are added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleHook {
    /// Fires before a contract is compiled.  Plugins can validate source,
    /// inject build flags, or abort the build by returning an error.
    PreBuild,
    /// Fires after a successful build.  Read-only: the wasm artifact exists
    /// at `payload.artifact_path`.
    PostBuild,
    /// Fires before a contract is deployed to the network.  Plugins can run
    /// extra policy checks or abort the deployment.
    PreDeploy,
    /// Fires after a successful deployment.  The contract address is
    /// available in `payload.contract_id`.  Ideal for Slack/webhook notify.
    PostDeploy,
    /// Fires before any signing operation (transaction, authorisation entry).
    /// Plugins can enforce approval policies or log sign attempts.
    PreSign,
}

impl LifecycleHook {
    /// Stable string identifier used in log output and documentation.
    pub fn name(&self) -> &'static str {
        match self {
            Self::PreBuild => "pre-build",
            Self::PostBuild => "post-build",
            Self::PreDeploy => "pre-deploy",
            Self::PostDeploy => "post-deploy",
            Self::PreSign => "pre-sign",
        }
    }
}

/// Data passed to a plugin's `on_hook` callback.
///
/// All fields are optional so new fields can be added in later schema
/// versions without breaking plugins compiled against earlier versions.
#[derive(Debug, Clone, Default)]
pub struct HookPayload {
    /// Schema version of this payload (currently `1`).
    pub schema_version: u32,
    /// The hook that fired.
    pub hook: Option<LifecycleHook>,
    /// Network passphrase in use, e.g. `"Test SDF Network ; September 2015"`.
    pub network_passphrase: Option<String>,
    /// Path to the compiled wasm artifact (set for `PostBuild` and later hooks).
    pub artifact_path: Option<String>,
    /// Contract ID after a successful deployment (set for `PostDeploy`).
    pub contract_id: Option<String>,
    /// Source account public key involved in a sign operation (set for `PreSign`).
    pub signer_public_key: Option<String>,
}

/// Policy that governs how a hook failure is handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HookFailurePolicy {
    /// Log the failure but continue the operation.
    #[default]
    Warn,
    /// Abort the operation immediately.
    Block,
}

/// Result returned by a plugin's `on_hook` implementation.
#[derive(Debug, Clone)]
pub struct HookResult {
    /// Whether the hook succeeded.
    pub ok: bool,
    /// Optional human-readable message surfaced to the user.
    pub message: Option<String>,
}

impl HookResult {
    pub fn success() -> Self {
        Self { ok: true, message: None }
    }
    pub fn failure(msg: impl Into<String>) -> Self {
        Self { ok: false, message: Some(msg.into()) }
    }
}

// ── Plugin trait ──────────────────────────────────────────────────────────────

pub trait Plugin: Any + Send + Sync {
    fn name(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn description(&self) -> &'static str;

    fn on_load(&self) {}
    fn on_unload(&self) {}

    /// Called at each lifecycle hook point unless `--no-hooks` was passed.
    ///
    /// The default implementation is a no-op so existing plugins compiled
    /// against earlier interface versions remain compatible.
    ///
    /// Return `HookResult::failure(reason)` from `PreBuild`, `PreDeploy`, or
    /// `PreSign` hooks to abort the operation (subject to the active
    /// `HookFailurePolicy`).
    fn on_hook(&self, _payload: &HookPayload) -> HookResult {
        HookResult::success()
    }

    fn execute(&self, args: &[String]) -> Result<(), String>;
}

pub struct PluginDeclaration {
    pub rustc_version: &'static str,
    pub core_version: &'static str,
    pub register: unsafe fn(&mut dyn PluginRegistrar),
}

pub trait PluginRegistrar {
    fn register_plugin(&mut self, plugin: Box<dyn Plugin>);
}

#[macro_export]
macro_rules! export_plugin {
    ($register:expr) => {
        #[doc(hidden)]
        #[no_mangle]
        pub static PLUGIN_DECLARATION: $crate::plugins::PluginDeclaration =
            $crate::plugins::PluginDeclaration {
                rustc_version: $crate::plugins::interface::RUSTC_VERSION,
                core_version: $crate::plugins::interface::CORE_VERSION,
                register: $register,
            };
    };
}

pub const RUSTC_VERSION: &str = env!("RUSTC_VERSION");
pub const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_five_hook_points_have_stable_names() {
        assert_eq!(LifecycleHook::PreBuild.name(), "pre-build");
        assert_eq!(LifecycleHook::PostBuild.name(), "post-build");
        assert_eq!(LifecycleHook::PreDeploy.name(), "pre-deploy");
        assert_eq!(LifecycleHook::PostDeploy.name(), "post-deploy");
        assert_eq!(LifecycleHook::PreSign.name(), "pre-sign");
    }

    #[test]
    fn hook_result_success_is_ok() {
        let r = HookResult::success();
        assert!(r.ok);
        assert!(r.message.is_none());
    }

    #[test]
    fn hook_result_failure_carries_message() {
        let r = HookResult::failure("policy violation");
        assert!(!r.ok);
        assert_eq!(r.message.as_deref(), Some("policy violation"));
    }

    #[test]
    fn default_hook_failure_policy_is_warn() {
        assert_eq!(HookFailurePolicy::default(), HookFailurePolicy::Warn);
    }

    #[test]
    fn payload_default_has_schema_version_zero() {
        let p = HookPayload::default();
        assert_eq!(p.schema_version, 0);
        assert!(p.hook.is_none());
    }

    #[test]
    fn payload_fields_set_correctly() {
        let p = HookPayload {
            schema_version: 1,
            hook: Some(LifecycleHook::PostDeploy),
            contract_id: Some("CABC123".into()),
            ..Default::default()
        };
        assert_eq!(p.schema_version, 1);
        assert_eq!(p.hook, Some(LifecycleHook::PostDeploy));
        assert_eq!(p.contract_id.as_deref(), Some("CABC123"));
    }
}
