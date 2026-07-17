# Claude Code Instructions (CLAUDE.md)

Refer to [AGENTS.md](./AGENTS.md) in the project root for the canonical tech stack, workflow guidelines, and the strict ontological principles defined in [CONSTITUTION.md](./CONSTITUTION.md).

The regeneration pipeline is governed by [`docs/PIPELINE_SPINE.md`](./docs/PIPELINE_SPINE.md) — the in-memory carrier spine, the single `gmeow.gts` terminal, and the post-pipeline fanout. It is canonical for any work touching `crates/pipeline` or any artifact under `generated/`: every such artifact must be a projection of `gmeow.gts`.

"make sync" is VERY expensive - you make ONLY run it if required or as part of stage3.
"make check" is VERY expensive - run it ONLY when you have to - ideally once in each stage.

Rust optimization and advanced-language-feature work is governed by
[`docs/RUST-OPTIMIZATION.md`](./docs/RUST-OPTIMIZATION.md): measure first,
preserve deterministic output, prefer Rust-native data/dispatch/ownership
changes over compiler-flag churn, and keep debug assertions, overflow checks,
and the no-debug-symbol policy intact.

## Standing constraints (non-negotiable)

[`.goals`](./.goals) and [`CONSTITUTION.md`](./CONSTITUTION.md) are **normative and override everything else** here.

* **GREENFIELD, no backwards-compat** — when replacing an element, remove the inferior one; lossy compatibility lives only in generated projections, never in the canonical core.
* **RUST-FIRST, Python-surface** — core work is Rust. **Adding ANY Python (code, tests, fixtures, orchestration) requires explicit authorization; if you think you may be writing Python, you are probably doing the wrong thing.**
* **No-optionality forbids silent capability degradation; it does not forbid explicit feature selection.** For a selected operation and profile, every declared input, capability, invariant, and output is mandatory. Explicit profiles, sinks, output formats, and DAG branches are permitted when they are first-class, deterministic, cache-keyed, and fully validated. Once selected, their stages and outputs are required. A missing cache causes recomputation; a missing dependency or implementation is a HARD FAIL, never permission to use a weaker parser, omit output, retain stale bytes, or otherwise degrade semantics. Rust `Option<T>` and conditional DAG edges are not themselves violations.
* **Data flows slices → `gmeow.gts` + the `gmeow` CLI** (the shippable deliverables). Maximise information flow, ontological use, and dogfooding.

## Working discipline

* **Never work in the top-level checkout.** It is shared by 30+ developers and a daemon resets it to clean `main` every ~30s — uncommitted work there is wiped, and a stray branch/edit there has a huge blast radius. Always work in a git worktree (`.worktrees/<slug>/`) and write files under that worktree path.
* **Workflow = merge `origin/main` INTO your branch (never rebase); land via squash-merge (`ghprsq`).** See [AGENTS.md](./AGENTS.md) § 6.
* **Deal-breakers — never:** `git checkout --theirs/--ours .`, `git merge -X theirs/ours`, `--no-verify`, skipping/mocking the component under test, or batch-resolving conflicts "to save time" (resolve each one individually).
* **GPG / signing is off-limits** — never run `gpg`/`gpgconf` or touch the agent or keys; if a step needs signing, ask the user to run it.
* **No time/effort estimates** — reason in dependency order and relative risk.

## Regenerate & gates

* Synchronize with `make sync`. The target delegates one **single, idempotent pass** to `gmeow-dev sync`; local runs default to update mode and all outputs, while CI defaults to strict read-only checking. The Rust command uses every available CPU unless `--jobs N` is explicitly supplied. A clean whole-run manifest skips a fixed-point run; an input miss executes the pipeline once. Use `make sync SYNC_VERBOSE=1` to stream live DAG stages during a miss. There is no separate diagnostics or docs pipeline.
* `generated/dist/gmeow.gts` is `merge=ours` (`.gitattributes`), so a `git merge` leaves it holding the branch's pre-merge bytes; `make sync` rewrites it from the merged sources, so run it once after integrating `main`. A stale bundle is **not** silently accepted — its drift is caught by the superset/fold gate (the `crates/pipeline` superset check + `tests/full_parity.rs`), which compares the bundle semantically because it is CBOR and cannot use a byte-only comparison.
* Verify with the full `make check` — `make validate` / `make reason` alone are not sufficient. CI builds the PR **merged into `main`**, so integrate current `main` before final verification.

## Canonical sources & forward direction

* All semantic grounding to external formalisms is owned by one of the three grounding slices: linguistic/serialization surfaces in `lang:`, mathematical surfaces in `math:`, and upper ontologies/logics/rule/validation dialects in `logic:`. The grounding namespace is always the source and the external vocabulary is the target. Never author a competing grounding in a domain slice; use the grounding term instead.
* The `logic:` core is the canonical reasoning language. OWL/RDFS, Datalog, SHACL, Prolog, and gUFO are typed views; BFO, OBO/RO, SUMO, DUL, YAMATO, and OpenCyc are commitment-shifting `BridgeView`s. `math:` owns Data Cube/STATO/OBCS and mathematical reference surfaces; `lang:` owns OntoLex/LexInfo/WordNet/NIF/Web Annotation and linguistic reference surfaces. Every live grounding law ships in the meta-level correspondence graph with an explicit preservation judgment; SSSOM, EDOAL, and FnO are generated lowerings (Principle 17).
* Grounding correspondences are shipped ontology content in the meta-level `graph/correspondence-laws` graph of `gmeow.gts`, outside object-level closure. SSSOM is not the authority. Read [`docs/GROUNDING.md`](./docs/GROUNDING.md) and [`docs/foundational-bridging.md`](./docs/foundational-bridging.md) before editing this surface.
* **Author validation in `logic:`, never on a shape surface.** Do **not** hand-author `sh:NodeShape`/`sh:PropertyShape` in a slice's `shapes.ttl` — that is a forbidden second source of truth (projection-purity gate). Author **declarative** checks (cardinality, class, datatype, node-kind, value-set) as **EL-safe OWL/RDFS axioms in the slice `module.ttl`** (the pipeline *derives* the SHACL/ShEx), and **procedural/cross-node** checks as a **`logic:Constraint` + `logic:Formula`** in `module.ttl`; laws are `logic:Formula` ASTs, higher-order gaps honest `logic:expressivenessBoundary` records. OWL as a downstream *surface* is a code smell; OWL/RDFS declarative *axioms in `module.ttl`* are the canonical derive-source and are correct. Full doctrine: **AGENTS.md §"Critical Ontological Rules"**, [`slices/grounding/logic/design/LOGIC-VALIDATION.md`](./slices/grounding/logic/design/LOGIC-VALIDATION.md), [`docs/MIGRATING-SHAPES-TO-LOGIC.md`](./docs/MIGRATING-SHAPES-TO-LOGIC.md).
* The design sets [`slices/grounding/logic/design/*.md`](./slices/grounding/logic/design/), [`slices/core/inhabitation/design/*.md`](./slices/core/inhabitation/design/), and [`docs/APPLIED_CATEGORY_THEORY/*.md`](./docs/APPLIED_CATEGORY_THEORY/) are **canonical** — read the relevant ones in full before working in those areas.

## Build and Validation Commands

* Show current task plan: `make help`
* Install environment: `make install`
* Run format: `make fmt`
* Run lint: `make lint`
* Validate Turtle & SHACL: `make validate`
* Validate bundled GTS snapshot: `make validate-gts`
* Regenerate generated artifacts: `make sync`
* Check generated artifacts: `make sync SYNC_MODE=check SYNC_OUTPUTS=generated`
* Run native reasoning: `make reason`
* Run native verification: `make verify`
* Run native reasoning + verification together: `make reason-verify`
* Run full local gate: `make check`

## Testing Commands

* Run Rust tests: `make rust-test`
* Run Rust clippy: `make clippy`
* Run a single crate's tests: `cargo nextest run -p <crate>`

## Generated and Release Outputs

* Regenerate docs: `make sync SYNC_OUTPUTS=docs`
* Build dist serializations: `make build`
* Run release build: `make release`
* Sign a release GTS: `make release-sign-gts SIGN_KEY=/tmp/gpg/signing-key.asc GTS_OUT=dist/gmeow.gts`

## Maintainer Tasks

Maintainer-only targets are prefixed with `maint-`. Use `make help` for the
complete list. Common lanes are `make maint-wikidata-live` and
`make maint-rust-heavy` (the off-gate heavy Rust suite). The native,
Docker-free reasoning-oracle cross-check (`gmeow-dev reason-crosscheck` over
`purrdf::entail`) runs on-gate inside `make reason-gate`, which shares one
fresh native closure between reasoned-graph verification and the purrdf-entail
oracle. `make reason-crosscheck` remains a standalone, focused convenience lane
(not a `maint-` lane), but it is no longer a separate `make check` gate task.
