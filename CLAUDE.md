# Claude Code Instructions (CLAUDE.md)

Refer to [AGENTS.md](./AGENTS.md) in the project root for the canonical tech stack, workflow guidelines, and the strict ontological principles defined in [CONSTITUTION.md](./CONSTITUTION.md).

## Build and Validation Commands

* Show current task plan: `make help`
* Install environment: `make install`
* Run format: `make fmt`
* Run lint: `make lint`
* Validate Turtle & SHACL: `make validate`
* Validate bundled GTS snapshot: `make validate-gts`
* Regenerate generated artifacts: `make regenerate`
* Check generated artifacts: `make check-generated`
* Run native reasoning: `make reason`
* Run native verification: `make verify`
* Run full local gate: `make check`

## Testing Commands

* Run Python tests: `make test`
* Run fast Python gate lane: `make test-fast`
* Run Rust tests: `make rust-test`
* Run Rust clippy: `make clippy`
* Run specific test file: `uv run pytest tests/test_names.py`

## Generated and Release Outputs

* Regenerate docs: `make docs`
* Build dist serializations: `make build`
* Run release build: `make release`
* Sign a release GTS: `make release-sign-gts SIGN_KEY=/tmp/gpg/signing-key.asc GTS_OUT=dist/gmeow.gts`

## Maintainer Tasks

Maintainer-only targets are prefixed with `maint-`. Use `make help` for the
complete list. Common lanes are `make maint-classic-cross-check`,
`make maint-reason-hermit`, `make maint-verify-docker`,
`make maint-wikidata-live`, `make maint-test-heavy`, and
`make maint-test-network`.
