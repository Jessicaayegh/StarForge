1. Add `ai = []` to Cargo.toml `[features]`, and also `default = ["ai"]`? Wait.
If `default = ["ai"]`, then `cargo build` (default) builds with `ai`. `cargo build --no-default-features` builds without `ai`.
Let's see if there is a `default` feature in Cargo.toml.
