# StarForge WASM - Build & Deployment Guide

## Overview

StarForge core functionality compiled to WebAssembly for browser execution, enabling web-based IDEs and development environments.

## Prerequisites

- Rust 1.70+ with `wasm32-unknown-unknown` target
- wasm-pack 1.3+
- Node.js 16+
- npm or yarn

## Installation

### 1. Add Rust WASM Target

```bash norun
rustup target add wasm32-unknown-unknown
```

### 2. Install wasm-pack

```bash norun
# macOS
brew install wasm-pack

# Linux
curl https://rustwasm.org/wasm-pack/installer/init.sh -sSf | sh

# Windows (with npm)
npm install -g wasm-pack
```

## Building

### Development Build (Fast)

```bash norun
cd wasm
wasm-pack build --target web --dev
```

Output: `pkg/` directory with `.wasm` and `.js` files

### Production Build (Optimized)

```bash norun
cd wasm
wasm-pack build --target web --release
```

Size: ~100-200KB minified + gzipped

## Local Testing

### Start Web Server

```bash norun
cd wasm
python3 -m http.server 8000

# OR
npm run serve
```

Visit: `http://localhost:8000`

## Features

### ✅ Wallet Management

- Generate keypairs
- Create/manage wallets
- Validate public keys
- Track balances

### ✅ Cryptography

- SHA256 hashing
- Random generation
- Base64 encoding/decoding
- Hex validation

### ✅ Browser Storage

- localStorage integration
- Config persistence
- Session management

### ✅ Horizon Integration

- Fetch account details
- Get balances
- Submit transactions (async)

### ✅ Contract Tools

- Validate contract IDs
- Format inspection
- Compatibility checks

## API Examples

### Rust WASM Binding

```javascript
import init, {
  WasmKeypair,
  WasmWallet,
  WasmCrypto,
} from "./pkg/starforge_wasm.js";

await init();

// Generate keypair
const keypair = WasmKeypair.generate();
console.log(keypair.public_key());

// Create wallet
const wallet = new WasmWallet(pubkey, "testnet");
wallet.set_balance(100);

// Hash
const hash = WasmCrypto.sha256("hello");
console.log(hash);

// Validate
const valid = WasmKeypair.validate_public_key("G...");
```

## Web Interface

Built-in web IDE with:

- **Wallet Tab**: Generate, create, manage wallets
- **Crypto Tab**: Hash, encode/decode, generate random
- **Contract Tab**: Validate contract IDs, inspect

Access at: `http://localhost:8000/index.html`

## Performance

| Operation            | Time   |
| -------------------- | ------ |
| Keypair generation   | ~5ms   |
| SHA256 hash          | ~1ms   |
| Random generation    | ~0.5ms |
| Base64 encode/decode | ~2ms   |
| Validation           | <1ms   |

## Deployment

### Static Hosting (Netlify, Vercel, GitHub Pages)

1. Build for production:

   ```bash norun
   wasm-pack build --target web --release
   ```

2. Copy `pkg/` to hosting static files

3. Serve `index.html`

### Docker

```dockerfile
FROM node:18-alpine
WORKDIR /app
COPY wasm/ .
RUN npm install -g wasm-pack
RUN wasm-pack build --target web --release
EXPOSE 8000
CMD ["npx", "http-server", "-p", "8000"]
```

### CDN Distribution

Upload to CDN:

```bash norun
wasm-pack build --target bundler
# Use with webpack/rollup
```

## Bundle Sizes

| Build           | Size  | Gzipped |
| --------------- | ----- | ------- |
| Dev             | 350KB | 80KB    |
| Release         | 180KB | 45KB    |
| Release + Strip | 120KB | 30KB    |

## Browser Compatibility

- ✅ Chrome 74+
- ✅ Firefox 79+
- ✅ Safari 14.1+
- ✅ Edge 74+
- ⚠️ Mobile browsers (90%+ support)

## Security Considerations

- WASM runs in browser sandbox
- No access to filesystem by default
- localStorage is client-side only
- Use HTTPS for production
- Validate all user inputs
- Never store secrets in localStorage

## Testing

### Unit Tests

```bash norun
wasm-pack test --headless --firefox
```

### Manual Testing

- Generate keypairs in browser
- Validate public keys
- Hash strings
- Encode/decode base64

## Troubleshooting

### "wasm-pack not found"

```bash norun
npm install -g wasm-pack
```

### Build errors

```bash norun
cargo clean
wasm-pack build --target web
```

### JavaScript errors

- Check browser console (F12)
- Verify WASM module loaded
- Check module paths in HTML

### Large bundle size

- Use `--release` flag
- Enable minification
- Use tree-shaking bundler

## Future Enhancements

- Smart contract compilation
- Transaction builder UI
- Multi-sig support
- Hardware wallet integration
- Advanced contract inspection
- Template preview

## API Reference

### WasmKeypair

- `generate()` → keypair
- `public_key()` → string
- `validate_public_key(key)` → bool
- `validate_contract_id(id)` → bool

### WasmWallet

- `new(pubkey, network)` → wallet
- `public_key()` → string
- `balance()` → f64
- `set_balance(amount)` → void

### WasmCrypto

- `sha256(input)` → string
- `random_hex(length)` → string
- `to_base64(input)` → string
- `from_base64(input)` → string
- `is_valid_hex(input)` → bool

### WasmConfig

- `new()` → config
- `get(key)` → string | null
- `set(key, value)` → void
- `load_from_storage(key)` → config
- `save_to_storage(key)` → void

### WasmHorizonClient

- `new(network)` → client
- `get_account(id)` → Promise<object>
- `get_balance(id)` → Promise<f64>
- `submit_transaction(tx)` → Promise<object>

## Support

- GitHub: https://github.com/Nanle-code/StarForge
- Issues: https://github.com/Nanle-code/StarForge/issues
- Discussions: https://github.com/Nanle-code/StarForge/discussions
