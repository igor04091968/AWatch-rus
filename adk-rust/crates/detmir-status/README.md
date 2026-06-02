# detmir-status

Read-only DetMir status probe written in Rust as a small ADK-Rust adoption
spike.

The command reads `/var/lib/detmir-ai/latest-state.json` and prints either a
compact operator summary, raw normalized JSON, or an ADK `Content` envelope that
can later be handed to an ADK runner/model/tool chain.

It does not start recovery, modify services, or write state.

```bash
cd adk-rust
cargo run -p detmir-status -- --json
cargo run -p detmir-status -- --adk-json
cargo run -p detmir-status -- status --json
```

The package also builds a compatibility binary named `detmir-adk-status`.
