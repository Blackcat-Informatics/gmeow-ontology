# GitHub Copilot Instructions

When suggesting code completions or generating templates in this repository, always align with the rules below:

1. Refer to [AGENTS.md](../AGENTS.md) in the project root for the available `make` targets and developer commands (for example, `make check` and `gmeow-dev sync`).
2. Strictly follow the twelve principles in [CONSTITUTION.md](../CONSTITUTION.md).
3. Do not suggest editing generated files. In particular:
   - Do not edit files under `mappings/` or `projections/` directly; edit files under `mapping-dsl/` instead.
   - Do not edit files under `statements/` directly; edit files under `statement-dsl/` instead.
4. Ensure all ontology classes, properties, and instances use the correct camelCase or PascalCase formatting as established in existing Turtle (`.ttl`) files in `ontology/modules/`.
5. Ensure Rust code is formatted with `cargo fmt` and passes `cargo clippy` (warnings-as-errors); crate dependencies are defined in the workspace `Cargo.toml`.
