# Python Contract Client Bindings

This document explains how to generate an installable Python client package for a
deployed Soroban contract, and how to use the generated client.

---

## 1. Overview

`starforge contract generate-bindings` reads a compiled contract WASM file's
embedded spec metadata and emits typed client code. For Python, `--output-dir`
produces a complete, `pip install`-able package rather than a single source
string: a `pyproject.toml`, a `README.md`, and a package directory containing
`__init__.py` and `client.py`.

The generator targets Python 3.10+ and uses only the standard library
(`dataclasses`, `typing`) at runtime — the generated package has no third-party
dependencies.

---

## 2. Generating a package

```bash
starforge contract generate-bindings path/to/contract.wasm \
  --lang python \
  --output-dir ./my_contract_client \
  --package-name my_contract
```

This writes:

```
my_contract_client/
├── pyproject.toml
├── README.md
└── my_contract/
    ├── __init__.py
    └── client.py
```

`--package-name` is normalized into both a valid PEP 503 distribution name
(lowercase, hyphen-separated, used in `pyproject.toml`'s `name` field) and a
valid PEP 8 importable module name (lowercase, underscore-separated, used as
the package directory name).

Without `--output-dir`, `--lang python` behaves as before: it prints the
generated `client.py` source to stdout.

---

## 3. Installing and using the generated client

```bash
cd my_contract_client
pip install .
```

```python
from my_contract import ContractClient, ContractClientOptions

client = ContractClient(ContractClientOptions(
    contract_id="C...",
    network="testnet",
))

# Each generated method returns the argument list for `starforge contract
# invoke`, type-hinted from the contract's spec (dataclasses for structs and
# events, Optional/List/Dict for Option/Vec/Map types).
args = client.some_function(amount=100, recipient="G...")
```

The generated methods build a `starforge contract invoke` argument list
(matching the CLI's existing `--arg <value> --type <type>` convention) rather
than making an RPC call directly, mirroring this project's existing
TypeScript, Go, and Rust binding generators.

---

## 4. Testing the generator itself

Package-generation logic lives in `src/utils/bindings.rs`
(`generate_python_package`, `write_package`) and is covered by
`tests/bindings_python_package.rs`, which checks:

- valid, parseable `pyproject.toml` build metadata
- an importable `__init__.py`
- the generated `client.py` matches `generate_python`'s existing output
- package/module name normalization, including edge cases with no
  alphanumeric characters
- `write_package` produces a directory a real `pip install .` could consume

Run with:

```bash
cargo test --test bindings_python_package
```
