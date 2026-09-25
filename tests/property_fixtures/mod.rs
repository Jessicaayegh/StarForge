//! Reusable `proptest` generators and reference models for contract property
//! tests (issue #725).
//!
//! Include from an integration test with `mod property_fixtures;`. The module
//! is deliberately independent of `soroban-sdk`: generators produce plain
//! values (amounts, actor indices, strkey-shaped addresses, auth contexts,
//! token operations) and [`TokenModel`] is a pure-Rust SEP-41 reference model
//! that a test can compare a real contract or a mock against.
//!
//! Every generator keeps values small and structured so that failures shrink
//! quickly, and [`bounded_config`] caps case counts and shrinking so CI stays
//! fast.

#![allow(dead_code)]

use proptest::prelude::*;
use proptest::test_runner::Config as ProptestConfig;
use starforge::utils::contract_mocks::{MockAddress, StorageKey};
use std::collections::{BTreeMap, BTreeSet};

// ─────────────────────────────────────────────────────────────────────────────
// Runtime bounds
// ─────────────────────────────────────────────────────────────────────────────

/// Largest "ordinary" amount a generator produces (10^15 stroops, i.e. 10^8
/// tokens at 7 decimals). Sequences of a few dozen operations bounded by this
/// value can never overflow an `i128`, so overflow is only exercised by the
/// dedicated edge-case generators.
pub const MAX_AMOUNT: i128 = 1_000_000_000_000_000;

/// Build a proptest config with a bounded number of cases and bounded
/// shrinking. `PROPTEST_CASES` still overrides the case count, so contributors
/// can run a deeper sweep locally (`PROPTEST_CASES=5000 cargo test ...`).
pub fn bounded_config(default_cases: u32) -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default_cases);
    ProptestConfig {
        cases,
        max_shrink_iters: 512,
        max_shrink_time: 10_000,
        ..ProptestConfig::default()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Amounts
// ─────────────────────────────────────────────────────────────────────────────

/// A valid (non-negative) token amount, biased towards boundary values.
pub fn amount() -> impl Strategy<Value = i128> {
    prop_oneof![
        3 => 0..=MAX_AMOUNT,
        1 => 0i128..=16,
        1 => Just(MAX_AMOUNT),
    ]
}

/// A strictly negative amount, including `i128::MIN`.
pub fn negative_amount() -> impl Strategy<Value = i128> + Clone {
    prop_oneof![
        3 => -MAX_AMOUNT..=-1i128,
        1 => Just(-1i128),
        1 => Just(i128::MIN),
    ]
    .boxed()
}

/// Any amount a caller could submit: mostly valid, sometimes negative. Used
/// by operation generators so rejection paths are exercised in every run.
pub fn signed_amount() -> impl Strategy<Value = i128> + Clone {
    prop_oneof![
        8 => amount(),
        1 => negative_amount(),
    ]
    .boxed()
}

/// Amounts at the edge of the `i128` range, for overflow checks.
pub fn extreme_amount() -> impl Strategy<Value = i128> {
    prop_oneof![
        Just(i128::MAX),
        Just(i128::MAX - 1),
        (i128::MAX / 2)..=i128::MAX,
    ]
}

// ─────────────────────────────────────────────────────────────────────────────
// Addresses
// ─────────────────────────────────────────────────────────────────────────────

const STRKEY_CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

fn strkey_body(len: usize) -> impl Strategy<Value = String> {
    proptest::collection::vec(proptest::sample::select(STRKEY_CHARSET), len)
        .prop_map(|v| String::from_utf8(v).expect("charset is ASCII"))
}

/// A strkey-shaped Stellar account address (`G` + 55 base32 characters).
pub fn account_strkey() -> impl Strategy<Value = String> {
    strkey_body(55).prop_map(|s| format!("G{}", s))
}

/// A strkey-shaped Soroban contract address (`C` + 55 base32 characters).
pub fn contract_strkey() -> impl Strategy<Value = String> {
    strkey_body(55).prop_map(|s| format!("C{}", s))
}

/// How a well-formed strkey can be corrupted. Each variant must be rejected
/// by address validation.
#[derive(Debug, Clone)]
pub enum StrkeyCorruption {
    Truncate(usize),
    Extend(String),
    Lowercase(usize),
    BadChar(usize, char),
    WrongPrefix(char),
}

/// A corruption to apply to a 56-character strkey.
pub fn strkey_corruption() -> impl Strategy<Value = StrkeyCorruption> {
    prop_oneof![
        (1usize..56).prop_map(StrkeyCorruption::Truncate),
        strkey_body(1).prop_map(StrkeyCorruption::Extend),
        (1usize..56).prop_map(StrkeyCorruption::Lowercase),
        (
            1usize..56,
            proptest::sample::select(vec!['0', '1', '8', '9', '=', '-', ' ', 'é'])
        )
            .prop_map(|(i, c)| StrkeyCorruption::BadChar(i, c)),
        proptest::sample::select(vec!['A', 'B', 'M', 'S', 'T', 'X', 'g', 'c'])
            .prop_map(StrkeyCorruption::WrongPrefix),
    ]
}

/// Apply `corruption` to a well-formed strkey.
pub fn corrupt_strkey(key: &str, corruption: &StrkeyCorruption) -> String {
    let mut chars: Vec<char> = key.chars().collect();
    match corruption {
        StrkeyCorruption::Truncate(n) => chars.truncate(*n),
        StrkeyCorruption::Extend(extra) => chars.extend(extra.chars()),
        StrkeyCorruption::Lowercase(i) => {
            // Force a letter so lowercasing always changes the key.
            chars[*i] = 'q';
        }
        StrkeyCorruption::BadChar(i, c) => chars[*i] = *c,
        StrkeyCorruption::WrongPrefix(c) => chars[0] = *c,
    }
    chars.into_iter().collect()
}

/// A mock account address from the shared `contract_mocks` helpers.
pub fn mock_account() -> impl Strategy<Value = MockAddress> {
    any::<u32>().prop_map(MockAddress::account)
}

/// A mock contract address from the shared `contract_mocks` helpers.
pub fn mock_contract() -> impl Strategy<Value = MockAddress> {
    any::<u32>().prop_map(MockAddress::contract)
}

/// An index into a pool of `actors` test identities. Tests map indices onto
/// concrete addresses (`Address::generate`, `MockAddress::account`, ...). A
/// small pool makes self-transfers and repeated spenders common.
pub fn actor(actors: usize) -> impl Strategy<Value = usize> {
    0..actors
}

// ─────────────────────────────────────────────────────────────────────────────
// Auth contexts
// ─────────────────────────────────────────────────────────────────────────────

/// The set of actors that signed (authorized) the current invocation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AuthContext {
    pub signers: BTreeSet<usize>,
}

impl AuthContext {
    pub fn signed_by(&self, actor: usize) -> bool {
        self.signers.contains(&actor)
    }
}

/// A random subset of the actor pool as signers, including the empty set
/// (nobody signed) and the full set (everybody signed).
pub fn auth_context(actors: usize) -> impl Strategy<Value = AuthContext> {
    prop_oneof![
        1 => Just(AuthContext::default()),
        1 => Just(AuthContext { signers: (0..actors).collect() }),
        4 => proptest::collection::btree_set(0..actors, 0..=actors)
            .prop_map(|signers| AuthContext { signers }),
    ]
}

// ─────────────────────────────────────────────────────────────────────────────
// Storage and upgrade patterns
// ─────────────────────────────────────────────────────────────────────────────

/// A storage key in one of the three Soroban durability scopes.
pub fn storage_key() -> impl Strategy<Value = StorageKey> {
    (0u8..3, "[A-Za-z][A-Za-z0-9_]{0,15}").prop_map(|(scope, key)| match scope {
        0 => StorageKey::instance(key),
        1 => StorageKey::persistent(key),
        _ => StorageKey::temporary(key),
    })
}

/// A minimal WASM module: valid magic + version header followed by an
/// arbitrary body. Suitable for upgrade/hash tests that only look at bytes.
pub fn wasm_module() -> impl Strategy<Value = Vec<u8>> {
    proptest::collection::vec(any::<u8>(), 0..256).prop_map(|body| {
        let mut wasm = b"\0asm\x01\0\0\0".to_vec();
        wasm.extend(body);
        wasm
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// SEP-41 token operations and reference model
// ─────────────────────────────────────────────────────────────────────────────

/// A state-changing SEP-41 token call. Actor fields are indices into the pool;
/// `Mint` is always authorized by the admin (see [`TokenModel::admin`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenOp {
    Mint {
        to: usize,
        amount: i128,
    },
    Transfer {
        from: usize,
        to: usize,
        amount: i128,
    },
    Approve {
        from: usize,
        spender: usize,
        amount: i128,
    },
    TransferFrom {
        spender: usize,
        from: usize,
        to: usize,
        amount: i128,
    },
    Burn {
        from: usize,
        amount: i128,
    },
    BurnFrom {
        spender: usize,
        from: usize,
        amount: i128,
    },
}

impl TokenOp {
    /// Contract function name for this operation.
    pub fn function(&self) -> &'static str {
        match self {
            TokenOp::Mint { .. } => "mint",
            TokenOp::Transfer { .. } => "transfer",
            TokenOp::Approve { .. } => "approve",
            TokenOp::TransferFrom { .. } => "transfer_from",
            TokenOp::Burn { .. } => "burn",
            TokenOp::BurnFrom { .. } => "burn_from",
        }
    }

    /// The single actor whose authorization this call requires.
    pub fn required_signer(&self, admin: usize) -> usize {
        match *self {
            TokenOp::Mint { .. } => admin,
            TokenOp::Transfer { from, .. }
            | TokenOp::Approve { from, .. }
            | TokenOp::Burn { from, .. } => from,
            TokenOp::TransferFrom { spender, .. } | TokenOp::BurnFrom { spender, .. } => spender,
        }
    }

    pub fn amount(&self) -> i128 {
        match *self {
            TokenOp::Mint { amount, .. }
            | TokenOp::Transfer { amount, .. }
            | TokenOp::Approve { amount, .. }
            | TokenOp::TransferFrom { amount, .. }
            | TokenOp::Burn { amount, .. }
            | TokenOp::BurnFrom { amount, .. } => amount,
        }
    }

    /// Whether the call changes the total supply when it succeeds.
    pub fn changes_supply(&self) -> bool {
        matches!(
            self,
            TokenOp::Mint { .. } | TokenOp::Burn { .. } | TokenOp::BurnFrom { .. }
        )
    }
}

/// A token operation over `actors` identities with amounts from `amounts`.
pub fn token_op_with(
    actors: usize,
    amounts: impl Strategy<Value = i128> + Clone + 'static,
) -> impl Strategy<Value = TokenOp> {
    let a = || actor(actors);
    prop_oneof![
        2 => (a(), amounts.clone()).prop_map(|(to, amount)| TokenOp::Mint { to, amount }),
        4 => (a(), a(), amounts.clone())
            .prop_map(|(from, to, amount)| TokenOp::Transfer { from, to, amount }),
        2 => (a(), a(), amounts.clone())
            .prop_map(|(from, spender, amount)| TokenOp::Approve { from, spender, amount }),
        3 => (a(), a(), a(), amounts.clone()).prop_map(|(spender, from, to, amount)| {
            TokenOp::TransferFrom { spender, from, to, amount }
        }),
        1 => (a(), amounts.clone()).prop_map(|(from, amount)| TokenOp::Burn { from, amount }),
        1 => (a(), a(), amounts)
            .prop_map(|(spender, from, amount)| TokenOp::BurnFrom { spender, from, amount }),
    ]
}

/// A token operation with mostly-valid, sometimes-negative amounts.
pub fn token_op(actors: usize) -> impl Strategy<Value = TokenOp> {
    token_op_with(actors, signed_amount())
}

/// A sequence of token operations of length `len`.
pub fn token_ops(
    actors: usize,
    len: std::ops::Range<usize>,
) -> impl Strategy<Value = Vec<TokenOp>> {
    proptest::collection::vec(token_op(actors), len)
}

/// Why the reference model rejected an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenError {
    NegativeAmount,
    Unauthorized,
    InsufficientBalance,
    InsufficientAllowance,
    Overflow,
}

/// Pure-Rust SEP-41 reference model mirroring
/// `templates/examples/sep41-token`. Operations are atomic: a rejected
/// operation leaves the model untouched.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenModel {
    pub admin: usize,
    pub balances: BTreeMap<usize, i128>,
    pub allowances: BTreeMap<(usize, usize), i128>,
    pub total_supply: i128,
}

impl TokenModel {
    pub fn new(admin: usize) -> Self {
        Self {
            admin,
            ..Self::default()
        }
    }

    pub fn balance(&self, who: usize) -> i128 {
        self.balances.get(&who).copied().unwrap_or(0)
    }

    pub fn allowance(&self, from: usize, spender: usize) -> i128 {
        self.allowances.get(&(from, spender)).copied().unwrap_or(0)
    }

    /// Sum of all balances; equals `total_supply` in every reachable state.
    pub fn sum_of_balances(&self) -> i128 {
        self.balances.values().sum()
    }

    /// Apply `op` under `auth`, failing with `Unauthorized` if the required
    /// signer did not sign.
    pub fn apply_with_auth(&mut self, op: &TokenOp, auth: &AuthContext) -> Result<(), TokenError> {
        if !auth.signed_by(op.required_signer(self.admin)) {
            return Err(TokenError::Unauthorized);
        }
        self.apply(op)
    }

    /// Apply `op` assuming the required signer authorized it.
    pub fn apply(&mut self, op: &TokenOp) -> Result<(), TokenError> {
        if op.amount() < 0 {
            return Err(TokenError::NegativeAmount);
        }
        let mut next = self.clone();
        match *op {
            TokenOp::Mint { to, amount } => {
                next.receive(to, amount)?;
                next.total_supply = next
                    .total_supply
                    .checked_add(amount)
                    .ok_or(TokenError::Overflow)?;
            }
            TokenOp::Transfer { from, to, amount } => {
                next.spend(from, amount)?;
                next.receive(to, amount)?;
            }
            TokenOp::Approve {
                from,
                spender,
                amount,
            } => {
                next.allowances.insert((from, spender), amount);
            }
            TokenOp::TransferFrom {
                spender,
                from,
                to,
                amount,
            } => {
                next.spend_allowance(from, spender, amount)?;
                next.spend(from, amount)?;
                next.receive(to, amount)?;
            }
            TokenOp::Burn { from, amount } => {
                next.spend(from, amount)?;
                next.total_supply -= amount;
            }
            TokenOp::BurnFrom {
                spender,
                from,
                amount,
            } => {
                next.spend_allowance(from, spender, amount)?;
                next.spend(from, amount)?;
                next.total_supply -= amount;
            }
        }
        *self = next;
        Ok(())
    }

    fn spend(&mut self, who: usize, amount: i128) -> Result<(), TokenError> {
        let bal = self.balance(who);
        if bal < amount {
            return Err(TokenError::InsufficientBalance);
        }
        self.balances.insert(who, bal - amount);
        Ok(())
    }

    fn receive(&mut self, who: usize, amount: i128) -> Result<(), TokenError> {
        let bal = self
            .balance(who)
            .checked_add(amount)
            .ok_or(TokenError::Overflow)?;
        self.balances.insert(who, bal);
        Ok(())
    }

    fn spend_allowance(
        &mut self,
        from: usize,
        spender: usize,
        amount: i128,
    ) -> Result<(), TokenError> {
        let allowance = self.allowance(from, spender);
        if allowance < amount {
            return Err(TokenError::InsufficientAllowance);
        }
        self.allowances.insert((from, spender), allowance - amount);
        Ok(())
    }
}
