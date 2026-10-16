# Peliarch integration toolkit

An Agentic APWorld skill repo with a portable Archipelago protocol client,
durable host, Windows GUI, discovery tools and integration bootstrap.
Start with [.agents/skills/agentic-apworld/SKILL.md](.agents/skills/agentic-apworld/SKILL.md)
or invoke `$agentic-apworld` in an agent that discovers repository skills.
Other agents can read that file directly. Keep the skill and this SDK together;
copying SKILL.md alone does not include its runtime dependencies.
See [BUILD-AN-INTEGRATION](docs/agents/BUILD-AN-INTEGRATION.md) for the SDK workflow.

The discovery scanner and engine references are a selective, pinned MIT-licensed
Universal Modder import. Attribution, revision and original hashes are under
the skill's vendor/universal-modder directory. No full plugin, generated assets,
cloud API keys or game files are needed. Discovery is heuristic, not a promise
of game support. This toolkit does not install or launch games automatically.

From this directory:

```text
cargo build --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
python -m unittest discover -s tooling -p "test_*.py"
python -m unittest discover -s tools -p "test_export_peliarch_toolkit.py"
python -m unittest discover -s tools -p "test_agentic_apworld_skill.py"
python tooling/new_integration.py ../my-integration --name my_integration
```

Cargo.lock is independently generated for these toolkit crates. EXPORT-MANIFEST.json
records file hashes, the lock-generation command and its output. Exporting never
overlays an existing destination. The websocket compatibility fork is pinned to
an immutable revision. Dependency downloads require network access unless cached.
