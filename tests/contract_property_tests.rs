//! Property-based tests for Soroban contract testing infrastructure.
//!
//! These tests use `proptest` to automatically generate inputs and verify
//! invariants that must hold for all valid (and many invalid) inputs in the
//! contract testing, WASM validation, and mock execution paths.
//!
//! Run with:
//!   cargo test --test contract_property_tests
//!
//! Increase iterations for deeper coverage:
//!   PROPTEST_CASES=5000 cargo test --test contract_property_tests
//!
//! Sections 7-11 (issue #725) use the shared generators in
//! `tests/property_fixtures/` (amounts, addresses, auth contexts, token
//! operations, storage keys, WASM modules) and run SEP-41 invariants against
//! a compiled copy of `templates/examples/sep41-token` in a Soroban test
//! environment. Their case counts are capped with `bounded_config` so the
//! whole file stays within a few seconds in CI.

#![allow(dead_code, unused_imports)]

mod property_fixtures;

#[path = "fixtures/sep41_token.rs"]
mod sep41_token;

use property_fixtures::{
    account_strkey, amount, auth_context, bounded_config, contract_strkey, corrupt_strkey,
    extreme_amount, negative_amount, storage_key, strkey_corruption, token_op, token_op_with,
    token_ops, wasm_module, AuthContext, TokenModel, TokenOp, MAX_AMOUNT,
};
use proptest::prelude::*;
use sep41_token::{Sep41Token, Sep41TokenClient};
use soroban_sdk::testutils::{
    Address as _, AuthorizedFunction, EnvTestConfig, MockAuth, MockAuthInvoke,
};
use soroban_sdk::{Address, Env, IntoVal, Val};
use starforge::utils::config::{validate_contract_id, validate_public_key};
use starforge::utils::contract_mocks::{
    MockAddress, MockAuthContext, MockContractClient, MockEnvironment, MockStorage,
    MockTokenBalances, StorageKey,
};
use starforge::utils::mock_soroban::validate_wasm;
use starforge::utils::wasm_hash::{compute_wasm_hash, BuildEnvironment};

// ─────────────────────────────────────────────────────────────────────────────
// 1. WASM validation — property tests
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    /// Any byte sequence shorter than 8 bytes must be rejected.
    #[test]
    fn prop_short_wasm_rejected(data in prop::collection::vec(any::<u8>(), 0..8)) {
        let result = validate_wasm(&data);
        prop_assert!(result.is_err(), "short WASM should be rejected");
    }

    /// Any byte sequence with a valid 4-byte magic header but < 8 bytes is rejected.
    #[test]
    fn prop_magic_header_short_rejected(data in prop::collection::vec(any::<u8>(), 0..4)) {
        let mut input = b"\0asm".to_vec();
        input.extend(data);
        let result = validate_wasm(&input);
        prop_assert!(result.is_err(), "short WASM with magic header should be rejected");
    }

    /// Any byte sequence >= 8 bytes with a valid magic header is accepted.
    #[test]
    fn prop_valid_magic_header_accepted(data in prop::collection::vec(any::<u8>(), 4..1024)) {
        let mut input = b"\0asm".to_vec();
        input.extend(data);
        let result = validate_wasm(&input);
        prop_assert!(result.is_ok(), "WASM with valid header and >= 8 bytes should be accepted");
    }

    /// Any byte sequence without the magic header is rejected.
    #[test]
    fn prop_missing_magic_header_rejected(data in prop::collection::vec(any::<u8>(), 8..1024)) {
        prop_assume!(!data.starts_with(b"\0asm"));
        let result = validate_wasm(&data);
        prop_assert!(result.is_err(), "WASM without magic header should be rejected");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. WASM hash computation — property tests
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    /// WASM hash of empty bytes must be rejected.
    #[test]
    fn prop_empty_wasm_hash_rejected(_data: ()) {
        let result = compute_wasm_hash(&[], BuildEnvironment::Linux);
        prop_assert!(result.is_err(), "empty WASM should be rejected");
    }

    /// WASM hash of bytes without magic header must be rejected.
    #[test]
    fn prop_no_magic_wasm_hash_rejected(data in prop::collection::vec(any::<u8>(), 8..1024)) {
        prop_assume!(!data.starts_with(b"\0asm"));
        let result = compute_wasm_hash(&data, BuildEnvironment::Linux);
        prop_assert!(result.is_err(), "WASM without magic header should be rejected");
    }

    /// WASM hash of valid WASM is always a 64-char lowercase hex string.
    #[test]
    fn prop_valid_wasm_hash_format(data in prop::collection::vec(any::<u8>(), 4..1024)) {
        let mut input = b"\0asm".to_vec();
        input.extend(data);
        if let Ok(hash) = compute_wasm_hash(&input, BuildEnvironment::Linux) {
            prop_assert_eq!(hash.len(), 64);
            prop_assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
            prop_assert!(hash.chars().all(|c| !c.is_ascii_uppercase()));
        }
    }

    /// WASM hash is deterministic: same input always produces same output.
    #[test]
    fn prop_wasm_hash_deterministic(data in prop::collection::vec(any::<u8>(), 4..1024)) {
        let mut input = b"\0asm".to_vec();
        input.extend(data);
        if let Ok(h1) = compute_wasm_hash(&input, BuildEnvironment::Linux) {
            let h2 = compute_wasm_hash(&input, BuildEnvironment::Linux).unwrap();
            prop_assert_eq!(h1, h2);
        }
    }

    /// WASM hash on unsupported environment is rejected.
    #[test]
    fn prop_unsupported_env_rejected(data in prop::collection::vec(any::<u8>(), 8..1024)) {
        let mut input = b"\0asm".to_vec();
        input.extend(data);
        let result = compute_wasm_hash(&input, BuildEnvironment::Unsupported("bsd".into()));
        prop_assert!(result.is_err(), "unsupported environment should be rejected");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Mock contract invocation — property tests
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    /// Invoking a function N times always records exactly N calls.
    #[test]
    fn prop_call_count_matches_invocations(
        function in "[a-z_]{1,20}",
        repeat in 0u8..10,
    ) {
        let client = MockContractClient::new(MockAddress::contract(1));
        for _ in 0..repeat {
            let _ = client.invoke(&function, vec![], None, 100);
        }
        prop_assert_eq!(client.call_count(&function), repeat as usize);
        prop_assert_eq!(client.total_calls(), repeat as usize);
    }

    /// Pre-configured return values are returned deterministically.
    #[test]
    fn prop_mock_return_is_deterministic(
        function in "[a-z_]{1,20}",
        value in any::<i64>(),
    ) {
        let client = MockContractClient::new(MockAddress::contract(1));
        client.mock_return(&function, serde_json::json!(value));
        let result1 = client.invoke(&function, vec![], None, 100);
        let result2 = client.invoke(&function, vec![], None, 100);
        prop_assert!(result1.is_ok());
        prop_assert!(result2.is_ok());
        prop_assert_eq!(result1.unwrap(), result2.unwrap());
    }

    /// Pre-configured errors are returned deterministically.
    #[test]
    fn prop_mock_error_is_deterministic(
        function in "[a-z_]{1,20}",
        error_msg in "[a-z_]{1,30}",
    ) {
        let client = MockContractClient::new(MockAddress::contract(1));
        client.mock_error(&function, &error_msg);
        let result1 = client.invoke(&function, vec![], None, 100);
        let result2 = client.invoke(&function, vec![], None, 100);
        prop_assert!(result1.is_err());
        prop_assert!(result2.is_err());
        prop_assert_eq!(result1.unwrap_err(), result2.unwrap_err());
    }

    /// Error takes priority over return value when both are configured.
    #[test]
    fn prop_error_takes_priority_over_return(
        function in "[a-z_]{1,20}",
    ) {
        let client = MockContractClient::new(MockAddress::contract(1));
        client.mock_return(&function, serde_json::json!(42u64));
        client.mock_error(&function, "error");
        let result = client.invoke(&function, vec![], None, 100);
        prop_assert!(result.is_err(), "error should take priority over return value");
    }

    /// Reset clears all call history and configurations.
    #[test]
    fn prop_reset_clears_state(
        function in "[a-z_]{1,20}",
        repeat in 1u8..5,
    ) {
        let client = MockContractClient::new(MockAddress::contract(1));
        client.mock_return(&function, serde_json::json!(1u64));
        for _ in 0..repeat {
            let _ = client.invoke(&function, vec![], None, 100);
        }
        prop_assert_eq!(client.total_calls(), repeat as usize);
        client.reset();
        prop_assert_eq!(client.total_calls(), 0);
        prop_assert_eq!(client.call_count(&function), 0);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. Mock storage — property tests
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    /// Setting and getting a value always returns the same value.
    #[test]
    fn prop_storage_set_get_roundtrip(
        key in "[a-z_]{1,20}",
        value in any::<i64>(),
    ) {
        let mut storage = MockStorage::new();
        let storage_key = StorageKey::instance(&key);
        storage.set(storage_key.clone(), serde_json::json!(value));
        let retrieved = storage.get(&storage_key);
        prop_assert!(retrieved.is_some());
        prop_assert_eq!(retrieved.unwrap(), &serde_json::json!(value));
    }

    /// Removing a key makes it absent.
    #[test]
    fn prop_storage_remove_makes_absent(
        key in "[a-z_]{1,20}",
        value in any::<i64>(),
    ) {
        let mut storage = MockStorage::new();
        let storage_key = StorageKey::persistent(&key);
        storage.set(storage_key.clone(), serde_json::json!(value));
        prop_assert!(storage.has(&storage_key));
        storage.remove(&storage_key);
        prop_assert!(!storage.has(&storage_key));
    }

    /// Storage length matches the number of unique keys set.
    #[test]
    fn prop_storage_len_matches_keys(
        keys in prop::collection::vec("[a-z]{1,10}", 0..20),
        value in any::<i64>(),
    ) {
        let mut storage = MockStorage::new();
        let unique_count = keys.iter().collect::<std::collections::HashSet<_>>().len();
        for key in &keys {
            storage.set(StorageKey::instance(key), serde_json::json!(value));
        }
        prop_assert_eq!(storage.len(), unique_count);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. Mock address — property tests
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    /// Account addresses always start with 'G' and are 57 chars (GA + 55 digits).
    #[test]
    fn prop_account_address_format(id in any::<u32>()) {
        let addr = MockAddress::account(id);
        let s = addr.as_str();
        prop_assert!(s.starts_with('G'));
        prop_assert_eq!(s.len(), 57);
    }

    /// Contract addresses always start with 'C' and are 63 chars (C + 62 hex).
    #[test]
    fn prop_contract_address_format(id in any::<u32>()) {
        let addr = MockAddress::contract(id);
        let s = addr.as_str();
        prop_assert!(s.starts_with('C'));
        prop_assert_eq!(s.len(), 63);
    }

    /// Display trait matches as_str.
    #[test]
    fn prop_display_matches_as_str(id in any::<u32>()) {
        let addr = MockAddress::account(id);
        prop_assert_eq!(format!("{}", addr), addr.as_str());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 6. Mock environment — property tests
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    /// Reset clears storage, events, and auth but preserves ledger config.
    #[test]
    fn prop_env_reset_preserves_ledger(
        initial_seq in 1u32..1000,
    ) {
        let mut env = MockEnvironment::new();
        env.ledger = env.ledger.clone().at_sequence(initial_seq);
        env.storage.set(StorageKey::instance("test"), serde_json::json!(1u64));
        env.emit_event(
            MockAddress::contract(1),
            vec![serde_json::json!("test")],
            serde_json::json!({"data": 1}),
        );
        let account = MockAddress::account(1);
        env.auth.auto_approve(account.clone());
        env.auth.require_auth(&account, &MockAddress::contract(1), "test_fn");
        env.auth.auto_approve(MockAddress::account(1));
        env.auth.require_auth(&MockAddress::account(1), &MockAddress::contract(1), "test");

        prop_assert!(!env.storage.is_empty());
        prop_assert!(!env.events.is_empty());
        prop_assert!(env.auth.auth_count() > 0);

        env.reset();

        prop_assert!(env.storage.is_empty());
        prop_assert!(env.events.is_empty());
        prop_assert_eq!(env.auth.auth_count(), 0);
        prop_assert_eq!(env.ledger.sequence, initial_seq);
    }

    /// advance_ledger increments sequence and timestamp.
    #[test]
    fn prop_ledger_advance_increments(
        initial_seq in 1u32..1000,
        ledgers in 1u32..100,
    ) {
        let mut env = MockEnvironment::new();
        env.ledger = env.ledger.clone().at_sequence(initial_seq);
        let initial_ts = env.ledger.timestamp;
        env.advance_ledger(ledgers);
        prop_assert_eq!(env.ledger.sequence, initial_seq + ledgers);
        prop_assert_eq!(env.ledger.timestamp, initial_ts + u64::from(ledgers) * 5);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 7. Address and auth-context generators — property tests
// ─────────────────────────────────────────────────────────────────────────────

/// Size of the actor pool used by the token properties. Actor 0 is the admin.
const ACTORS: usize = 4;
const ADMIN: usize = 0;

proptest! {
    #![proptest_config(bounded_config(128))]

    /// Generated account strkeys pass validation; any corruption is rejected.
    #[test]
    fn prop_account_strkey_encoding(key in account_strkey(), corruption in strkey_corruption()) {
        prop_assert!(validate_public_key(&key).is_ok(), "{} should be valid", key);
        let bad = corrupt_strkey(&key, &corruption);
        prop_assert!(validate_public_key(&bad).is_err(), "{:?} -> {} should be rejected", corruption, bad);
    }

    /// Generated contract strkeys pass validation; any corruption is rejected.
    #[test]
    fn prop_contract_strkey_encoding(id in contract_strkey(), corruption in strkey_corruption()) {
        prop_assert!(validate_contract_id(&id).is_ok(), "{} should be valid", id);
        let bad = corrupt_strkey(&id, &corruption);
        prop_assert!(validate_contract_id(&bad).is_err(), "{:?} -> {} should be rejected", corruption, bad);
        // An account key is never accepted where a contract id is expected.
        let as_account = format!("G{}", &id[1..]);
        prop_assert!(validate_contract_id(&as_account).is_err());
    }

    /// In the mock auth context, exactly the signers are approved.
    #[test]
    fn prop_mock_auth_only_signers_approved(ctx in auth_context(ACTORS), function in "[a-z_]{1,16}") {
        let contract = MockAddress::contract(1);
        let mut auth = MockAuthContext::new();
        for &signer in &ctx.signers {
            auth.auto_approve(MockAddress::account(signer as u32));
        }
        for actor in 0..ACTORS {
            let addr = MockAddress::account(actor as u32);
            let approved = auth.require_auth(&addr, &contract, &function);
            prop_assert_eq!(approved, ctx.signed_by(actor));
            prop_assert_eq!(auth.was_authorised(&addr, &function), ctx.signed_by(actor));
        }
        prop_assert_eq!(auth.auth_count(), ACTORS);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 8. Token model and mock balances — property tests
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(bounded_config(256))]

    /// The reference model itself upholds SEP-41 invariants for any sequence:
    /// supply equals the sum of balances, nothing goes negative, and rejected
    /// operations leave the state untouched.
    #[test]
    fn prop_token_model_invariants(ops in token_ops(ACTORS, 1..48)) {
        let mut model = TokenModel::new(ADMIN);
        for op in &ops {
            let before = model.clone();
            match model.apply(op) {
                Ok(()) => {
                    if !op.changes_supply() {
                        prop_assert_eq!(model.total_supply, before.total_supply);
                    }
                }
                Err(_) => prop_assert_eq!(&model, &before),
            }
            prop_assert_eq!(model.sum_of_balances(), model.total_supply);
            prop_assert!(model.balances.values().all(|b| *b >= 0));
            prop_assert!(model.allowances.values().all(|a| *a >= 0));
        }
    }

    /// The shared `MockTokenBalances` helper conserves supply across transfers
    /// and leaves balances untouched when a transfer is rejected.
    #[test]
    fn prop_mock_token_balances_conserve_supply(
        mints in proptest::collection::vec(amount(), ACTORS),
        transfers in proptest::collection::vec((0..ACTORS, 0..ACTORS, amount()), 0..32),
    ) {
        let token = "TOKEN";
        let addr = |i: usize| MockAddress::account(i as u32).0;
        let mut balances = MockTokenBalances::new();
        for (i, m) in mints.iter().enumerate() {
            balances.mint(token, &addr(i), *m);
        }
        let supply: i128 = mints.iter().sum();
        for (from, to, amt) in transfers {
            let before: Vec<i128> = (0..ACTORS).map(|i| balances.get(token, &addr(i))).collect();
            let result = balances.transfer(token, &addr(from), &addr(to), amt);
            prop_assert_eq!(result.is_ok(), before[from] >= amt);
            let after: Vec<i128> = (0..ACTORS).map(|i| balances.get(token, &addr(i))).collect();
            if result.is_err() {
                prop_assert_eq!(&after, &before);
            }
            prop_assert_eq!(after.iter().sum::<i128>(), supply);
            prop_assert!(after.iter().all(|b| *b >= 0));
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 9. SEP-41 template contract — property tests
// ─────────────────────────────────────────────────────────────────────────────

/// A deployed copy of the SEP-41 template in a Soroban test environment,
/// with a fixed pool of actor addresses (actor 0 is the admin).
struct TokenHarness {
    env: Env,
    id: Address,
    actors: Vec<Address>,
}

impl TokenHarness {
    fn new() -> Self {
        // No snapshot files: property tests create many short-lived envs.
        let env = Env::new_with_config(EnvTestConfig {
            capture_snapshot_at_drop: false,
        });
        env.mock_all_auths();
        let id = env.register(Sep41Token, ());
        let actors: Vec<Address> = (0..ACTORS).map(|_| Address::generate(&env)).collect();
        let harness = Self { env, id, actors };
        harness.client().initialize(
            &harness.actors[ADMIN],
            &7,
            &soroban_sdk::String::from_str(&harness.env, "Property Token"),
            &soroban_sdk::String::from_str(&harness.env, "PROP"),
        );
        harness
    }

    /// A harness where every actor holds `balance` and every (owner, spender)
    /// pair has an allowance of `allowance`, mirrored in the returned model.
    fn funded(balance: i128, allowance: i128) -> (Self, TokenModel) {
        let harness = Self::new();
        let mut model = TokenModel::new(ADMIN);
        let mut setup = Vec::new();
        for owner in 0..ACTORS {
            setup.push(TokenOp::Mint {
                to: owner,
                amount: balance,
            });
            for spender in 0..ACTORS {
                setup.push(TokenOp::Approve {
                    from: owner,
                    spender,
                    amount: allowance,
                });
            }
        }
        for op in &setup {
            model.apply(op).expect("setup op is valid");
            assert!(harness.invoke(op), "setup op {:?} failed", op);
        }
        (harness, model)
    }

    fn client(&self) -> Sep41TokenClient<'_> {
        Sep41TokenClient::new(&self.env, &self.id)
    }

    fn addr(&self, actor: usize) -> &Address {
        &self.actors[actor]
    }

    /// Invocation arguments of `op`, as passed to `require_auth`.
    fn args(&self, op: &TokenOp) -> soroban_sdk::Vec<Val> {
        let e = &self.env;
        match *op {
            TokenOp::Mint { to, amount } => (self.addr(to).clone(), amount).into_val(e),
            TokenOp::Transfer { from, to, amount } => {
                (self.addr(from).clone(), self.addr(to).clone(), amount).into_val(e)
            }
            TokenOp::Approve {
                from,
                spender,
                amount,
            } => (self.addr(from).clone(), self.addr(spender).clone(), amount).into_val(e),
            TokenOp::TransferFrom {
                spender,
                from,
                to,
                amount,
            } => (
                self.addr(spender).clone(),
                self.addr(from).clone(),
                self.addr(to).clone(),
                amount,
            )
                .into_val(e),
            TokenOp::Burn { from, amount } => (self.addr(from).clone(), amount).into_val(e),
            TokenOp::BurnFrom {
                spender,
                from,
                amount,
            } => (self.addr(spender).clone(), self.addr(from).clone(), amount).into_val(e),
        }
    }

    /// Invoke `op` on the contract, returning whether it succeeded.
    fn invoke(&self, op: &TokenOp) -> bool {
        let c = self.client();
        let a = |i: usize| self.addr(i);
        match *op {
            TokenOp::Mint { to, amount } => c.try_mint(a(to), &amount).is_ok(),
            TokenOp::Transfer { from, to, amount } => {
                c.try_transfer(a(from), a(to), &amount).is_ok()
            }
            TokenOp::Approve {
                from,
                spender,
                amount,
            } => c.try_approve(a(from), a(spender), &amount).is_ok(),
            TokenOp::TransferFrom {
                spender,
                from,
                to,
                amount,
            } => c
                .try_transfer_from(a(spender), a(from), a(to), &amount)
                .is_ok(),
            TokenOp::Burn { from, amount } => c.try_burn(a(from), &amount).is_ok(),
            TokenOp::BurnFrom {
                spender,
                from,
                amount,
            } => c.try_burn_from(a(spender), a(from), &amount).is_ok(),
        }
    }

    /// Invoke `op` with only the actors in `ctx` having signed it.
    fn invoke_with_auth(&self, op: &TokenOp, ctx: &AuthContext) -> bool {
        let invoke = MockAuthInvoke {
            contract: &self.id,
            fn_name: op.function(),
            args: self.args(op),
            sub_invokes: &[],
        };
        let auths: Vec<MockAuth> = ctx
            .signers
            .iter()
            .map(|&signer| MockAuth {
                address: self.addr(signer),
                invoke: &invoke,
            })
            .collect();
        self.env.mock_auths(&auths);
        let ok = self.invoke(op);
        self.env.mock_all_auths();
        ok
    }

    fn balances(&self) -> Vec<i128> {
        let c = self.client();
        self.actors.iter().map(|a| c.balance(a)).collect()
    }

    /// Assert that on-chain balances and allowances equal the model and that
    /// no balance or allowance is negative.
    fn assert_matches(&self, model: &TokenModel) -> Result<(), TestCaseError> {
        let c = self.client();
        let balances = self.balances();
        for (actor, bal) in balances.iter().enumerate() {
            prop_assert!(*bal >= 0, "negative balance for actor {}: {}", actor, bal);
            prop_assert_eq!(*bal, model.balance(actor), "balance of actor {}", actor);
        }
        prop_assert_eq!(balances.iter().sum::<i128>(), model.total_supply);
        for from in 0..ACTORS {
            for spender in 0..ACTORS {
                let allowance = c.allowance(self.addr(from), self.addr(spender));
                prop_assert!(allowance >= 0);
                prop_assert_eq!(allowance, model.allowance(from, spender));
            }
        }
        Ok(())
    }
}

proptest! {
    #![proptest_config(bounded_config(32))]

    /// For any sequence of calls (including invalid ones) the template agrees
    /// with the reference model on success/failure and on the resulting
    /// balances and allowances, so supply is conserved by transfers and no
    /// balance or allowance ever goes negative.
    #[test]
    fn prop_sep41_matches_reference_model(ops in token_ops(ACTORS, 1..24)) {
        let token = TokenHarness::new();
        let mut model = TokenModel::new(ADMIN);
        for op in &ops {
            let expected = model.apply(op);
            let ok = token.invoke(op);
            prop_assert_eq!(ok, expected.is_ok(), "{:?}: model said {:?}", op, expected);
            token.assert_matches(&model)?;
        }
    }

    /// Every successful state-changing call is authorized by exactly one
    /// address, the one SEP-41 requires: the admin for `mint`, the owner for
    /// `transfer`/`approve`/`burn`, the spender for `transfer_from`/`burn_from`.
    #[test]
    fn prop_sep41_requires_exactly_the_right_signer(ops in token_ops(ACTORS, 1..16)) {
        let (token, mut model) = TokenHarness::funded(MAX_AMOUNT, MAX_AMOUNT);
        for op in &ops {
            let expected = model.apply(op);
            let ok = token.invoke(op);
            prop_assert_eq!(ok, expected.is_ok());
            if !ok {
                continue;
            }
            let auths = token.env.auths();
            prop_assert_eq!(auths.len(), 1, "{:?} recorded auths {:?}", op, auths.len());
            let (signer, invocation) = &auths[0];
            prop_assert_eq!(signer, token.addr(op.required_signer(ADMIN)));
            match &invocation.function {
                AuthorizedFunction::Contract((contract, name, _)) => {
                    prop_assert_eq!(contract, &token.id);
                    prop_assert_eq!(
                        name,
                        &soroban_sdk::Symbol::new(&token.env, op.function())
                    );
                }
                _ => prop_assert!(false, "unexpected authorized function for {:?}", op),
            }
            prop_assert!(invocation.sub_invocations.is_empty());
        }
    }

    /// Under an arbitrary auth context a call succeeds only if the required
    /// signer signed it; unauthorized calls change nothing.
    #[test]
    fn prop_sep41_auth_context_gates_state_changes(
        steps in proptest::collection::vec(
            (token_op_with(ACTORS, 0i128..=1_500), auth_context(ACTORS)),
            1..10,
        ),
    ) {
        let (token, mut model) = TokenHarness::funded(1_000, 500);
        for (op, ctx) in &steps {
            let expected = model.apply_with_auth(op, ctx);
            let ok = token.invoke_with_auth(op, ctx);
            prop_assert_eq!(ok, expected.is_ok(), "{:?} with signers {:?}: model said {:?}", op, ctx.signers, expected);
            token.assert_matches(&model)?;
        }
    }

    /// Negative amounts are rejected by every entry point and leave state
    /// untouched, so they can never mint value or create negative balances.
    #[test]
    fn prop_sep41_negative_amounts_rejected(
        ops in proptest::collection::vec(token_op_with(ACTORS, negative_amount()), 1..12),
    ) {
        let (token, model) = TokenHarness::funded(1_000, 500);
        for op in &ops {
            prop_assert!(!token.invoke(op), "{:?} should be rejected", op);
            token.assert_matches(&model)?;
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 10. SEP-41 allowance and overflow edge cases — property tests
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(bounded_config(32))]

    /// `approve` overwrites (never adds to) the previous allowance, and
    /// `transfer_from`/`burn_from` spend exactly the amount used, succeed only
    /// within both allowance and balance, and never touch other allowances.
    #[test]
    fn prop_sep41_allowance_semantics(
        balance in 0i128..=10_000,
        first in 0i128..=10_000,
        second in 0i128..=10_000,
        spend in 0i128..=10_000,
        burn in any::<bool>(),
    ) {
        let (owner, spender, other, to) = (1, 2, 3, 0);
        let token = TokenHarness::new();
        let c = token.client();
        let a = |i: usize| token.addr(i);
        c.mint(a(owner), &balance);
        c.approve(a(owner), a(spender), &first);
        c.approve(a(owner), a(other), &7);
        c.approve(a(owner), a(spender), &second);
        prop_assert_eq!(c.allowance(a(owner), a(spender)), second);

        let ok = if burn {
            c.try_burn_from(a(spender), a(owner), &spend).is_ok()
        } else {
            c.try_transfer_from(a(spender), a(owner), a(to), &spend).is_ok()
        };
        let allowed = spend <= second && spend <= balance;
        prop_assert_eq!(ok, allowed);
        let used = if ok { spend } else { 0 };
        prop_assert_eq!(c.allowance(a(owner), a(spender)), second - used);
        prop_assert_eq!(c.allowance(a(owner), a(other)), 7);
        prop_assert_eq!(c.allowance(a(spender), a(owner)), 0);
        prop_assert_eq!(c.balance(a(owner)), balance - used);
        prop_assert_eq!(c.balance(a(to)), if burn { 0 } else { used });
    }

    /// Balance arithmetic is checked: a mint or transfer that would overflow
    /// an `i128` is rejected instead of wrapping.
    #[test]
    fn prop_sep41_overflow_rejected(held in extreme_amount(), extra in 1i128..=MAX_AMOUNT) {
        let token = TokenHarness::new();
        let c = token.client();
        let (holder, sender) = (1, 2);
        c.mint(token.addr(holder), &held);
        let fits = held.checked_add(extra).is_some();

        prop_assert_eq!(c.try_mint(token.addr(holder), &extra).is_ok(), fits);
        let held_now = if fits { held + extra } else { held };
        prop_assert_eq!(c.balance(token.addr(holder)), held_now);

        c.mint(token.addr(sender), &extra);
        let fits = held_now.checked_add(extra).is_some();
        prop_assert_eq!(
            c.try_transfer(token.addr(sender), token.addr(holder), &extra).is_ok(),
            fits
        );
        prop_assert_eq!(c.balance(token.addr(holder)), if fits { held_now + extra } else { held_now });
        prop_assert_eq!(c.balance(token.addr(sender)), if fits { 0 } else { extra });
    }
}

/// The compiled fixture must stay identical to the template it mirrors, or
/// the SEP-41 properties above would be testing stale code.
#[test]
fn sep41_fixture_matches_template() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let template = std::fs::read_to_string(format!(
        "{manifest}/templates/examples/sep41-token/src/lib.rs"
    ))
    .expect("read SEP-41 template");
    let fixture = std::fs::read_to_string(format!("{manifest}/tests/fixtures/sep41_token.rs"))
        .expect("read SEP-41 fixture");

    let contract = template
        .split("#[cfg(test)]")
        .next()
        .expect("template has a contract section")
        .replace("#![no_std]", "")
        .replace("{{PROJECT_NAME_PASCAL}}", "Sep41Token");
    let copy = fixture
        .split("// BEGIN TEMPLATE COPY")
        .nth(1)
        .and_then(|rest| rest.split("// END TEMPLATE COPY").next())
        .expect("fixture has BEGIN/END TEMPLATE COPY markers");

    let normalize = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    assert_eq!(
        normalize(copy),
        normalize(&contract),
        "tests/fixtures/sep41_token.rs drifted from templates/examples/sep41-token/src/lib.rs"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 11. Storage scopes and upgrade hashing — property tests
// ─────────────────────────────────────────────────────────────────────────────

proptest! {
    #![proptest_config(bounded_config(128))]

    /// The same key name in different durability scopes is stored
    /// independently, so writing or removing one never clobbers another.
    #[test]
    fn prop_storage_scopes_are_isolated(key in storage_key(), a in any::<i64>(), b in any::<i64>()) {
        let others: Vec<StorageKey> = ["instance", "persistent", "temporary"]
            .iter()
            .filter(|s| **s != key.scope)
            .map(|s| StorageKey { scope: (*s).to_string(), key: key.key.clone() })
            .collect();
        let mut storage = MockStorage::new();
        storage.set(key.clone(), serde_json::json!(a));
        for other in &others {
            storage.set(other.clone(), serde_json::json!(b));
        }
        prop_assert_eq!(storage.len(), 3);
        prop_assert_eq!(storage.entries_by_scope(&key.scope).len(), 1);
        storage.remove(&key);
        prop_assert!(!storage.has(&key));
        for other in &others {
            prop_assert_eq!(storage.get(other), Some(&serde_json::json!(b)));
        }
    }

    /// Upgrade detection: re-deploying identical code yields the same hash,
    /// and any code change yields a different one.
    #[test]
    fn prop_upgrade_hash_tracks_code(old in wasm_module(), new in wasm_module()) {
        let h_old = compute_wasm_hash(&old, BuildEnvironment::Linux).unwrap();
        let h_again = compute_wasm_hash(&old.clone(), BuildEnvironment::Linux).unwrap();
        let h_new = compute_wasm_hash(&new, BuildEnvironment::Linux).unwrap();
        prop_assert_eq!(&h_old, &h_again);
        prop_assert_eq!(old == new, h_old == h_new);
    }
}
