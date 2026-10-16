# Contributing

Project Kane AP is an experimental adaptation. Please preserve the original Sly
APWorld identity, location/item IDs and access rules unless a change explicitly
introduces and documents a new compatibility boundary.

The pinned native upstream is recorded in `native/upstream.json`. Our changes are
distributed as a patch plus `native/additions`, rather than a complete copy of
ProjectCane. `Build-Native.cmd` reconstructs and builds that source locally. Preserve
ProjectCane's authorship and component notices.

The companion source is in `companion/`, with the exact reusable SDK source under
`sdk/`. Build with stable Rust (`cargo build --manifest-path companion/Cargo.toml
--release --locked -j 1`). Its local path dependencies are intentionally included.
Native/companion compatibility uses the `sly1-native-v1` contract.

Useful checks:

```powershell
cargo test --manifest-path companion/Cargo.toml --locked -j 1
cargo clippy --manifest-path companion/Cargo.toml --all-targets --locked -j 1 -- -D warnings
runtime\python\python.exe -m unittest discover -s tests -p "logic*_test.py"
```

The Python checks require the matching runtime/AP core included in the player ZIP.
Native transaction tests can run with `tests/native_bridge_harness.py` against a
built executable and a fresh disposable output directory. Those use synthetic game
state and must not be described as a gameplay pass.

Do not submit game data, personal saves, connection passwords, or unreviewed full
logs. Describe reproducible steps and distinguish observed behavior from hypotheses.
Keep network/delivery authority in the companion, logic in Python, and native game
effects on the game thread. Do not acknowledge a repeatable item before its game
state and receipt are durably committed together.
