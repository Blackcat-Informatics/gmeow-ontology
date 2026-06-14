# GitHub Copilot Instructions

When suggesting code completions or generating templates in this repository, always align with the rules below:

1. Refer to [AGENTS.md](../AGENTS.md) in the project root for the available `make` targets and developer commands (e.g., `make check`, `gmeow-dev regenerate mappings`, `gmeow-dev regenerate statements`).
2. Strictly follow the twelve principles in [CONSTITUTION.md](../CONSTITUTION.md).
3. Do not suggest editing generated files. In particular:
   - Do not edit files under `mappings/` or `projections/` directly; edit files under `mapping-dsl/` instead.
   - Do not edit files under `statements/` directly; edit files under `statement-dsl/` instead.
4. Ensure all ontology classes, properties, and instances use the correct camelCase or PascalCase formatting as established in existing Turtle (`.ttl`) files in `ontology/modules/`.
5. Ensure Python code complies with the ruff auto-formatter rules and targets the dependencies defined in `pyproject.toml` using `uv`.
