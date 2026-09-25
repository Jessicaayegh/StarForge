//! Soroban host function table.
//!
//! Soroban contracts reach the host through imports with one-letter module
//! names and short export names (for example `("l", "1")` is
//! `get_contract_data`). This table is transcribed from `env.json` in
//! `soroban-env-common` 22.x, the interface definition that both the SDK and
//! the host are generated from, so the analyzer can name every host call a
//! contract makes and attribute it to the resource it consumes.

use serde::{Deserialize, Serialize};

/// The resource family a host function draws on. Drives both the static cost
/// index and the suggestion engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostCategory {
    /// Ledger reads (`get_contract_data`, `has_contract_data`): each distinct
    /// key is a footprint read entry plus read bytes.
    StorageRead,
    /// Ledger writes and deletes: each distinct key is a write entry plus
    /// write bytes, the most expensive per-byte resource.
    StorageWrite,
    /// TTL extensions: rent payments on persistent/instance entries.
    StorageTtl,
    /// Contract / code deployment and upgrades.
    Deploy,
    /// Cross-contract calls (`call`, `try_call`): a VM instantiation of the
    /// callee plus its own footprint.
    CrossContract,
    /// Hashing, signature verification and BLS12-381 operations (CPU heavy).
    Crypto,
    /// `require_auth*` and related authorization checks.
    Auth,
    /// `contract_event`: contributes to the events resource fee.
    Event,
    /// `log_from_linear_memory`: diagnostic logging (debug builds only).
    Log,
    /// XDR (de)serialization of values to/from `Bytes`.
    Serialization,
    /// Host object manipulation (Vec, Map, Bytes, String, big integers).
    Object,
    /// Ledger/context information and error signalling.
    Context,
    /// Pseudo-random number generation.
    Prng,
    /// Test-only host functions.
    Test,
    /// An import that is not part of the Soroban 22 interface.
    Unknown,
}

impl HostCategory {
    /// Relative weight used by the static cost index. These are ordinal
    /// weights, not instruction counts: they encode that a storage write is
    /// far more expensive than a vector push, which is what matters when
    /// ranking functions against each other.
    pub fn weight(self) -> u64 {
        match self {
            HostCategory::StorageWrite => 400,
            HostCategory::CrossContract => 400,
            HostCategory::Deploy => 400,
            HostCategory::StorageRead => 200,
            HostCategory::StorageTtl => 150,
            HostCategory::Crypto => 150,
            HostCategory::Auth => 100,
            HostCategory::Event => 60,
            HostCategory::Serialization => 40,
            HostCategory::Prng => 20,
            HostCategory::Log => 10,
            HostCategory::Object => 10,
            HostCategory::Context => 5,
            HostCategory::Test => 1,
            HostCategory::Unknown => 50,
        }
    }

    /// True for categories whose use inside a loop is worth flagging.
    pub fn is_expensive(self) -> bool {
        matches!(
            self,
            HostCategory::StorageRead
                | HostCategory::StorageWrite
                | HostCategory::StorageTtl
                | HostCategory::CrossContract
                | HostCategory::Crypto
                | HostCategory::Deploy
                | HostCategory::Event
                | HostCategory::Auth
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            HostCategory::StorageRead => "storage-read",
            HostCategory::StorageWrite => "storage-write",
            HostCategory::StorageTtl => "storage-ttl",
            HostCategory::Deploy => "deploy",
            HostCategory::CrossContract => "cross-contract",
            HostCategory::Crypto => "crypto",
            HostCategory::Auth => "auth",
            HostCategory::Event => "event",
            HostCategory::Log => "log",
            HostCategory::Serialization => "serialization",
            HostCategory::Object => "object",
            HostCategory::Context => "context",
            HostCategory::Prng => "prng",
            HostCategory::Test => "test",
            HostCategory::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for HostCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// `(module, export, name)` for every host function in the Soroban 22
/// interface.
pub const HOST_FUNCTIONS: &[(&str, &str, &str)] = &[
    ("x", "_", "log_from_linear_memory"),
    ("x", "0", "obj_cmp"),
    ("x", "1", "contract_event"),
    ("x", "2", "get_ledger_version"),
    ("x", "3", "get_ledger_sequence"),
    ("x", "4", "get_ledger_timestamp"),
    ("x", "5", "fail_with_error"),
    ("x", "6", "get_ledger_network_id"),
    ("x", "7", "get_current_contract_address"),
    ("x", "8", "get_max_live_until_ledger"),
    ("i", "_", "obj_from_u64"),
    ("i", "0", "obj_to_u64"),
    ("i", "1", "obj_from_i64"),
    ("i", "2", "obj_to_i64"),
    ("i", "3", "obj_from_u128_pieces"),
    ("i", "4", "obj_to_u128_lo64"),
    ("i", "5", "obj_to_u128_hi64"),
    ("i", "6", "obj_from_i128_pieces"),
    ("i", "7", "obj_to_i128_lo64"),
    ("i", "8", "obj_to_i128_hi64"),
    ("i", "9", "obj_from_u256_pieces"),
    ("i", "a", "u256_val_from_be_bytes"),
    ("i", "b", "u256_val_to_be_bytes"),
    ("i", "c", "obj_to_u256_hi_hi"),
    ("i", "d", "obj_to_u256_hi_lo"),
    ("i", "e", "obj_to_u256_lo_hi"),
    ("i", "f", "obj_to_u256_lo_lo"),
    ("i", "g", "obj_from_i256_pieces"),
    ("i", "h", "i256_val_from_be_bytes"),
    ("i", "i", "i256_val_to_be_bytes"),
    ("i", "j", "obj_to_i256_hi_hi"),
    ("i", "k", "obj_to_i256_hi_lo"),
    ("i", "l", "obj_to_i256_lo_hi"),
    ("i", "m", "obj_to_i256_lo_lo"),
    ("i", "n", "u256_add"),
    ("i", "o", "u256_sub"),
    ("i", "p", "u256_mul"),
    ("i", "q", "u256_div"),
    ("i", "r", "u256_rem_euclid"),
    ("i", "s", "u256_pow"),
    ("i", "t", "u256_shl"),
    ("i", "u", "u256_shr"),
    ("i", "v", "i256_add"),
    ("i", "w", "i256_sub"),
    ("i", "x", "i256_mul"),
    ("i", "y", "i256_div"),
    ("i", "z", "i256_rem_euclid"),
    ("i", "A", "i256_pow"),
    ("i", "B", "i256_shl"),
    ("i", "C", "i256_shr"),
    ("i", "D", "timepoint_obj_from_u64"),
    ("i", "E", "timepoint_obj_to_u64"),
    ("i", "F", "duration_obj_from_u64"),
    ("i", "G", "duration_obj_to_u64"),
    ("m", "_", "map_new"),
    ("m", "0", "map_put"),
    ("m", "1", "map_get"),
    ("m", "2", "map_del"),
    ("m", "3", "map_len"),
    ("m", "4", "map_has"),
    ("m", "5", "map_key_by_pos"),
    ("m", "6", "map_val_by_pos"),
    ("m", "7", "map_keys"),
    ("m", "8", "map_values"),
    ("m", "9", "map_new_from_linear_memory"),
    ("m", "a", "map_unpack_to_linear_memory"),
    ("v", "_", "vec_new"),
    ("v", "0", "vec_put"),
    ("v", "1", "vec_get"),
    ("v", "2", "vec_del"),
    ("v", "3", "vec_len"),
    ("v", "4", "vec_push_front"),
    ("v", "5", "vec_pop_front"),
    ("v", "6", "vec_push_back"),
    ("v", "7", "vec_pop_back"),
    ("v", "8", "vec_front"),
    ("v", "9", "vec_back"),
    ("v", "a", "vec_insert"),
    ("v", "b", "vec_append"),
    ("v", "c", "vec_slice"),
    ("v", "d", "vec_first_index_of"),
    ("v", "e", "vec_last_index_of"),
    ("v", "f", "vec_binary_search"),
    ("v", "g", "vec_new_from_linear_memory"),
    ("v", "h", "vec_unpack_to_linear_memory"),
    ("l", "_", "put_contract_data"),
    ("l", "0", "has_contract_data"),
    ("l", "1", "get_contract_data"),
    ("l", "2", "del_contract_data"),
    ("l", "3", "create_contract"),
    ("l", "4", "create_asset_contract"),
    ("l", "5", "upload_wasm"),
    ("l", "6", "update_current_contract_wasm"),
    ("l", "7", "extend_contract_data_ttl"),
    ("l", "8", "extend_current_contract_instance_and_code_ttl"),
    ("l", "9", "extend_contract_instance_and_code_ttl"),
    ("l", "a", "get_contract_id"),
    ("l", "b", "get_asset_contract_id"),
    ("l", "c", "extend_contract_instance_ttl"),
    ("l", "d", "extend_contract_code_ttl"),
    ("l", "e", "create_contract_with_constructor"),
    ("d", "_", "call"),
    ("d", "0", "try_call"),
    ("b", "_", "serialize_to_bytes"),
    ("b", "0", "deserialize_from_bytes"),
    ("b", "1", "bytes_copy_to_linear_memory"),
    ("b", "2", "bytes_copy_from_linear_memory"),
    ("b", "3", "bytes_new_from_linear_memory"),
    ("b", "4", "bytes_new"),
    ("b", "5", "bytes_put"),
    ("b", "6", "bytes_get"),
    ("b", "7", "bytes_del"),
    ("b", "8", "bytes_len"),
    ("b", "9", "bytes_push"),
    ("b", "a", "bytes_pop"),
    ("b", "b", "bytes_front"),
    ("b", "c", "bytes_back"),
    ("b", "d", "bytes_insert"),
    ("b", "e", "bytes_append"),
    ("b", "f", "bytes_slice"),
    ("b", "g", "string_copy_to_linear_memory"),
    ("b", "h", "symbol_copy_to_linear_memory"),
    ("b", "i", "string_new_from_linear_memory"),
    ("b", "j", "symbol_new_from_linear_memory"),
    ("b", "k", "string_len"),
    ("b", "l", "symbol_len"),
    ("b", "m", "symbol_index_in_linear_memory"),
    ("c", "_", "compute_hash_sha256"),
    ("c", "0", "verify_sig_ed25519"),
    ("c", "1", "compute_hash_keccak256"),
    ("c", "2", "recover_key_ecdsa_secp256k1"),
    ("c", "3", "verify_sig_ecdsa_secp256r1"),
    ("c", "4", "bls12_381_check_g1_is_in_subgroup"),
    ("c", "5", "bls12_381_g1_add"),
    ("c", "6", "bls12_381_g1_mul"),
    ("c", "7", "bls12_381_g1_msm"),
    ("c", "8", "bls12_381_map_fp_to_g1"),
    ("c", "9", "bls12_381_hash_to_g1"),
    ("c", "a", "bls12_381_check_g2_is_in_subgroup"),
    ("c", "b", "bls12_381_g2_add"),
    ("c", "c", "bls12_381_g2_mul"),
    ("c", "d", "bls12_381_g2_msm"),
    ("c", "e", "bls12_381_map_fp2_to_g2"),
    ("c", "f", "bls12_381_hash_to_g2"),
    ("c", "g", "bls12_381_multi_pairing_check"),
    ("c", "h", "bls12_381_fr_add"),
    ("c", "i", "bls12_381_fr_sub"),
    ("c", "j", "bls12_381_fr_mul"),
    ("c", "k", "bls12_381_fr_pow"),
    ("c", "l", "bls12_381_fr_inv"),
    ("a", "_", "require_auth_for_args"),
    ("a", "0", "require_auth"),
    ("a", "1", "strkey_to_address"),
    ("a", "2", "address_to_strkey"),
    ("a", "3", "authorize_as_curr_contract"),
    ("t", "_", "dummy0"),
    ("t", "0", "protocol_gated_dummy"),
    ("p", "_", "prng_reseed"),
    ("p", "0", "prng_bytes_new"),
    ("p", "1", "prng_u64_in_inclusive_range"),
    ("p", "2", "prng_vec_shuffle"),
];

/// Resolve an import to its host function name, if it is a known Soroban
/// host function.
pub fn host_function_name(module: &str, field: &str) -> Option<&'static str> {
    HOST_FUNCTIONS
        .iter()
        .find(|(m, e, _)| *m == module && *e == field)
        .map(|(_, _, n)| *n)
}

/// Classify an import by the resource it consumes.
pub fn categorize(module: &str, field: &str) -> HostCategory {
    let name = match host_function_name(module, field) {
        Some(n) => n,
        None => return HostCategory::Unknown,
    };
    match module {
        "l" => match name {
            "get_contract_data" | "has_contract_data" => HostCategory::StorageRead,
            "put_contract_data" | "del_contract_data" => HostCategory::StorageWrite,
            n if n.starts_with("extend_") => HostCategory::StorageTtl,
            "get_contract_id" | "get_asset_contract_id" => HostCategory::Context,
            _ => HostCategory::Deploy,
        },
        "d" => HostCategory::CrossContract,
        "c" => HostCategory::Crypto,
        "a" => match name {
            "require_auth" | "require_auth_for_args" | "authorize_as_curr_contract" => {
                HostCategory::Auth
            }
            _ => HostCategory::Object,
        },
        "x" => match name {
            "contract_event" => HostCategory::Event,
            "log_from_linear_memory" => HostCategory::Log,
            _ => HostCategory::Context,
        },
        "b" => match name {
            "serialize_to_bytes" | "deserialize_from_bytes" => HostCategory::Serialization,
            _ => HostCategory::Object,
        },
        "i" | "m" | "v" => HostCategory::Object,
        "p" => HostCategory::Prng,
        "t" => HostCategory::Test,
        _ => HostCategory::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_ledger_functions() {
        assert_eq!(host_function_name("l", "1"), Some("get_contract_data"));
        assert_eq!(host_function_name("l", "_"), Some("put_contract_data"));
        assert_eq!(categorize("l", "1"), HostCategory::StorageRead);
        assert_eq!(categorize("l", "0"), HostCategory::StorageRead);
        assert_eq!(categorize("l", "_"), HostCategory::StorageWrite);
        assert_eq!(categorize("l", "2"), HostCategory::StorageWrite);
        assert_eq!(categorize("l", "7"), HostCategory::StorageTtl);
        assert_eq!(categorize("l", "3"), HostCategory::Deploy);
    }

    #[test]
    fn resolves_other_modules() {
        assert_eq!(categorize("d", "_"), HostCategory::CrossContract);
        assert_eq!(categorize("x", "1"), HostCategory::Event);
        assert_eq!(categorize("x", "_"), HostCategory::Log);
        assert_eq!(categorize("a", "0"), HostCategory::Auth);
        assert_eq!(categorize("c", "0"), HostCategory::Crypto);
        assert_eq!(categorize("b", "0"), HostCategory::Serialization);
        assert_eq!(categorize("v", "6"), HostCategory::Object);
    }

    #[test]
    fn unknown_imports_are_flagged() {
        assert_eq!(categorize("env", "abort"), HostCategory::Unknown);
        assert_eq!(categorize("l", "zz"), HostCategory::Unknown);
    }

    #[test]
    fn table_has_no_duplicate_keys() {
        let mut seen = std::collections::HashSet::new();
        for (m, e, _) in HOST_FUNCTIONS {
            assert!(seen.insert((*m, *e)), "duplicate host fn {}.{}", m, e);
        }
        assert!(HOST_FUNCTIONS.len() > 150);
    }
}
