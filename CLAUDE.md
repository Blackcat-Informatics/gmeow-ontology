# Claude Code Instructions (CLAUDE.md)

Refer to [AGENTS.md](./AGENTS.md) in the project root for the canonical tech stack, workflow guidelines, and the strict ontological principles defined in [CONSTITUTION.md](./CONSTITUTION.md).

## Standing constraints (non-negotiable)

[`.goals`](./.goals) and [`CONSTITUTION.md`](./CONSTITUTION.md) are **normative and override everything else** here.

* **GREENFIELD, no backwards-compat** — when replacing an element, remove the inferior one; lossy compatibility lives only in generated projections, never in the canonical core.
* **RUST-FIRST, Python-surface** — core work is Rust. **Adding ANY Python (code, tests, fixtures, orchestration) requires explicit authorization; if you think you may be writing Python, you are probably doing the wrong thing.**
* **No optionality / hard-fail** — no feature-gates, optional deps, or degraded fallbacks; a missing required thing is a HARD FAIL: stop and report, never paper over it.
* **Data flows slices → `gmeow.gts` + the `gmeow` CLI** (the shippable deliverables). Maximise information flow, ontological use, and dogfooding.

## Working discipline

* **Never work in the top-level checkout.** It is shared by 30+ developers and a daemon resets it to clean `main` every ~30s — uncommitted work there is wiped, and a stray branch/edit there has a huge blast radius. Always work in a git worktree (`.worktrees/<slug>/`) and write files under that worktree path.
* **Workflow = merge `origin/main` INTO your branch (never rebase); land via squash-merge (`ghprsq`).** See [AGENTS.md](./AGENTS.md) § 5.
* **Deal-breakers — never:** `git checkout --theirs/--ours .`, `git merge -X theirs/ours`, `--no-verify`, skipping/mocking the component under test, or batch-resolving conflicts "to save time" (resolve each one individually).
* **GPG / signing is off-limits** — never run `gpg`/`gpgconf` or touch the agent or keys; if a step needs signing, ask the user to run it.
* **No time/effort estimates** — reason in dependency order and relative risk.

## Regenerate & gates

* Regenerate with `make regenerate` — never a bare `gmeow-dev regenerate` (it drops the diagnostics fold).
* In a fresh worktree, build the native PyO3 extensions (`make native-py`) **before** `make regenerate` / `make validate`.
* `generated/dist/gmeow.gts` is `merge=ours`: after integrating `main`, **regenerate it** — the drift gates do not catch a stale bundle.
* Verify with the full `make check` — `make validate` / `make reason` alone are not sufficient. CI builds the PR **merged into `main`**, so integrate current `main` before final verification.

## Canonical sources & forward direction

* The `logic:` core is the canonical reasoning language; OWL, Datalog, SHACL, Prolog, gUFO, SSSOM, EDOAL, and FnO are **generated lossy projections** of it (Principle 17), each carrying a preservation judgment in the loss ledger.
* The design sets [`slices/core/logic/design/*.md`](./slices/core/logic/design/), [`slices/core/inhabitation/design/*.md`](./slices/core/inhabitation/design/), and [`docs/APPLIED_CATEGORY_THEORY/*.md`](./docs/APPLIED_CATEGORY_THEORY/) are **canonical** — read the relevant ones in full before working in those areas.

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
