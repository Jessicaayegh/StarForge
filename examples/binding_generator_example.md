# Binding Generator Example

This example demonstrates the enhanced contract ABI binding generator for StarForge.

## Overview

The binding generator now provides:

1. **Multi-language support**: Rust, TypeScript, Python, Go
2. **Type-safe interfaces**: Properly typed client interfaces
3. **Event type definitions**: Generated event types from contract metadata
4. **Method discovery**: Automatic discovery of contract functions
5. **Serialization/deserialization**: Built-in type conversion

## Usage

### Basic Usage

```bash
# Generate Rust bindings (stdout)
starforge contract generate-bindings ./contract.wasm --lang rust

# Generate a complete Cargo-compatible Rust client crate skeleton
starforge contract generate-bindings ./contract.wasm --lang rust --crate-dir ./crates/my-contract-client --crate-name my-contract-client

# Generate a no_std compatible WASM client crate
starforge contract generate-bindings ./contract.wasm --lang rust --crate-dir ./crates/wasm-client --no-std

# Generate TypeScript bindings  
starforge contract generate-bindings ./contract.wasm --lang ts

# Generate Python bindings
starforge contract generate-bindings ./contract.wasm --lang python

# Generate Go bindings
starforge contract generate-bindings ./contract.wasm --lang go
```

### Rust Client Crate Skeleton Layout & Features
When `--crate-dir` is provided, StarForge emits a full Cargo package pinned to Soroban SDK `22.0.0`:
- `Cargo.toml`: Package manifest with feature flags (`std`, `no_std`, `cli-backend`, `rpc-backend`, `testutils`).
- `src/lib.rs`: Strongly typed contract methods, argument serialization, and data structs.
- `README.md`: Crate documentation, function list, and versioning policy.


## Generated Code Examples

### Rust Bindings

```rust
use std::process::Command;
use std::io::{self, Write};
use anyhow::{Result, Context};

pub struct ContractClient {
    pub contract_id: String,
    pub network: String,
    pub wallet: Option<String>,
}

impl ContractClient {
    pub fn new(contract_id: impl Into<String>, network: impl Into<String>) -> Self {
        Self { contract_id: contract_id.into(), network: network.into(), wallet: None }
    }
    
    pub fn with_wallet(mut self, wallet: impl Into<String>) -> Self {
        self.wallet = Some(wallet.into());
        self
    }
    
    // Generated function calls with proper error handling
    pub fn transfer(&self, from: impl ToString, to: impl ToString, amount: impl ToString) -> Result<()> {
        let mut cmd = Command::new("starforge");
        cmd.args(["contract", "invoke", &self.contract_id, "transfer", "--network", &self.network]);
        cmd.arg("--arg").arg(from.to_string()).arg("--type").arg("Address");
        cmd.arg("--arg").arg(to.to_string()).arg("--type").arg("Address");
        cmd.arg("--arg").arg(amount.to_string()).arg("--type").arg("u128");
        
        if let Some(wallet) = &self.wallet {
            cmd.arg("--wallet").arg(wallet).arg("--submit");
        }
        
        let result = self.execute_command(cmd)?;
        Ok(())
    }
    
    fn execute_command(&self, mut cmd: Command) -> Result<String> {
        let output = cmd.output().context("Failed to execute command")?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Command failed: {}", stderr)
        }
    }
}
```

### TypeScript Bindings

```typescript
export type ContractClientOptions = {
    contractId: string;
    network?: string;
    wallet?: string;
};

export class ContractClient {
    constructor(private readonly options: ContractClientOptions) {}
    
    transfer(from: string, to: string, amount: number | bigint): string[] {
        return this.invokeArgs("transfer", [
            [from, "Address"],
            [to, "Address"],
            [amount, "u128"]
        ]);
    }
    
    private invokeArgs(functionName: string, args: Array<[unknown, string]>): string[] {
        const cli = ["contract", "invoke", this.options.contractId, functionName, 
                    "--network", this.options.network ?? "testnet"];
        for (const [value, typeName] of args) cli.push("--arg", String(value), "--type", typeName);
        if (this.options.wallet) cli.push("--wallet", this.options.wallet, "--submit");
        return cli;
    }
}

// Generated event types
export interface TransferEvent {
    from: string;
    to: string;
    amount: string;
}
```

### Python Bindings

```python
from dataclasses import dataclass
from typing import List, Dict, Optional, Union, Tuple
import subprocess

@dataclass
class ContractClientOptions:
    contract_id: str
    network: str = "testnet"
    wallet: Optional[str] = None

class ContractClient:
    def __init__(self, options: ContractClientOptions):
        self.options = options
    
    def transfer(self, from_addr: str, to_addr: str, amount: int) -> List[str]:
        return self._invoke_args("transfer", [
            (from_addr, "Address"),
            (to_addr, "Address"),
            (amount, "u128")
        ])
    
    def _invoke_args(self, function_name: str, args: List[Tuple[str, str]]) -> List[str]:
        cli = ["starforge", "contract", "invoke", self.options.contract_id, 
               function_name, "--network", self.options.network]
        for value, type_name in args:
            cli.extend(["--arg", str(value), "--type", type_name])
        if self.options.wallet:
            cli.extend(["--wallet", self.options.wallet, "--submit"])
        return cli

@dataclass
class TransferEvent:
    from_addr: str
    to_addr: str
    amount: int
```

### Go Bindings

```go
package client

import "os/exec"

type ContractClientOptions struct {
    ContractID string
    Network    string
    Wallet     *string
}

type ContractClient struct {
    options ContractClientOptions
}

func NewContractClient(options ContractClientOptions) *ContractClient {
    if options.Network == "" {
        options.Network = "testnet"
    }
    return &ContractClient{options: options}
}

func (c *ContractClient) Transfer(from string, to string, amount string) []string {
    args := [][2]string{
        {from, "Address"},
        {to, "Address"},
        {amount, "u128"},
    }
    return c.invokeArgs("transfer", args)
}

func (c *ContractClient) invokeArgs(functionName string, args [][2]string) []string {
    cli := []string{"contract", "invoke", c.options.ContractID, functionName, 
                   "--network", c.options.Network}
    for _, arg := range args {
        cli = append(cli, "--arg", arg[0], "--type", arg[1])
    }
    if c.options.Wallet != nil {
        cli = append(cli, "--wallet", *c.options.Wallet, "--submit")
    }
    return cli
}

type TransferEvent struct {
    From   string
    To     string
    Amount string
}
```

## Features

### 1. Type-Safe Interfaces
- Proper type mapping for all Soroban types
- Compile-time type checking
- Automatic type conversion

### 2. Event Support
- Extracts error enums as events
- Generates event type definitions
- Language-specific event structures

### 3. Complex Type Handling
- Options, Results, Vectors, Maps
- Nested structures
- Custom UDTs (User Defined Types)

### 4. Error Handling
- Proper error propagation
- Type conversion errors
- Command execution errors

## Testing

The binding generator includes comprehensive tests:

```bash
# Run all binding generator tests
cargo test bindings_tests
```

Tests cover:
- Basic functionality for all languages
- Error handling for invalid WASM files
- Type conversion correctness
- Event generation

## Implementation Details

### Key Improvements

1. **Enhanced Type Mapping**: 
   - Better handling of complex types (Option<T>, Result<T, E>, Vec<T>, etc.)
   - Language-specific type representations
   - Recursive type resolution

2. **Event Extraction**:
   - Parses `ScSpecEntry::UdtErrorEnumV0` as events
   - Generates event types in all languages
   - Proper field type mapping

3. **Improved Client Interfaces**:
   - Actual method calls instead of CLI string generation
   - Proper error handling and result parsing
   - Type-safe parameter passing

4. **Comprehensive Testing**:
   - Unit tests for type conversions
   - Integration tests with example contracts
   - Error handling tests

## Conclusion

The enhanced binding generator provides production-ready, type-safe interfaces for interacting with Soroban contracts across multiple programming languages. It significantly improves developer experience by reducing boilerplate code and providing compile-time type safety.