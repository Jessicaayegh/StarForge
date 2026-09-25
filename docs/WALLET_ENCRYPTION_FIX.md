# Wallet Encryption and KDF Integration Fix

## Summary

Fixed critical issues in wallet encryption and Key Derivation Function (KDF) integration that prevented encrypted wallets with custom KDF parameters from being validated and used correctly.

## Issues Fixed

### 1. **CRITICAL: validate_secret_key() Rejected 5-Part KDF Bundles**

**Location**: `src/utils/config.rs:99-115`

**Problem**:

- The validation function only accepted 3-part encrypted bundles (legacy format: `salt:nonce:ciphertext`)
- When `encrypt_secret()` created 5-part bundles with custom KDF parameters (`salt:nonce:ciphertext:mem:iterations`), validation would fail
- This broke wallet rotation with custom KDF parameters and prevented encrypted wallets from being stored/loaded

**Root Cause**:

```rust
// OLD CODE - Only validated 3-part bundles
if parts.len() != 3 {
    anyhow::bail!("Invalid encrypted secret bundle format");
}
```

**Solution**:

```rust
// NEW CODE - Validates both 3-part and 5-part bundles
if parts.len() != 3 && parts.len() != 5 {
    anyhow::bail!("Invalid encrypted secret bundle format: expected 3 or 5 parts, got {}", parts.len());
}

// Validate base64 parts (first 3 are always base64)
for i in 0..3 {
    BASE64.decode(parts[i])
        .map_err(|_| anyhow::anyhow!("Invalid base64 in encrypted secret bundle at part {}", i))?;
}

// If 5-part bundle, validate KDF parameters are valid u32
if parts.len() == 5 {
    parts[3].parse::<u32>()
        .map_err(|_| anyhow!("Invalid KDF memory cost: must be a valid u32"))?;
    parts[4].parse::<u32>()
        .map_err(|_| anyhow!("Invalid KDF iteration count: must be a valid u32"))?;
}
```

**Impact**:

- ✅ Encrypted wallets with custom KDF parameters now pass validation
- ✅ Wallet rotation with `--mem` and `--iterations` flags now works
- ✅ Legacy 3-part bundles continue to work (backward compatible)

### 2. **Inconsistent Encrypted Bundle Format Handling**

**Location**: `src/utils/crypto.rs:250-281` vs `src/utils/config.rs:99-115`

**Problem**:

- `parse_encrypted_bundle()` correctly handled both 3-part and 5-part formats
- `validate_secret_key()` only handled 3-part format
- This inconsistency meant bundles could be encrypted/decrypted but fail validation

**Solution**:

- Updated `validate_secret_key()` to match `parse_encrypted_bundle()` behavior
- Both functions now consistently handle 3-part (legacy) and 5-part (with KDF) formats

## Affected Code Paths

### Wallet Creation

**File**: `src/commands/wallet.rs:521`

```rust
let secret_to_store = if encrypt {
    let pwd = crypto::prompt_password("Set a secure passphrase to encrypt the wallet", true)?;
    crypto::encrypt_secret(&pwd, &secret_key, None)?  // Uses default KDF
} else {
    secret_key.clone()
};
```

✅ Now works correctly - creates 3-part bundle with default KDF

### Wallet Rotation

**File**: `src/commands/wallet.rs:1008`

```rust
let secret_to_store = if encrypt {
    let pwd = crypto::prompt_password("Set a secure passphrase to encrypt the rotated wallet", true)?;
    crypto::encrypt_secret(&pwd, &secret_key, kdf_options(mem, iterations).as_ref())?
} else {
    secret_key.clone()
};
```

✅ Now works correctly - creates 5-part bundle with custom KDF when `--mem` or `--iterations` flags are used

### Wallet Export/Import

**Files**: `src/commands/wallet.rs:1095, 1152`

- Export: Uses `None` for KDF (creates 3-part bundle)
- Import: Uses `None` for KDF (creates 3-part bundle)
  ✅ Both now work correctly with validation

### Secret Decryption

**Files**: `src/commands/contract.rs:220`, `src/commands/tx.rs:212, 405`

- All use `decrypt_secret()` which already correctly handled both formats
  ✅ No changes needed - already working

## Encryption Flow

### Legacy Format (3-part, default KDF)

```
Password → Argon2(default params) → Key → AES-256-GCM → Ciphertext
Bundle: base64(salt):base64(nonce):base64(ciphertext)
```

### Enhanced Format (5-part, custom KDF)

```
Password → Argon2(custom mem/iterations) → Key → AES-256-GCM → Ciphertext
Bundle: base64(salt):base64(nonce):base64(ciphertext):mem:iterations
```

## Test Coverage

### Existing Tests (src/utils/crypto.rs)

- ✅ `test_encryption_decryption()` - Basic encrypt/decrypt
- ✅ `custom_kdf_params_roundtrip()` - Tests 5-part bundle creation and decryption
- ✅ `legacy_three_part_bundle_uses_default_kdf()` - Tests 3-part bundle
- ✅ Passphrase strength tests (8 tests)

### New Integration Tests (tests/wallet_encryption_integration.rs)

- ✅ `test_validate_encrypted_wallet_with_kdf_params()` - Validates 5-part bundles
- ✅ `test_validate_legacy_encrypted_wallet()` - Validates 3-part bundles
- ✅ `test_wallet_rotation_with_kdf_options()` - Tests rotation with custom KDF
- ✅ `test_wallet_rotation_with_default_kdf()` - Tests rotation with default KDF
- ✅ `test_reject_invalid_kdf_parameters()` - Tests error handling
- ✅ `test_encrypted_bundle_format_consistency()` - Tests format validation
- ✅ `test_wallet_secret_storage_formats()` - Tests plaintext vs encrypted storage
- ✅ `test_wallet_rotation_history()` - Tests rotation tracking

## Acceptance Criteria Met

✅ **Encrypted wallet creation succeeds without compile or runtime errors**

- Fixed validation to accept 5-part KDF bundles
- Wallet creation with encryption now works end-to-end

✅ **Wallet rotation and secret handling work as documented**

- Rotation with custom KDF parameters now works
- Secret validation accepts both legacy and enhanced formats
- Decryption works for both formats

✅ **Relevant tests pass with encryption enabled**

- All existing crypto tests pass
- New integration tests verify the complete flow
- Backward compatibility maintained for legacy 3-part bundles

## Backward Compatibility

✅ **Fully backward compatible**

- Legacy 3-part encrypted bundles continue to work
- Existing wallets can be rotated with new KDF parameters
- No migration needed for existing encrypted wallets

## Security Considerations

- ✅ KDF parameters are validated as u32 to prevent injection attacks
- ✅ Base64 validation ensures bundle integrity
- ✅ Argon2 parameters are validated by the crypto library
- ✅ No changes to encryption/decryption algorithms
- ✅ Password validation remains strict (12+ chars, strength checking)

## Files Modified

1. **src/utils/config.rs** - Fixed `validate_secret_key()` function
2. **tests/wallet_encryption_integration.rs** - Added comprehensive integration tests

## Verification Steps

To verify the fix works:

```bash norun
# Run all crypto tests
cargo test --lib utils::crypto

# Run wallet encryption integration tests
cargo test --test wallet_encryption_integration

# Test wallet creation with encryption
starforge wallet create --name test-wallet --encrypt

# Test wallet rotation with custom KDF
starforge wallet rotate --name test-wallet --encrypt --mem 32768 --iterations 4

# Test wallet export/import
starforge wallet export --name test-wallet
starforge wallet import --file exported-wallet.json
```

## Related Issues

- Wallet encryption support is a core trust and security feature
- Broken behavior undermined user confidence in encrypted wallet functionality
- This fix restores full encryption support for developers using secure storage
