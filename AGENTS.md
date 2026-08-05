<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# AI Developer Agent Guide (AGENTS.md)

Welcome, AI Agent! This file is your behavioral contract and instruction manual for contributing to the GMEOW repository. Please read and adhere strictly to the rules below.

@/home/paudley/Active/gmeow-ontology/.goals
@/home/paudley/Active/gmeow-ontology/.baseline

---

## 1. Project Overview & Architecture

GMEOW is a **reasoning-centric, RDF 1.2-native, logic-canonical super-vocabulary** that unifies document metadata, entity descriptions, legal agreements, contacts, and person-centric data. OWL 2 DL, SHACL, gUFO, and other external formalisms are typed target views of the canonical grounding kernel, not its semantic owners.

Every design decision, code modification, and schema change is governed by the nineteen principles of the [CONSTITUTION.md](./CONSTITUTION.md). Cite these principles by number (e.g., `"Principle 4"`) in your commit messages, pull requests, and discussions.

### Critical Ontological Rules

* **One Canonical Source (Principle 4)**: Do not hand-edit generated files. Any change must be made in the canonical source files.
  * **Mappings**: Shared vocabulary and cross-cutting sets live in [dsl/mappings/](./dsl/mappings/); slice-owned mappings live in `slices/<group>/<name>/mappings/`. Both compile to `generated/`.
  * **Statements**: Authored in [dsl/statements/](./dsl/statements/) -> compiled to `generated/statements/`.
  * **Citations**: Authored in [metadata/references.ttl](./metadata/references.ttl) -> compiled to [generated/references/](./generated/references/). Follow [docs/CITATIONS.md](./docs/CITATIONS.md) when adding external sources, standards, issues, PRs, or review-thread citations.
* **Grounding ownership (Principles 17 & 19)**: All semantic grounding to external formalisms is authored in `slices/grounding/lang`, `slices/grounding/math`, or `slices/grounding/logic`. The grounding term is the source and the external vocabulary is the target. Grounding correspondences ship in `graph/correspondence-laws`; SSSOM is only a generated view. Read [docs/GROUNDING.md](./docs/GROUNDING.md) and the owning slice's design set before editing these seams.
* **RDF 1.2 / RDF\*-first (Principles 2 & 3)**: Statement-level metadata (provenance, confidence, temporal scope) is authored as native RDF 1.2 / RDF\* in the statement DSL. The canonical logical core is `logic:`; OWL 2 DL is a generated decidable projection.
* **Co-equal & Non-privileged (Principles 9 & 10)**: There is no `primaryName`, `preferredGender`, or single-winner preference. A contested fact is represented as coexisting standpoint-indexed claims. A superseded label/deadname is suppressed using `gmeow:displayable false` rather than deleted.
* **Validation is authored in `logic:`, never on a shape surface (Principle 17)**: OWL, SHACL, ShEx, and Datalog are **generated lossy projections** of the canonical `logic:` core, not authoring surfaces. See [`slices/grounding/logic/design/LOGIC-VALIDATION.md`](./slices/grounding/logic/design/LOGIC-VALIDATION.md) and [`docs/MIGRATING-SHAPES-TO-LOGIC.md`](./docs/MIGRATING-SHAPES-TO-LOGIC.md). Concretely, when you add a constraint to a slice:
  * **NEVER hand-author a `sh:NodeShape` / `sh:PropertyShape` in a slice's `shapes.ttl`** (or root `shapes/*.ttl`). A hand-authored shape is a **second source of truth** no reasoner backs, no loss ledger governs, and free to drift — it is sealed off by the projection-purity gate (`meta:gate-projection-shape-purity`). Any authored shape that remains must carry a `logic:formalizes` back-reference naming the `logic:` node it projects; an un-backed shape is flagged `slice-quality.projection.ungrounded-shape` by the `gmeow:axisShapeMigration` quality axis. (The many legacy `shapes.ttl` blocks are a **debt being migrated under equivalence-before-deletion**, not a pattern to copy.)
  * **The projection-vocabulary ratchet caps net-new ungrounded growth.** A `make check` gate holds a per-(slice, guarded-vocabulary) non-increasing ceiling on ungrounded residue — hand-authored constructs carrying no resolvable typed `logic:formalizes`, authored outside the vocabulary's grounding-slice owner boundary. Net-new second-source authoring in SHACL, gUFO, BFO/OBO/DOLCE/SUMO bridge vocabularies, RDFS subsumption (`rdfs:subClassOf`/`subPropertyOf`), and the alignment stack reds the gate; a ceiling only ever falls relative to its **relocation-adjusted base** (equivalence-before-deletion). A ceiling budgets net-new ungrounded *authoring*, which is location-independent, so a dated `gmeow:CeilingRelocation` re-projects the merge-base ceiling through a declared move **before** the unchanged lower-only comparison runs — every transported unit must be witnessed by a construct that genuinely departed the source and arrived at the destination, funded by a matching lowering, and pinned to the destination's measured residue. No tool ever creates headroom: a raise beyond that adjustment still reds and is still a maintainer-only decision authorized out-of-band. Axis floors are deliberately **not** netted by a relocation — a floor measures the documentation quality of the inventory a slice owns, which really is location-dependent. See [`docs/PROJECTION-VOCABULARY-RATCHET.md`](./docs/PROJECTION-VOCABULARY-RATCHET.md).
  * **Declarative checks** (cardinality, `sh:class`/datatype/node-kind/value-set) are authored as **ordinary EL-safe OWL/RDFS axioms in the slice's `module.ttl`** — `rdfs:domain`/`rdfs:range`, `owl:allValuesFrom`/`someValuesFrom`/`hasValue`, `owl:oneOf`, `owl:disjointWith`, `owl:FunctionalProperty`, and `owl:maxQualifiedCardinality`/`minQualifiedCardinality` **+ `owl:onClass`**. The pipeline **derives** the SHACL Core + ShEx surfaces from these axioms (`derive_validation_shapes`); you write the axiom, never the shape. **Never** un-qualified `owl:cardinality`/`owl:minCardinality`/`owl:maxCardinality` (out of the EL fragment — hard-fails `make reason-verify`). A genuinely closed-world **required path** (`sh:minCount 1`) is `owl:allValuesFrom` **plus** an explicit `logic:ClosureEntry` (`logic:onClass K`, `logic:closureKey P`, `logic:closureValue logic:ClosedWorldClosure`), never a bare existential.
  * **Procedural / cross-node checks** (value comparisons, guarded existence, uniqueness, forbidden patterns) are authored as a **`logic:Constraint`** whose `logic:integrity` is a realized `logic:Formula` tree (`∀/∃/∧/∨/¬/→`, `logic:relation` + `logic:argument`), carrying `logic:formalizes` and `logic:message`. The pipeline lowers it to a `sh:SPARQLConstraint`. Slice sugar (`logic:GuardedImplicationConstraint`, `logic:ChoiceGroupConstraint`, `logic:ForbiddenPatternConstraint`, …) is available; see [`docs/MIGRATING-SHAPES-TO-LOGIC.md`](./docs/MIGRATING-SHAPES-TO-LOGIC.md) for the cheatsheet and [`slices/core/ai/module.ttl`](./slices/core/ai/module.ttl) for worked examples.
  * **Mathematical / structural laws** are authored as real first-order `logic:Formula` ASTs (e.g. `math:continuityLaw`), attached via `math:definingLaw`/`math:preservesStructure`; a genuinely higher-order property that is not first-order axiomatizable is carried as an honest `logic:expressivenessBoundary` record (e.g. `math:compactnessBoundary`), never a faked formula.
  * **In short — OWL as a downstream *surface* is a code smell; OWL/RDFS declarative *axioms in `module.ttl`* are the canonical derive-source and are correct.** If a check cannot be expressed as an EL-safe axiom or a `logic:Constraint`, it is carried as flagged unsupported residue in the loss ledger and the check is relocated — a STOP-and-ask, never a self-granted hand-authored-shape exception.

### No-optionality doctrine

> **No-optionality forbids silent capability degradation; it does not forbid explicit feature selection.**

For a selected operation and profile, every declared input, capability, invariant,
and output is mandatory. Explicit profiles, sinks, output formats, and DAG branches
are permitted when they are first-class, deterministic, cache-keyed, and fully
validated. Missing capabilities must fail; they must never cause silent semantic
degradation.

Accordingly, CLI choices such as requesting documentation output, selecting an
extended projection profile, choosing RDF 1.2 output, or naming an output path are
valid optional inputs that define the requested DAG. Once selected, their stages
and outputs are required. A missing cache causes recomputation, not skipped work.
A missing dependency, source, or implementation must hard-fail rather than invoke
a weaker parser, omit part of an output, retain stale bytes, or otherwise
half-implement the selected operation. Rust `Option<T>` and conditional DAG edges
are not themselves violations; opportunistic fallback with weaker semantics is.

---

## 2. Core Toolchain & Commands

The repository is built on Rust/Cargo, with Docker used only for explicit
maintainer/oracle lanes. The Makefile is the definitive task-oriented plan:
run `make help` first when you need the current target surface, and use `make`
targets rather than calling `gmeow-dev`, `cargo`, or helper scripts directly
unless you are adding or debugging the target itself.

Rust performance and advanced-language-feature work must also follow
[`docs/RUST-OPTIMIZATION.md`](./docs/RUST-OPTIMIZATION.md): measure first,
preserve deterministic output, prefer Rust-native data/dispatch/ownership
changes over compiler-flag churn, and keep the existing debug-assertion,
overflow-check, and no-debug-symbol contracts intact.

### The CLI razor — `gmeow` vs `gmeow-dev`

There are two CLIs, and a single razor decides where a command belongs:

> **`gmeow` does not need a repo; `gmeow-dev` does.**

* **`gmeow`** ([crates/gmeow-cli](./crates/gmeow-cli)) is the public consumer surface — a native Rust binary. Every command must work from the installed binary alone — backed by the embedded `generated/dist/gmeow.gts` snapshot — with **no source checkout, Docker, generator inputs, or repo-local query trees**. Transpiling a user's own RDF, describing a term, verifying the bundle: consumer operations, so `gmeow`. Because the slice-quality rubric is itself embedded in `gmeow.gts` (the wheel-shippable engine landed in the A1 factory workstream), scoring or linting an *external* slice directory is a consumer operation too — so `gmeow slice quality <dir>` and `gmeow slice lint <dir>` legitimately live here, scoring the foreign slice against the embedded rubric with no checkout.
* **`gmeow-dev`** ([crates/gmeow-dev-cli](./crates/gmeow-dev-cli)) is repository maintenance. It may read anything in the tree — `dsl/`, `generated/`, `imports/`, `tests/fixtures/` — because it only ever runs inside a checkout. Regenerating artifacts, sweeping/ratcheting slice-quality scores across the whole dev corpus (`gmeow-dev slice-quality`, which reads the repo-wide model the consumer bundle cannot), refreshing vendored snapshots: developer operations, so `gmeow-dev`. Both surfaces share one scoring engine (`gmeow-slice-quality`); they differ only in whether wide-scope inputs come from the surrounding checkout or the embedded bundle.

When adding a command, ask the razor first. If it needs a repo path that the binary does not bundle, it is `gmeow-dev` — or the data it needs must first be bundled so it can be `gmeow`.

### Environment & Formatting

```bash
make help            # Show the grouped task plan
make install         # Source-first bootstrap: build the producer, sync generated/, build the consumer CLIs
make fmt             # Auto-format Rust sources with cargo fmt
make lint            # Run the issue-ref lint and the pre-commit hygiene suite (Rust fmt/clippy, spelling, YAML, actions, secrets)
make clean           # Remove ephemeral build artifacts and native build stamps
```

`make install` bootstraps source-first: it builds only the `gmeow-dev` producer
crate, runs the producer (`make check-sync SYNC_MODE=update SYNC_OUTPUTS=all`)
to materialize the git-ignored `generated/` tree
(including `generated/dist/gmeow.gts`) from canonical sources, and only then
builds the consumer CLIs that embed that materialized bundle. There is no Git
merge-driver step — `generated/` is never tracked, so it never participates in
a merge.

Every payload-bearing frame in any production-authored GMEOW GTS bundle MUST use
exactly `zstd-rsyncable` at compression level 12. This is a hard distribution
contract: never substitute gzip, plain zstd, identity, or a size-dependent
fallback. The `gmeow-gts-profile` leaf crate is the ONE production entry to
`purrdf::gts_compose::emit_gts` anywhere in the workspace; its wrapper and
committed-bundle frame audit enforce the transform, while a compile-time assertion
pins purrdf's dist level to 12. It is a leaf, not part of `gmeow-pipeline`, so that
every bundle author can depend on it — including `gmeow-math`, which the pipeline
itself depends on and which therefore cannot depend back on the pipeline. Header and
payload-free transport metadata are not compression frames.

### Validation & Compilation

```bash
make validate        # Validate Turtle syntax, term annotations, and SHACL
make check-sync SYNC_MODE=update # Materialize ALL generated artifacts (the single producer; parallel by default)
make check-sync # Drift + orphan + internal-tag-leak check for every registered generator (read-only default)
make constitution-check # Every principle has live enforcement (governance/constitution.ttl)
make crate-check     # Verify Rust crate layering and acyclic crate DAGs
make wikidata        # Validate Wikidata QID/PID syntax in the mappings (offline)
make coverage        # Gate vendored entity-slice class and predicate coverage
make acceptance      # Gate full transpile recall against external RDF snapshots
make mappings        # Build alignment axioms and VoID linksets from SSSOM mappings
make doc-lint        # Lint ontology-docs for dangling links and coverage gaps
```

The per-artifact `compile-*` commands were replaced by the unified generator
registry: every committed artifact under `generated/` is produced by a
registered generator with staging, source-hash banners, drift detection,
orphan detection, and the internal-tag leak gate (no `@x-gmeow-*` tag may
appear in a generated artifact; the statements compilation — the canonical
internal form — is the sole opt-out).

#### Annotation-completeness gate

`make validate` enforces that **every** GMEOW-namespaced term carries three
annotation properties:

* `rdfs:label` — human-readable name
* `skos:definition` — human-readable definition
* `rdfs:isDefinedBy` — pointer to the ontology / module / DSL vocabulary that
  owns the term

The gate applies to ontology headers, classes, object/datatype/annotation
properties, datatypes, and individuals. It is implemented in both native Rust
(`structural_lint_dataset` in [crates/validate](./crates/validate)) and SHACL
(`shapes/gmeow-shapes.ttl`, `shapes/mapping-dsl-shapes.ttl`,
`shapes/statement-dsl-shapes.ttl`). Missing annotations are **violations**, not
warnings. The DSL vocabularies are gated separately through their own SHACL
pass. New terms that lack these annotations will fail `make validate`
and CI (Principles 1, 6, 7).

### Refreshing & Committing Generated Artifacts

When you change canonical sources (ontology modules, mapping-dsl, statement-dsl), the checked-in generated artifacts can become stale. Use these targets to refresh and commit them safely:

The build's architecture — the in-memory carrier spine, the single `gmeow.gts` terminal, and the post-pipeline fanout that projects the flat files back out — is specified in [`docs/PIPELINE_SPINE.md`](./docs/PIPELINE_SPINE.md). Every committed artifact under `generated/` is a projection of `gmeow.gts`; that document is canonical for any work that produces one.

**One producer, one run.** `make check-sync` is the ONLY Make target that runs the regeneration pipeline, and `make check` drives it (in update mode) as the first node of the gate DAG. So the normal answer to "my generated tree is stale" is simply `make check`: it materializes and then gates, inside ONE hold of the host-global gate lock. Materializing separately first and then gating runs the whole pipeline twice and — because the second run must wait for the lock — queues the machine behind you. `make regen` was that second entry point; it is poisoned and refuses.

The doctrine behind the gate — why there is exactly one producer, why the host-global lock has no override, why the pipeline records while the gate grades, when a gate may read a record instead of recomputing it, and what puts a lane on `make heavy` instead of `make check` — is [`docs/GATE-AND-PIPELINE.md`](./docs/GATE-AND-PIPELINE.md). Read it before adding or moving a `make check` task, changing a nextest budget, blessing a ratchet baseline, or writing a comment that claims a gate enforces something; each of its rules carries the real defect in this repository that produced it, and it ends with a checklist for adding a gate task or a pipeline stage.

```bash
make check                                   # THE entry point: materialize, then gate
make check-sync SYNC_MODE=update             # Artifacts only, no gate (rarely what you want)
make check-sync                              # Strict read-only verification (mode=check is the default)
make check-sync SYNC_MODE=update SYNC_VERBOSE=1   # Stream live DAG stages and sync boundaries
make commit          # Run sync, stage the artifacts, and commit (default message)
make commit MESSAGE="feat: ..."  # Same, with a custom commit message
```

`make check-sync` runs the registered pipeline exactly once in topological order. `SYNC_OUTPUTS` selects the fanout scope: `generated` (the default — the bundle plus the `generated/` tree) or the wider `docs` and `all` profiles, which additionally fan out runtime `dist/` projections and external documentation. Narrowing the scope never weakens the selected profile's dependencies or gates. Independent generators at the same topological level use every available CPU by default; `--jobs N` is an explicit local override, never a hidden low thread cap. Use `SYNC_VERBOSE=1` (or `gmeow-dev sync --verbose`) to stream live DAG stages and synchronization boundaries. A worktree-local clean manifest under `.cache/gmeow-sync/manifests/` hashes canonical inputs and witnesses every managed output, so a warm fixed-point run skips the pipeline entirely. On a miss, cumulative carrier snapshots are recomputed in memory rather than serialized into a multi-gigabyte stage cache. Update mode writes only byte-changed files and removes stale owned outputs; check mode renders and validates without touching files.

* `generated/mappings/`, `generated/projections/`, `generated/queries/` — the `mappings` generator
* `generated/statements/` — the `statements` generator (RDF 1.2 lead + OWL downcast)
* `generated/metadata/void.ttl`, `generated/metadata/dcat.ttl` — the `metadata` generator
* `generated/apache/gmeow.conf` — the `apache` generator
* `generated/lpg/`, `generated/schemas/` — the `lpg` and `schemas` generators
* `generated/module-status.md` — the `matrix` generator (the per-slice audit ledger)

`make commit` stages only the generated artifacts above. If you also have source changes (for example in `dsl/mappings/` or a slice-local `mappings/` directory), stage them separately with `git add` before running `make commit`, or amend the commit afterward.

> [!TIP]
> If you suspect generated files are stale but do not want to commit yet, run `make check-sync SYNC_MODE=check`. It checks the complete fixed point without touching files.

### Release Outputs

```bash
make check-sync SYNC_MODE=update SYNC_OUTPUTS=docs # Regenerate external site/book/print/snippet/model docs
make build           # Build serializations and JSON-LD context into dist/
make project         # Project GMEOW data to external vocabulary profiles
make release         # Regenerate, native-reason, build, report, and emit CrossRef deposit
make release-sign-gts SIGN_KEY=/tmp/gpg/signing-key.asc GTS_OUT=dist/gmeow.gts
```

`make release-sign-gts` signs a freshly regenerated GTS snapshot for release
packaging. The committed `generated/dist/gmeow.gts` remains unsigned unless a
release workflow explicitly writes the signed copy there before packaging.

Documentation projections are never embedded in `gmeow.gts`. The static site,
mdbook sources, print PDF/Typst, prompt snippets, and generated model docs are
derived artifacts regenerated with `make check-sync SYNC_MODE=update SYNC_OUTPUTS=docs`; Pages CI must
use the source-backed `gmeow-dev sync --mode update --outputs docs` command. The
wider `SYNC_OUTPUTS=all` profile additionally materializes runtime export
projections such as OKF Markdown, JSON-LD, and YAML-LD — the gate's default
`SYNC_OUTPUTS=generated` scope produces the bundle and the `generated/` tree
only. This keeps the logical GTS carrier separate
from large, easily-derived presentation payloads (Principle 4).

### Reasoning & Negative Tests

```bash
make reason          # Native Docker-free EL/DL reasoning authority
make verify          # Native reasoned-graph negative tests
make reason-verify   # Native reasoning + reasoned-graph verify, one closure (Docker-free)
```

The native `logic:` engine is the single reasoning authority — Docker-free,
in-process, and the sole production forward authority. `make reason-verify`
shares one complete native result between reasoning and reasoned-graph
verification; there is no Java/Docker lane to run separately, and no live
second reasoner on-gate.

### Testing & Verification

```bash
make check           # Synchronize outputs, then run the local gate DAG (every task)
make heavy           # CI-ONLY breadth lane (wasm parity, transpile acceptance, golden soak)
make rust-test       # Run the Rust workspace tests (cargo nextest + doctests)
make clippy          # Run cargo clippy on all Rust targets with warnings as errors
make rust-build      # Compile Rust workspace test binaries without running them
```

`make check` owns the normal local synchronization boundary: it runs the
registered pipeline in update mode first, writes only byte-changed generated
artifacts, and then validates that exact fixed point. A clean manifest makes the
sync step effectively free. CI and direct `make check-sync` invocations retain
read-only check mode, so CI still fails on uncommitted drift rather than repairing
it.

`make check` physically executes every task in its DAG; there is no reuse or
selection profile. What it does own is an *accurate* dependency graph: a task
declares `sync` as a prerequisite if and only if it reads a `generated/` artifact,
so the lint, crate-layering, and translation gates start in the first scheduling
wave rather than queueing behind synchronization, and the Rust surface runs as four
concurrent siblings (`carrier-purity`, `clippy`, `nextest`, `doctests`) under one
`rust-build`. Use `make check CHECK_ARGS="--explain"` to print the wave plan
without running anything or taking the host gate lock, and
`CHECK_ARGS="--timings-json dist/check-timings.json"` to record per-task wall time.

`make heavy` is the CI-only companion: the lanes whose runtime is set by breadth
(a whole-external-corpus recall sweep, four release wasm builds plus four Node
execution lanes) or by a repeat-for-confidence soak. It refuses to run unless both
`CI=true` and a CI-vendor marker are set. Nothing was dropped — CI runs `make heavy`
on every PR — and each task stays runnable by name (`make wasm-parity`).

The entire toolchain is native Rust; there is no Python test suite. To run a
single crate's tests, use `cargo nextest run -p <crate>`.

Generic RDF 1.2 / RDF\* and SPARQL compliance belongs to PurRDF's own test
suite. GMEOW does not duplicate that authority with queries that merely prove
an upstream engine can execute. Repository query tests must assert expected
GMEOW product behaviour. Engine-independent coverage of GMEOW's native
reasoning calculus is retained without a live second engine: the committed,
frozen `dl_oracle_gold` corpus and the native gap-zero DL⊇EL crosscheck
ledger (Principles 4, 7, 18).

### Maintainer Tasks

All maintainer-only work is prefixed with `maint-`. These targets may use
Docker, Java, the network, heavyweight tests, or release/report-only tooling and
are intentionally outside the normal local `make check` path unless a workflow
calls them explicitly.

```bash
make maint-extract TARGET=foaf      # Import/extract policy for one target
make maint-refresh-target-axioms    # Re-vendor minimal target-axiom snapshots
make maint-wikidata-live            # Network existence checks for Wikidata IDs
make maint-wikidata-coverage        # Report Wikidata mapping coverage by domain
make maint-wikidata-audit           # Audit fixtures/modules for Wikidata misuse
make maint-rust-heavy               # Off-gate heavy Rust suite (maint-heavy profile)
make maint-quality                  # OOPS! network pitfall scan
make maint-evals-score              # Score committed model emissions
make maint-compliance-report-full   # Full compliance-report emission
```

#### Snapshot goldens (insta, T8)

The Rust suite pins large structured outputs — the logic projection back-ends,
the diagnostics/SARIF renderers, and the explanation prose — with
[`insta`](https://insta.rs) snapshot goldens (`*.snap` files committed next to
their tests). The renderers are pure and deterministic, so the goldens carry the
output verbatim with no redaction.

The review flow is **non-interactive** (no `cargo insta review` TTY prompt), so
it works in CI and agent runs:

```bash
make rust-test                 # default: HARD-FAILS on any snapshot drift (writes a *.snap.new)
make insta-review              # after an INTENTIONAL output change: regenerate the .snap goldens,
                               # then re-run in CI mode so the run still fails if anything is
                               # non-deterministic. Review the .snap diff, then commit.
```

Drift is a hard failure by design (`INSTA_UPDATE=no` in CI). Never auto-accept a
snapshot you have not inspected, and never leave a `*.snap.new` uncommitted. The
`.snap` files are the byte-exact unit golden; cross-engine *semantic* corpus
parity stays with the native `crates/conformance` harness (graph-isomorphism +
bless), which insta does not replace.

#### Suite quality & the gate-perf budget (benchmarks / coverage / mutation, T9)

The **always-on hard gate is `make rust-test`** (plus `make check`). Benchmarks,
coverage, and mutation testing are SLOW and **report-only** — they run **off the
required PR path**, scheduled + `workflow_dispatch` in
[`.github/workflows/suite-quality.yml`](./.github/workflows/suite-quality.yml),
exactly like the nightly fuzz job and the maintainer-only network lanes. Keeping slow
tools off-gate IS the documented **gate-perf budget**: nothing here can block a
PR, so the required lane stays fast.

```bash
make bench           # criterion hot-path benchmarks (host-tuned target-cpu=native);
                     #   reasoning (reason_all/el_closure/materialize_core), SHACL
                     #   validate, RDF layout, foundation chase, …
make bench-compare   # report-only perf scoreboard: live criterion run vs committed
                     # bench/baseline.json (ok|watch|regressed; always exits 0).
                     #   maint-bench-baseline refreshes the committed baseline + leaderboard.
make rust-coverage   # cargo-llvm-cov region coverage (lcov + HTML, --include-ffi); report-only.
                     #   Named NOT `coverage` — that is the entity-coverage gate.
make mutants         # cargo-mutants over the logic+validate cores (mutants.toml). Grades whether
                     #   the suite catches regressions. The full logic run is HOURS — scope
                     #   locally with MUTANTS_ARGS="-p gmeow-validate -f <file>".
```

A surviving mutant is a real test-strength gap: kill it by strengthening a test
(see the `constitution.rs` `literal_i64`/`literal_string` tests added),
or document why it is acceptable. Coverage/mutation/bench numbers are *evidence
to act on*, never a fabricated metric — report exactly what ran.

For optimization doctrine beyond the report-only benchmark commands, see
[`docs/RUST-OPTIMIZATION.md`](./docs/RUST-OPTIMIZATION.md). It is the standing
guide for static iterator seams, dense typed IDs, const generics/type-state,
targeted SIMD, sealed traits, deterministic output, and Cargo profile changes.

#### Test performance and long-running coverage

Wall-clock duration is **not a correctness gate**. `make rust-test`,
`make rust-gate`, and the CI Rust shards fail on correctness, lint, snapshot,
and genuine runaway failures — never because one test crossed an elapsed-time
threshold. Shared development servers and hosted runners vary too much in load,
cache warmth, and filesystem contention for a single wall-clock sample to be a
stable policy signal.

The nextest `slow-timeout` remains a 60-second repeated-progress backstop. It
terminates only after two slow periods and exists to catch hangs/runaways; it is
not a performance budget. There is no `gmeow-test-budget` post-processor and no
`GMEOW_TEST_BUDGET_*` enforcement surface.

Scheduling must scale with the host. Do not add fixed global or test-group
concurrency caps, especially one- or two-thread ceilings. Tests that themselves
start a full-width worker pool may reserve `threads-required = "num-cpus"` so
nextest avoids nested oversubscription; that reservation automatically uses the
available CPU count on both a small CI runner and a 32+ CPU development server.

Optimize from evidence rather than a threshold:

* use `make bench`, `make bench-compare`, allocation counters, profiles, and
  deterministic structural counts;
* compare the same production path before and after, recording cache state and
  host contention;
* remove redundant parsing, indexing, serialization, joins, fixture construction,
  and I/O in production code where possible;
* preserve semantic parity, deterministic output, and generated-artifact identity.

The default nextest filter may still place genuinely exhaustive whole-repository
proofs on `maint-heavy` when focused tests and dedicated drift gates cover the
same per-commit contract. This is an architectural lane distinction, not a
duration allowlist. Never move a test off the default profile merely because a
contended wall-clock sample was slow. `make maint-rust-heavy` remains the command
for the complete exhaustive lane.

#### GTS engines (moved to the `gmeow-gts` repo)

The four GTS engines (Python, Rust, Go, TypeScript) and the frozen conformance
corpus now live in the standalone
[`gmeow-gts`](https://github.com/Blackcat-Informatics/gmeow-gts) repo. This
ontology reads and writes the GTS format natively through the Rust `gts` codec
(the `purrdf` path); it does not depend on the external GTS package at build
time. The standalone `gts` CLI, when needed, is available via
`pip install gmeow-gts` or
`go install go.blackcatinformatics.ca/gts/cmd/gts@latest`.

> [!IMPORTANT]
> Always run `make check` locally and ensure it passes completely before proposing changes, committing, or submitting a PR.

---

## 3. How the Compilers Work

The Makefile is only a task runner. The actual compiler and validation logic lives in the native Rust crates under [crates/](./crates/) (e.g. the pipeline stages in `crates/pipeline` and structural linting in `crates/validate`), and the `gmeow` CLI is a native Rust binary ([crates/gmeow-cli](./crates/gmeow-cli)).

### Mapping Compiler

Mapping compilation runs inside `gmeow-dev sync --mode update --outputs generated` (the `mappings` generator), implemented by the native Rust pipeline stage in [crates/pipeline/src/stages/mappings.rs](./crates/pipeline/src/stages/mappings.rs) and the `gmeow-slice` emitters.

* **Canonical input**: all Turtle files under [dsl/mappings/](./dsl/mappings/) and every slice-local `slices/<group>/<name>/mappings/` directory. The shared DSL vocabulary remains [dsl/mappings/vocabulary.ttl](./dsl/mappings/vocabulary.ttl).
* **Generated outputs**:
  * `generated/mappings/*.sssom.tsv` — SSSOM term-equivalence rows.
  * `generated/projections/*.edoal.ttl` — EDOAL alignment cells.
  * `generated/projections/functions.fno.ttl` — generated FnO function catalog.
  * `generated/queries/projections/*.rq` — executable SPARQL CONSTRUCT projection queries.
* **Hand-authored companion file**: `dsl/mappings/transforms.fno.ttl` is read by the compiler/lints but is authored, never generated.
* **Important behavior**: the registered generator first renders artifacts into a staging product, runs projection cross-layer invariants, and only then writes generated files. If an invariant fails, nothing is written.
* **Drift check**: `make check-sync` renders into a staging tree, compares against the committed `generated/` artifacts, detects orphans, and enforces the internal-tag leak gate.

The mapping DSL has two main authoring units:

* a native RDF-1.2 alignment cell (a reified `S skos:*Match O {| … |}` statement) for pure cross-ontology links that compile to SSSOM rows.
* `gmeow:ProjectionMapping` for directional, possibly lossy projections that compile to SPARQL branches and, when applicable, EDOAL/FnO/SSSOM artifacts.

`logic:GroundingCorrespondence` is the explicit grounding marker on either a
native alignment cell or a single-binding
`gmeow:ProjectionMapping`. It requires `gmeow:justification`, named
`logic:sourceEndpoint` and `logic:targetEndpoint` values, and explicit
`logic:morphismClass`, `logic:morphismKind`, and `logic:preservationKind`
judgments; the complete contract is specified in
[`LOGIC-CORRESPONDENCE.md`](./slices/grounding/logic/design/LOGIC-CORRESPONDENCE.md).
The cell compiles to a shipped content-addressed `logic:Correspondence`. All
external grounding belongs to exactly one
grounding slice: linguistic and serialization catalogs in
[`slices/grounding/lang/mappings/`](./slices/grounding/lang/mappings/),
mathematical catalogs in
[`slices/grounding/math/mappings/`](./slices/grounding/math/mappings/), and
formal or upper-ontology catalogs in
[`slices/grounding/logic/mappings/`](./slices/grounding/logic/mappings/).
These laws ship in `gmeow.gts`; they are not disposable documentation
projections. A domain slice must use the grounding term and must not re-author
the external term or a competing correspondence. Incomplete marker metadata is
a validation error.

Do not patch a generated SSSOM, EDOAL, FnO, or projection query file directly to satisfy review feedback. Patch the DSL source, re-run the compiler, and include the regenerated artifacts.

### Statement Compiler

Statement compilation runs inside `gmeow-dev sync --mode update --outputs generated` (the `statements` generator), implemented by the native Rust stage in [crates/pipeline/src/stages/statements.rs](./crates/pipeline/src/stages/statements.rs) and the `gmeow-rdf` statement codec.

* **Canonical input**: all Turtle files under [dsl/statements/](./dsl/statements/), plus the DSL vocabulary in [dsl/statements/vocabulary.ttl](./dsl/statements/vocabulary.ttl).
* **Generated outputs**:
  * `generated/statements/gmeow.rdf12.ttl` — RDF 1.2 / RDF* lead artifact, written natively by the `gmeow-rdf` Rust codec (`gmeow_rdf.project_statements_rdf12`); no Java, no Docker, no SPARQL engine. rdflib cannot parse RDF 1.2 triple terms, so the native codec also supplies the OWL normal form for the round-trip check.
  * `generated/statements/gmeow-statements.owl.ttl` — OWL 2 axiom-annotation downcast consumed by OWL 2 DL reasoners.
* **Important behavior**: the DSL is plain Turtle that structurally mirrors RDF 1.2 reifying statements. The compiler emits the OWL form, projects it to RDF 1.2 natively with `gmeow-rdf`, then normalizes the RDF 1.2 form back to OWL and requires graph isomorphism before writing. Apache Jena re-reads the committed artifact only in the non-required `maint-statements-docker-check` oracle lane.
* **Drift check**: `make check-sync` performs the registered-generator check and fails if committed statement artifacts are stale.

Do not edit `generated/statements/gmeow.rdf12.ttl` or `generated/statements/gmeow-statements.owl.ttl` directly. If metadata is wrong, fix the `gmeow:StatementMetadata` cells in `dsl/statements/`.

### Generated Artifact Rule

Generated files contain a `GENERATED by ... DO NOT EDIT` banner where practical. Treat that as binding:

* Source changes belong in `slices/<group>/<name>/module.ttl`, slice-local `mappings/`, `dsl/mappings/`, `dsl/statements/`, shapes, queries, tests, or toolchain source.
* Generated artifact changes must be reproducible by `make check`.
* If a read-only `make check-sync` reports drift, run `make check` rather than hand-editing the output.
* If a generated artifact is nondeterministic, fix the compiler determinism bug. Do not normalize the artifact by hand.

### Vocabulary Index (llms.txt)

This project automatically generates a single-file, flat index of all classes,
properties, and individuals (with CURIEs, parent classes, and definitions) at
`dist/llms.txt` through the export stage of the registered build pipeline. It
is **not checked in** — run `make check` to produce it on demand.

If you are an agent trying to look up terms, resolve definitions, or discover vocabulary details, generate and ingest `dist/llms.txt` to get a clean, context-efficient overview of the entire ontology.

---

## 4. Directory Layout

**The one rule:** if a path is under `generated/`, a registered generator owns it and you never edit it; if it is under `dist/`, it is ephemeral and never committed; anything else is authored by a human.

**Exception:** `ontology-docs/` at the repository root is an ephemeral generated artifact owned by the `docs` registered generator. It lives outside `generated/` so GitHub Pages can publish it directly, but it is ignored and regenerated on demand with `make check-sync SYNC_MODE=update SYNC_OUTPUTS=docs` or `gmeow-dev sync --mode update --outputs docs`. It is never embedded in `generated/dist/gmeow.gts`.

```text
slices/<group>/<name>/   # THE unit of the ontology: a slice. The <group> segment
                         #   (core/, extensions/) is human organization only —
                         #   manifest.ttl is the SOLE source of identity (IRI) and
                         #   tier. Anatomy (discovered, never configured):
                         #   manifest.ttl, module.ttl, shapes.ttl, mappings/,
                         #   queries/, examples/, tests/, docs.md
slices/vocabulary.ttl    # The slice-manifest authoring vocabulary (spec layer)
ontology/gmeow.ttl # Root ontology = the CORE profile (generated imports)
dsl/mappings/            # Shared mapping vocabulary, cross-cutting sets,
                         #   shared equivalences, transforms.fno.ttl
dsl/statements/          # Statement DSL (canonical RDF 1.2 statement metadata)
shapes/                  # Authored SHACL (incl. slice-manifest-shapes.ttl)
queries/                 # Authored SPARQL: competency/, verify/, qc/, codecs/
imports/                 # Vendored externals (gUFO + validation snapshots)
docs/                    # Cross-slice doctrine docs (slice guides live IN slices)
governance/              # Project governance artifacts
generated/               # EVERY committed generated artifact, one root:
                         #   mappings/ projections/ queries/ statements/ schemas/
                         #   lpg/ metadata/ apache/ module-status.md
dist/                    # Ephemeral build products (one .gitignore line)
crates/                  # The native Rust toolchain: gmeow-cli (`gmeow …`),
                         #   gmeow-dev-cli, pipeline, validate, rdf/purrdf, gts codec, …
tests/                   # Cross-slice tests (slice-local tests live IN slices)
```

Slice rules (Principles 15–16): core slices interlink freely and reason as one union; **extension slices depend only on core** (the dependency DAG gate rejects extension→extension edges); every slice names its consumer in the manifest; every term is *declared* in exactly one slice. To add a slice, copy any core slice's anatomy — there is nothing else to learn. The generated `generated/module-status.md` matrix tracks tier, dependencies, and documentation status per slice.

## 5. Work Sizing: Decompose, Never Defer

A work item is well-formed only if it is **completable as a reasonable unit** — one
branch that lands via a single squash-merge with `make check` green. An issue that no
single PR can ever close is not an "epic," it is a planning defect: unbounded,
unownable, and self-perpetuating.

When a proposed unit is too large to land that way, **decompose it into blocking and
dependent sub-issues before writing code:**

* each child is itself a reasonable unit (one landable PR);
* the children form an explicit dependency DAG — every child names what **blocks** it
  and what it **blocks** (`Blocked by: #N` / `Blocks: #N`), so the build order is
  derivable;
* the original becomes a **tracking epic** (label `epic`) whose body lists its children
  and carries no code of its own; it closes only when every child has closed;
* every requirement of the original maps to exactly one child — nothing is dropped.

**Decomposition is not deferral.** The `.baseline` rule — *no deferrals, no follow-ups,
work is NOW* — bans silently dropping or postponing a requirement **inside the unit you
are landing**. It does **not** license cramming an oversized problem into one
unreviewable, monolithic PR (the failure mode that collapses under its own review
surface). The distinction is sharp:

* **Deferral (banned):** a requirement inside the unit you are landing is left
  unimplemented, stubbed, or tagged "future work."
* **Decomposition (required):** an oversized unit is split so every requirement lands in
  a tracked, blocking/dependent child — each *now*, in dependency order, none dropped.

"Work is NOW" means the decomposition happens now and each unblocked child is built now —
never that a hard problem is forced through a single monolithic PR.

## 6. PR Lifecycle: Integrate, Review, Push

When a PR is open and feedback arrives, follow this cycle strictly.

### Integrate latest main

Refresh a stale branch by **merging current `main` into it** — never rebase. `main` enforces
linear history, which the final squash-merge (`ghprsq`, see § Finalize) provides; the branch may
freely contain merge commits because the squash collapses them.

```bash
git fetch origin main
git merge origin/main
```

Resolve **each conflict individually** — read both sides and keep maximum functionality. The
following are forbidden (they discard a side wholesale and are an ETHOS deal-breaker):

```bash
# ❌ NEVER
git checkout --theirs .   ;   git checkout --ours .
git merge -X theirs       ;   git merge -X ours
```

For conflicts in `generated/*` artifacts, do **not** hand-edit or hand-pick a side — regenerate
them from canonical sources after the merge. `generated/dist/gmeow.gts` is a git-ignored local
product (never tracked, so it never produces a merge conflict), but it still holds pre-merge bytes
on disk until you re-materialize it:

```bash
make check          # re-materialize generated/ (including the bundle) on the merged base, then gate
```

One command, not two: `make check` runs the producer itself and then proves the
result, so a separate materialize-first step would run the whole pipeline twice
against the same host-global gate lock.

#### Integrating the `generated/` untracking transition

`generated/` was a committed tree and is now a git-ignored local product. When you integrate the
`main` revision that removed it from tracking, apply these rules — the transition is one-way, and
**deletion always wins**:

* **A branch that never touched `generated/`** merges cleanly: `main`'s deletion of those paths
  applies against your unchanged copies with no conflict. Nothing to resolve — just re-materialize
  afterward with `make check`.
* **A branch that modified `generated/`** hits delete/modify conflicts (your side edited a path
  `main` deleted). Resolve **every one in favor of the deletion** (`git rm <path>` for each
  conflicted `generated/` path) — never re-add the file, never hand-pick your edited bytes. Your
  intent lives in the canonical *sources*; once the tree is deleted, run `make check` to regenerate
  it locally from those sources and confirm the change landed in the materialized product.
* **Never `git add -f generated/`.** The path is git-ignored on purpose; force-adding it re-commits
  the product and re-introduces exactly the coupling this change removed. If you think you need to
  force-add a `generated/` file, you are working around the ignore rule instead of fixing the source.

An older clone or long-lived branch stays perfectly usable across this change — it only needs one
`make check` after integrating `main`. (A separate, later change will rewrite retained history to
drop `generated/` from past commits; only *after* that rewrite do stale clones/branches become
unsafe and require a fresh clone. This branch does not perform that rewrite.)

### Pull review feedback

Use the GitHub CLI to inspect all comments and reviews. **Important distinction:**

* `gh pr view --json comments` only returns **top-level PR comments** (not inline review threads).
* `gh api repos/<owner>/<repo>/pulls/<PR_NUMBER>/comments` returns **inline review comments** — this is where most actionable feedback lives.

Recommended inspection sequence:

```bash
# 1. Top-level review summaries (human + bot overview)
gh pr view <PR_NUMBER> --json reviews

# 2. Inline review comments with file/line context (the actionable items)
gh api repos/<owner>/<repo>/pulls/<PR_NUMBER>/comments \
    | jq -r '.[] | "File: \(.path)\nLine: \(.line)\nBody: \(.body)\n---"'

# 3. Compact scan: sort by recency to spot new feedback quickly
gh api repos/<owner>/<repo>/pulls/<PR_NUMBER>/comments \
    | jq -r '.[] | "\(.user.login) | \(.path):\(.line) | \(.body[:200])"'

# 4. Filter to a specific file/line when chasing a known issue
gh api repos/<owner>/<repo>/pulls/<PR_NUMBER>/comments \
    | jq -r '.[] | select(.path == "crates/validate/src/repo_static.rs" and .line == 100) | .body'
```

Read both automated (CodeRabbit, Gemini) and human reviews. Treat actionable automated feedback as binding unless it contradicts the ontology design principles in [CONSTITUTION.md](./CONSTITUTION.md).

### Address feedback

Apply fixes **only in canonical source files** (Principle 4):

| Review target | Canonical source to edit |
|---|---|
| SSSOM / EDOAL / FnO / projection queries | owning slice's `mappings/`, or `dsl/mappings/` for genuinely cross-cutting sources |
| RDF 1.2 / OWL statement artifacts | `dsl/statements/` |
| Ontology terms, axioms, observation bridges | `slices/<group>/<name>/module.ttl` |
| SHACL shapes | `shapes/` |
| Tests, fixtures | `tests/` |

Never patch generated artifacts by hand. After editing canonical sources, regenerate:

```bash
make check          # after ANY canonical-source change: materializes, then gates
```

### Validate before pushing

```bash
make check
```

All Docker-free local gates must have passing evidence: lint, validate,
generated-artifact drift check, native reasoning, native verify (one closure via
`make reason-verify`), and the Rust tests.

### Push

Commit your changes and push — no rebase, no amend, no force-push (the branch only *gains*
commits, including the merge from § Integrate, so a normal push fast-forwards the remote):

```bash
git add <explicit paths>   # stage explicit paths, never `git add -A` in a shared checkout
git commit
git push
```

### Finalize

The PR lands as a **squash-merge via `ghprsq`** — never `git merge` / `gh pr merge`. The squash
collapses the whole branch (your commits *and* the merge commits from § Integrate) into one commit
on `main`, so `main` stays linear. See the `/stage3` finalization flow.
