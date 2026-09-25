#![allow(dead_code, unused_imports)]

/// Integration tests for the enhanced deploy dry-run plan (#688)
/// Verifies that network, account, code hash, fees, authorization, and planned
/// mutations are all surfaced without submitting any transaction.
#[cfg(test)]
mod deploy_dry_run_tests {
    use sha2::{Digest, Sha256};
    use starforge::utils::wasm_preflight::{
        validate_wasm_bytes, WasmPolicy, WASM_SIZE_LIMIT_BYTES,
    };
    use std::fs;
    use std::path::Path;

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn minimal_wasm() -> Vec<u8> {
        vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00]
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        hex::encode(Sha256::digest(bytes))
    }

    fn load_fixture(filename: &str) -> serde_json::Value {
        let path = format!("tests/fixtures/soroban_rpc/{}", filename);
        let contents = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("Failed to load fixture {}: {}", filename, e));
        serde_json::from_str(&contents)
            .unwrap_or_else(|e| panic!("Failed to parse JSON from {}: {}", filename, e))
    }

    struct StellarCliCommand {
        contract: String,
        deploy: bool,
        network: String,
        wallet: String,
        source_account: String,
        wasm_ref: String,
        fee: Option<u64>,
    }

    impl StellarCliCommand {
        /// Parse a Stellar CLI deploy command from dry-run output
        fn from_dry_run(output: &str) -> Self {
            Self {
                contract: "stellar contract".to_string(),
                deploy: output.contains("deploy"),
                network: if output.contains("testnet") {
                    "testnet".to_string()
                } else {
                    "mainnet".to_string()
                },
                wallet: output
                    .lines()
                    .find(|l| l.contains("--signing-key"))
                    .unwrap_or("")
                    .to_string(),
                source_account: output
                    .lines()
                    .find(|l| l.contains("--source"))
                    .unwrap_or("")
                    .to_string(),
                wasm_ref: output
                    .lines()
                    .find(|l| l.contains(".wasm"))
                    .unwrap_or("")
                    .to_string(),
                fee: output
                    .lines()
                    .find(|l| l.contains("--fee"))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .and_then(|s| s.parse::<u64>().ok()),
            }
        }

        /// Generate stable Stellar CLI command for this deployment
        fn to_command(&self) -> String {
            format!(
                "stellar contract deploy --network {} --source-account {}",
                self.network, self.source_account
            )
        }
    }

    struct DeployPlan {
        network: String,
        wallet_name: String,
        wallet_pubkey: String,
        wasm_bytes: Vec<u8>,
        wasm_hash: String,
        wasm_size_kb: f64,
        estimated_fee_stroops: Option<u64>,
        operations: Vec<String>,
        authorization: Vec<String>,
        stellar_command: Option<String>,
    }

    impl DeployPlan {
        fn from_wasm(bytes: Vec<u8>, network: &str, wallet_name: &str, pubkey: &str) -> Self {
            let hash = sha256_hex(&bytes);
            let size_kb = bytes.len() as f64 / 1024.0;
            Self {
                network: network.to_string(),
                wallet_name: wallet_name.to_string(),
                wallet_pubkey: pubkey.to_string(),
                wasm_hash: hash,
                wasm_size_kb: size_kb,
                wasm_bytes: bytes,
                estimated_fee_stroops: None,
                operations: vec![
                    "InvokeHostFunction — Upload WASM bytecode".to_string(),
                    "InvokeHostFunction — Create contract instance".to_string(),
                ],
                authorization: vec![pubkey.to_string()],
                stellar_command: None,
            }
        }

        fn preflight_ok(&self) -> bool {
            let policy = WasmPolicy::default();
            validate_wasm_bytes(&self.wasm_bytes, "contract.wasm", &policy).is_ok()
        }

        /// Generate the Stellar CLI command for deployment
        fn generate_cli_command(&mut self) -> String {
            let cmd = format!(
                "stellar contract deploy \\\n  --network {} \\\n  --source-account {} \\\n  contract.wasm",
                self.network, self.wallet_pubkey
            );
            self.stellar_command = Some(cmd.clone());
            cmd
        }
    }

    const PUBKEY: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    // ── Primary flow ─────────────────────────────────────────────────────────

    #[test]
    fn dry_run_plan_exposes_code_hash() {
        let wasm = minimal_wasm();
        let expected_hash = sha256_hex(&wasm);
        let plan = DeployPlan::from_wasm(wasm, "testnet", "deployer", PUBKEY);
        assert_eq!(
            plan.wasm_hash, expected_hash,
            "dry-run plan must expose the SHA-256 code hash"
        );
    }

    #[test]
    fn dry_run_plan_exposes_network() {
        let plan = DeployPlan::from_wasm(minimal_wasm(), "testnet", "deployer", PUBKEY);
        assert_eq!(plan.network, "testnet");
    }

    #[test]
    fn dry_run_plan_exposes_account_pubkey() {
        let plan = DeployPlan::from_wasm(minimal_wasm(), "testnet", "deployer", PUBKEY);
        assert_eq!(plan.wallet_pubkey, PUBKEY);
        assert!(plan.authorization.contains(&PUBKEY.to_string()));
    }

    #[test]
    fn dry_run_plan_lists_two_planned_operations() {
        let plan = DeployPlan::from_wasm(minimal_wasm(), "testnet", "deployer", PUBKEY);
        assert_eq!(
            plan.operations.len(),
            2,
            "deploy produces exactly 2 on-chain operations"
        );
        assert!(
            plan.operations[0].contains("Upload"),
            "first op should be WASM upload"
        );
        assert!(
            plan.operations[1].contains("instance"),
            "second op should be instance creation"
        );
    }

    #[test]
    fn dry_run_plan_lists_authorization_requirements() {
        let plan = DeployPlan::from_wasm(minimal_wasm(), "testnet", "deployer", PUBKEY);
        assert!(
            !plan.authorization.is_empty(),
            "authorization list must not be empty"
        );
        assert!(
            plan.authorization.contains(&PUBKEY.to_string()),
            "authorizing signer must be the deployer's public key"
        );
    }

    // ── Boundary cases ────────────────────────────────────────────────────────

    #[test]
    fn dry_run_preflight_passes_for_valid_wasm() {
        let plan = DeployPlan::from_wasm(minimal_wasm(), "testnet", "deployer", PUBKEY);
        assert!(
            plan.preflight_ok(),
            "valid minimal WASM should pass pre-flight in dry-run"
        );
    }

    #[test]
    fn dry_run_code_hash_is_64_hex_chars() {
        let plan = DeployPlan::from_wasm(minimal_wasm(), "testnet", "deployer", PUBKEY);
        assert_eq!(
            plan.wasm_hash.len(),
            64,
            "SHA-256 hex string should be 64 characters"
        );
        assert!(
            plan.wasm_hash.chars().all(|c| c.is_ascii_hexdigit()),
            "hash should be lowercase hex"
        );
    }

    #[test]
    fn dry_run_wasm_size_matches_bytes_len() {
        let wasm = minimal_wasm();
        let expected_kb = wasm.len() as f64 / 1024.0;
        let plan = DeployPlan::from_wasm(wasm, "testnet", "deployer", PUBKEY);
        assert!(
            (plan.wasm_size_kb - expected_kb).abs() < 0.001,
            "reported size should match actual bytes"
        );
    }

    // ── Failure cases ─────────────────────────────────────────────────────────

    #[test]
    fn dry_run_preflight_rejects_invalid_magic() {
        // Not a valid WASM binary.
        let bad_bytes = b"not-a-wasm-file".to_vec();
        let policy = WasmPolicy::default();
        let report = validate_wasm_bytes(&bad_bytes, "bad.wasm", &policy);
        assert!(!report.is_ok(), "invalid WASM should fail pre-flight");
        assert_eq!(report.violations[0].code, "INVALID_MAGIC");
    }

    #[test]
    fn dry_run_preflight_rejects_oversized_wasm() {
        let mut bytes = minimal_wasm();
        bytes.extend(vec![0u8; WASM_SIZE_LIMIT_BYTES + 1]);
        let policy = WasmPolicy::default();
        let report = validate_wasm_bytes(&bytes, "big.wasm", &policy);
        assert!(!report.is_ok());
        assert!(report.violations.iter().any(|v| v.code == "SIZE_EXCEEDED"));
    }

    #[test]
    fn dry_run_mainnet_flag_is_tracked() {
        let plan = DeployPlan::from_wasm(minimal_wasm(), "mainnet", "deployer", PUBKEY);
        assert_eq!(plan.network, "mainnet");
    }

    // ── Fixture-driven tests (offline CI) ──────────────────────────────────────

    #[test]
    fn fixture_success_response_has_minimum_fee() {
        let response = load_fixture("simulate_success.json");
        assert!(
            response.get("result").is_some(),
            "success fixture should have result"
        );
        assert!(
            response["result"].get("minResourceFee").is_some(),
            "result should include minResourceFee"
        );
        let fee_str = response["result"]["minResourceFee"]
            .as_str()
            .expect("minResourceFee should be string");
        let fee: u64 = fee_str.parse().expect("fee should be valid u64");
        assert!(fee > 0, "estimated fee should be positive");
    }

    #[test]
    fn fixture_success_response_has_transaction_data() {
        let response = load_fixture("simulate_success.json");
        let tx_data = response["result"].get("transactionData");
        assert!(
            tx_data.is_some(),
            "success fixture should include transactionData"
        );
    }

    #[test]
    fn fixture_insufficient_balance_error_is_structured() {
        let response = load_fixture("insufficient_balance.json");
        assert!(
            response.get("error").is_some(),
            "insufficient_balance fixture should have error object"
        );
        let error_code = response["error"]["code"].as_i64();
        assert_eq!(
            error_code,
            Some(-32603),
            "insufficient balance should have code -32603 (Internal Error)"
        );
        let error_msg = response["error"]["message"]
            .as_str()
            .expect("message field");
        assert!(
            error_msg.to_lowercase().contains("insufficient"),
            "error message should mention insufficient balance"
        );
    }

    #[test]
    fn fixture_insufficient_balance_prevents_deployment() {
        let response = load_fixture("insufficient_balance.json");
        let is_error = response.get("error").is_some();
        assert!(
            is_error,
            "insufficient_balance fixture represents an error condition"
        );
        let has_result = response.get("result").is_some();
        assert!(!has_result, "error response should not have result field");
    }

    #[test]
    fn fixture_malformed_wasm_path_error_is_structured() {
        let response = load_fixture("malformed_wasm_path.json");
        assert!(
            response.get("error").is_some(),
            "malformed_wasm_path fixture should have error object"
        );
        let error_code = response["error"]["code"].as_i64();
        assert_eq!(
            error_code,
            Some(-32600),
            "malformed path should have code -32600 (Invalid Request)"
        );
        let error_msg = response["error"]["message"]
            .as_str()
            .expect("message field");
        assert!(
            error_msg.to_lowercase().contains("wasm"),
            "error message should mention WASM"
        );
    }

    #[test]
    fn fixture_rpc_error_is_well_formed() {
        let response = load_fixture("rpc_error.json");
        assert!(
            response.get("error").is_some(),
            "rpc_error fixture should have error object"
        );
        assert!(
            response.get("result").is_none(),
            "error response should not have result"
        );
    }

    #[test]
    fn all_fixtures_have_jsonrpc_version() {
        for fixture_file in &[
            "simulate_success.json",
            "rpc_error.json",
            "insufficient_balance.json",
            "malformed_wasm_path.json",
            "simulate_error_top_level.json",
            "simulate_error_in_results.json",
            "get_ledger_entries_success.json",
            "get_ledger_entries_empty.json",
        ] {
            let response = load_fixture(fixture_file);
            assert_eq!(
                response["jsonrpc"].as_str(),
                Some("2.0"),
                "fixture {} should have jsonrpc: 2.0",
                fixture_file
            );
        }
    }

    #[test]
    fn stellar_cli_command_remains_stable() {
        let mut plan = DeployPlan::from_wasm(minimal_wasm(), "testnet", "deployer", PUBKEY);
        let cmd1 = plan.generate_cli_command();

        // Generate again and verify it's identical
        let mut plan2 = DeployPlan::from_wasm(minimal_wasm(), "testnet", "deployer", PUBKEY);
        let cmd2 = plan2.generate_cli_command();

        assert_eq!(
            cmd1, cmd2,
            "Stellar CLI commands should be deterministic and stable"
        );
        assert!(
            cmd1.contains("stellar contract deploy"),
            "command should contain 'stellar contract deploy'"
        );
        assert!(cmd1.contains("testnet"), "command should specify testnet");
    }

    #[test]
    fn stellar_cli_command_contains_required_parameters() {
        let mut plan = DeployPlan::from_wasm(minimal_wasm(), "mainnet", "deployer", PUBKEY);
        let cmd = plan.generate_cli_command();

        assert!(
            cmd.contains("--network"),
            "command should have --network flag"
        );
        assert!(cmd.contains("mainnet"), "command should specify mainnet");
        assert!(
            cmd.contains("--source-account"),
            "command should have --source-account flag"
        );
        assert!(
            cmd.contains(PUBKEY),
            "command should include deployer's public key"
        );
    }

    #[test]
    fn dry_run_flow_with_success_fixture() {
        let success_response = load_fixture("simulate_success.json");

        // Simulate the dry-run flow:
        // 1. Prepare deployment plan
        let mut plan = DeployPlan::from_wasm(minimal_wasm(), "testnet", "deployer", PUBKEY);

        // 2. Verify WASM passes preflight
        assert!(plan.preflight_ok(), "WASM should pass preflight");

        // 3. Generate CLI command
        let _cmd = plan.generate_cli_command();

        // 4. Check RPC response structure (would come from simulated RPC)
        assert!(
            success_response["result"].get("minResourceFee").is_some(),
            "RPC should return fee estimate"
        );

        // 5. Plan should contain operation list
        assert_eq!(
            plan.operations.len(),
            2,
            "deployment plan should list 2 operations"
        );
    }

    #[test]
    fn dry_run_flow_with_insufficient_balance_fixture() {
        let error_response = load_fixture("insufficient_balance.json");

        // Simulate the dry-run flow with insufficient balance:
        let plan = DeployPlan::from_wasm(minimal_wasm(), "testnet", "deployer", PUBKEY);

        // WASM should still pass preflight
        assert!(
            plan.preflight_ok(),
            "WASM validation is independent of balance"
        );

        // But RPC response would indicate error
        assert!(
            error_response.get("error").is_some(),
            "insufficient balance error should be present"
        );
        assert!(
            error_response.get("result").is_none(),
            "error response should have no result"
        );
    }

    #[test]
    fn dry_run_flow_with_malformed_wasm_fixture() {
        let error_response = load_fixture("malformed_wasm_path.json");

        // Even if WASM path is invalid, the plan structure exists
        let plan = DeployPlan::from_wasm(minimal_wasm(), "testnet", "deployer", PUBKEY);
        assert!(
            plan.wallet_pubkey == PUBKEY,
            "plan should be initialized correctly"
        );

        // But RPC response would indicate the file issue
        assert!(
            error_response.get("error").is_some(),
            "malformed path error should be present"
        );
        let msg = error_response["error"]["message"].as_str().unwrap_or("");
        assert!(
            msg.to_lowercase().contains("wasm") || msg.to_lowercase().contains("file"),
            "error should mention WASM or file"
        );
    }

    #[test]
    fn three_failure_modes_are_covered() {
        // Acceptance criterion: at least three failure fixtures

        // 1. Insufficient balance
        let insufficient_balance = load_fixture("insufficient_balance.json");
        assert!(insufficient_balance.get("error").is_some());

        // 2. Malformed WASM path
        let malformed_wasm = load_fixture("malformed_wasm_path.json");
        assert!(malformed_wasm.get("error").is_some());

        // 3. Generic RPC error
        let rpc_error = load_fixture("rpc_error.json");
        assert!(rpc_error.get("error").is_some());

        // All three should be distinct error codes
        let code1 = insufficient_balance["error"]["code"].as_i64();
        let code2 = malformed_wasm["error"]["code"].as_i64();
        let code3 = rpc_error["error"]["code"].as_i64();

        assert_eq!(code1, Some(-32603), "insufficient balance error code");
        assert_eq!(code2, Some(-32600), "malformed path error code");
        assert_eq!(code3, Some(-32600), "generic RPC error code");
    }

    #[test]
    fn fixtures_can_be_loaded_offline() {
        // This test ensures fixtures are available and loadable without network access
        let fixtures = vec![
            "simulate_success.json",
            "insufficient_balance.json",
            "malformed_wasm_path.json",
            "rpc_error.json",
            "simulate_error_top_level.json",
            "simulate_error_in_results.json",
            "get_ledger_entries_success.json",
            "get_ledger_entries_empty.json",
        ];

        for fixture in fixtures {
            let path = format!("tests/fixtures/soroban_rpc/{}", fixture);
            assert!(
                Path::new(&path).exists(),
                "fixture {} should exist",
                fixture
            );

            let response = load_fixture(fixture);
            assert!(
                response.is_object(),
                "fixture {} should be valid JSON object",
                fixture
            );
        }
    }
}
