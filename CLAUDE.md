# Claude Code Instructions (CLAUDE.md)

Refer to [AGENTS.md](./AGENTS.md) in the project root for the canonical tech stack, workflow guidelines, and the strict ontological principles defined in [CONSTITUTION.md](./CONSTITUTION.md).

## Build and Validation Commands

* Install environment: `make install`
* Run format: `make fmt`
* Run lint: `make lint`
* Validate Turtle & SHACL: `make validate`
* Compile mappings: `make compile-mappings`
* Compile statements: `make compile-statements`
* Run OWL consistency reasoning: `make reason`
* Run full verification: `make check`

## Testing Commands

* Run all tests: `make test`
* Run specific test file: `uv run pytest tests/test_names.py`
