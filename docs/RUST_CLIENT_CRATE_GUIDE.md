# Rust Client Crate Generation Guide

StarForge can generate complete, first-party, strongly typed Rust client crates from compiled Soroban contract interfaces (`.wasm`).

First-party Rust clients enable type-safe integration tests, downstream application tooling, and microservices that share encoding logic with StarForge itself.

---

## 1. Quickstart

### Generate a Client Crate
```bash
starforge contract generate-bindings ./contracts/test_token.wasm \
  --lang rust \
  --crate-dir ./crates/test-token-client \
  --crate-name test-token-client
```

### Generate a `no_std` WASM-Friendly Client Crate
```bash
starforge contract generate-bindings ./contracts/test_token.wasm \
  --lang rust \
  --crate-dir ./crates/test-token-client \
  --crate-name test-token-client \
  --no-std
```

---

## 2. Generated Crate Layout

```text
crates/test-token-client/
├── Cargo.toml      # Package manifest with network backend & environment feature flags
├── README.md       # Crate docs, pinned SDK documentation, function index, quickstart
└── src/
    └── lib.rs      # Type-safe client, method invocations, shared argument encoding, types
```

---

## 3. Pinned Dependencies & Versioning Policy

### Pinned Versions
To prevent wire-level encoding mismatch and silent ABI drift between Soroban contracts and client applications, generated client crates are pinned against exact dependency versions:

- **`soroban-sdk`**: `=22.0.0`
- **`stellar-xdr`**: `=22.0.0`

### Versioning Policy
Generated client crates follow [Semantic Versioning](https://semver.org/):
- **Patch updates** (`0.1.x`): Bug fixes and non-breaking internal improvements.
- **Minor updates** (`0.x.0`): Additive contract functions, non-breaking struct fields.
- **Major updates** (`x.0.0`): Breaking contract interface changes, renamed methods, or Soroban protocol version upgrades.

---

## 4. Feature Flags

| Feature | Default | Description |
|---|---|---|
| `std` | **Yes** | Standard library support with `std::error::Error`, CLI subprocess execution, and string formatting. |
| `no_std` | No | Zero-allocation / embedded / WASM-client compatibility without standard library dependencies. |
| `cli-backend` | Yes (in `std`) | Invokes contract functions via the StarForge CLI command runner (`starforge contract invoke`). |
| `rpc-backend` | No | Direct asynchronous JSON-RPC Soroban network client. |
| `testutils` | No | In-memory Soroban test environment integration with `soroban-sdk/testutils`. |

---

## 5. Shared Encoding Helpers

The generated client includes helper functions that reuse StarForge argument serialization rules:

- `build_cli_args(&self, function: &str, args: &[(&str, &str)]) -> Vec<String>`: Constructs consistent CLI arguments matching `starforge contract invoke`.
- `serialize_arg<T: Display>(&self, value: &T) -> Result<String>`: Serializes typed values into Soroban CLI parameter format.
- `parse_result<T: FromStr>(&self, result: &str) -> Result<T>`: Deserializes contract return strings into typed Rust outputs.

---

## 6. Integration Testing Example

```rust,no_run
use test_token_client::{ContractClient, TokenMetadata};

#[test]
fn test_contract_client_integration() {
    let client = ContractClient::new("CA...", "testnet")
        .with_wallet("alice");

    // Type-safe argument passing and return types
    let balance = client.balance_of("G...".to_string()).expect("failed to get balance");
    println!("Balance: {}", balance);
}
```
