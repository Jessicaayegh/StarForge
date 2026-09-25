//! Sandboxed WebAssembly plugin execution.
//!
//! The linker intentionally exposes no ambient imports. Filesystem and network
//! access therefore fail at instantiation unless a future capability-specific
//! WIT host interface is explicitly granted. Fuel and epoch deadlines bound
//! CPU time, while Wasmtime keeps plugin linear memory separate from StarForge.

use anyhow::{Context, Result};
use std::path::Path;
use std::time::Duration;
use wasmtime::{Config, Engine, Instance, Linker, Module, Store};

#[derive(Debug, Clone)]
pub struct WasmSandboxPolicy {
    pub fuel: u64,
    pub timeout: Duration,
}

impl Default for WasmSandboxPolicy {
    fn default() -> Self {
        Self {
            fuel: 1_000_000,
            timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Debug, Clone)]
struct HostState {
    _policy: WasmSandboxPolicy,
}

pub struct SandboxedWasmPlugin {
    engine: Engine,
    module: Module,
    policy: WasmSandboxPolicy,
}

impl SandboxedWasmPlugin {
    pub fn load(path: impl AsRef<Path>, policy: WasmSandboxPolicy) -> Result<Self> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.epoch_interruption(true);
        let engine = Engine::new(&config).context("create Wasmtime engine")?;
        let module = Module::from_file(&engine, path).context("load WASM plugin")?;
        Ok(Self {
            engine,
            module,
            policy,
        })
    }

    /// Execute the portable `run() -> i32` entrypoint used by the minimal host.
    /// Components should use the WIT contract in `wit/starforge-plugin.wit`.
    pub fn run(&self) -> Result<i32> {
        let state = HostState {
            _policy: self.policy.clone(),
        };
        let mut store = Store::new(&self.engine, state);
        store
            .set_fuel(self.policy.fuel)
            .context("configure plugin fuel")?;
        store.set_epoch_deadline(1);
        let engine = self.engine.clone();
        let timeout = self.policy.timeout;
        std::thread::scope(|scope| {
            scope.spawn(move || {
                std::thread::sleep(timeout);
                engine.increment_epoch();
            });
            let linker = Linker::new(&self.engine);
            let instance = linker
                .instantiate(&mut store, &self.module)
                .context("instantiate sandboxed plugin")?;
            let entry = instance
                .get_typed_func::<(), i32>(&mut store, "run")
                .context("plugin must export run() -> i32")?;
            entry
                .call(&mut store, ())
                .context("sandboxed plugin execution")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_without_imports_runs_in_a_separate_store() {
        let mut config = Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config).unwrap();
        let module = Module::new(
            &engine,
            "(module (func (export \"run\") (result i32) i32.const 7))",
        )
        .unwrap();
        let mut store = Store::new(
            &engine,
            HostState {
                _policy: WasmSandboxPolicy::default(),
            },
        );
        store.set_fuel(1000).unwrap();
        let instance = Instance::new(&mut store, &module, &[]).unwrap();
        let run = instance
            .get_typed_func::<(), i32>(&mut store, "run")
            .unwrap();
        assert_eq!(run.call(&mut store, ()).unwrap(), 7);
    }

    #[test]
    fn filesystem_imports_are_denied_by_default() {
        let mut config = Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config).unwrap();
        let module = Module::new(
            &engine,
            "(module (import \"wasi_snapshot_preview1\" \"fd_write\" (func)))",
        )
        .unwrap();
        let mut store = Store::new(
            &engine,
            HostState {
                _policy: WasmSandboxPolicy::default(),
            },
        );
        let linker = Linker::new(&engine);
        assert!(linker.instantiate(&mut store, &module).is_err());
    }
}
