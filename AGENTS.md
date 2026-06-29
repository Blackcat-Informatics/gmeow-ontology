<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# AI Developer Agent Guide (AGENTS.md)

Welcome, AI Agent! This file is your behavioral contract and instruction manual for contributing to the GMEOW repository. Please read and adhere strictly to the rules below.

---

## 1. Project Overview & Architecture

GMEOW is a **reasoning-centric, OWL 2 DL, upper-ontology-grounded super-vocabulary** that unifies document metadata, entity descriptions, legal agreements, contacts, and person-centric data.

Every design decision, code modification, and schema change is governed by the twelve principles of the [CONSTITUTION.md](./CONSTITUTION.md). Cite these principles by number (e.g., `"Principle 4"`) in your commit messages, pull requests, and discussions.

### Critical Ontological Rules

* **One Canonical Source (Principle 4)**: Do not hand-edit generated files. Any change must be made in the canonical source files.
  * **Mappings**: Authored in [dsl/mappings/](./dsl/mappings/) -> compiled to `generated/`
  * **Statements**: Authored in [dsl/statements/](./dsl/statements/) -> compiled to `generated/statements/`.
  * **Citations**: Authored in [metadata/references.ttl](./metadata/references.ttl) -> compiled to [generated/references/](./generated/references/). Follow [docs/CITATIONS.md](./docs/CITATIONS.md) when adding external sources, standards, issues, PRs, or review-thread citations.
* **RDF 1.2 / RDF\*-first (Principles 2 & 3)**: Statement-level metadata (provenance, confidence, temporal scope) is authored as native RDF 1.2 / RDF\* in the statement DSL. The logical core stays OWL 2 DL.
* **Co-equal & Non-privileged (Principles 9 & 10)**: There is no `primaryName`, `preferredGender`, or single-winner preference. A contested fact is represented as coexisting standpoint-indexed claims. A superseded label/deadname is suppressed using `gmeow:displayable false` rather than deleted.

---

## 2. Core Toolchain & Commands

The repository uses Python (`uv`), Rust/Cargo, and Docker only for explicit
maintainer/oracle lanes. The Makefile is the definitive task-oriented plan:
run `make help` first when you need the current target surface, and use `make`
targets rather than calling `gmeow-dev`, `cargo`, or helper scripts directly
unless you are adding or debugging the target itself.

### The CLI razor — `gmeow` vs `gmeow-dev` (#517)

There are two CLIs, and a single razor decides where a command belongs:

> **`gmeow` does not need a repo; `gmeow-dev` does.**

* **`gmeow`** ([src/gmeow_tools/cli.py](./src/gmeow_tools/cli.py)) is the public, PyPI-facing surface. Every command must work from the installed wheel alone — backed by the bundled `generated/dist/gmeow.gts` snapshot — with **no source checkout, Docker, generator inputs, or repo-local query trees**. Transpiling a user's own RDF, describing a term, verifying the bundle: consumer operations, so `gmeow`.
* **`gmeow-dev`** ([src/gmeow_tools/cli_dev.py](./src/gmeow_tools/cli_dev.py)) is repository maintenance. It may read anything in the tree — `dsl/`, `generated/`, `imports/`, `tests/fixtures/` — because it only ever runs inside a checkout. Regenerating artifacts, scoring coverage against the dev corpus, refreshing vendored snapshots: developer operations, so `gmeow-dev`.

When adding a command, ask the razor first. If it needs a repo path that the wheel does not bundle, it is `gmeow-dev` — or the data it needs must first be bundled so it can be `gmeow`.

### Environment & Formatting

```bash
make help            # Show the grouped task plan
make install         # Sync uv and configure repo-local Git merge drivers
make fmt             # Auto-format Python files with ruff
make lint            # Run ruff check, ruff format --check, and mypy
make clean           # Remove ephemeral build artifacts and native build stamps
```

`make install` also runs `scripts/bootstrap-git-merge-drivers.sh`, which sets
`merge.ours.driver=true` in the local Git config. That driver backs the
`.gitattributes` rule for `generated/dist/gmeow.gts`: Git keeps the current
side during binary bundle merges/rebases, and the developer regenerates/checks
the bundle from canonical sources afterward.

### Validation & Compilation

```bash
make validate        # Validate Turtle syntax, term annotations, and SHACL
make validate-gts    # Validate generated/dist/gmeow.gts
make regenerate      # Rebuild ALL committed generated artifacts (the #279 registry; parallel by default)
make check-generated # Drift + orphan + internal-tag-leak check for every registered generator (parallel by default)
make constitution-check # Every principle has live enforcement (governance/constitution.ttl, #280)
make crate-check     # Verify Rust crate layering and acyclic crate DAGs
make wikidata        # Validate Wikidata QID/PID syntax in the mappings (offline)
make coverage        # Gate vendored entity-slice class and predicate coverage
make acceptance      # Gate full transpile recall against external RDF snapshots
make mappings        # Build alignment axioms and VoID linksets from SSSOM mappings
make doc-lint        # Lint ontology-docs for dangling links and coverage gaps
```

The per-artifact `compile-*` commands were replaced by the unified generator
registry (#279): every committed artifact under `generated/` is produced by a
registered generator with staging, source-hash banners, drift detection,
orphan detection, and the internal-tag leak gate (no `@x-gmeow-*` tag may
appear in a generated artifact; the statements compilation — the canonical
internal form — is the sole opt-out).

#### Annotation-completeness gate (issue #221)

`make validate` enforces that **every** GMEOW-namespaced term carries three
annotation properties:

* `rdfs:label` — human-readable name
* `skos:definition` — human-readable definition
* `rdfs:isDefinedBy` — pointer to the ontology / module / DSL vocabulary that
  owns the term

The gate applies to ontology headers, classes, object/datatype/annotation
properties, datatypes, and individuals. It is implemented in both Python
(`structural_lint()` in `src/gmeow_tools/validate.py`) and SHACL
(`shapes/gmeow-shapes.ttl`, `shapes/mapping-dsl-shapes.ttl`,
`shapes/statement-dsl-shapes.ttl`). Missing annotations are **violations**, not
warnings. The DSL vocabularies are gated separately through
`_dsl_shacl()`. New terms that lack these annotations will fail `make validate`
and CI (Principles 1, 6, 7).

### Refreshing & Committing Generated Artifacts

When you change canonical sources (ontology modules, mapping-dsl, statement-dsl), the checked-in generated artifacts can become stale. Use these targets to refresh and commit them safely:

```bash
make regenerate      # Rebuild ALL checked-in generated artifacts from canonical sources
make commit          # Run regenerate, stage the artifacts, and commit (default message)
make commit MESSAGE="feat: ..."  # Same, with a custom commit message
```

`make regenerate` runs the registered generators in topological order. Independent generators at the same topological level execute in parallel (default `-j` capped at the CPU count, with a memory-aware ceiling), and a source/output hash stamp cache under `.stamps/generators/` lets `regenerate` and `check-generated` skip generators whose inputs, implementation, and committed outputs have not changed. Override with `--no-skip-unchanged` or `-j 1` via the CLI if needed. It refreshes everything under `generated/`:

* `generated/mappings/`, `generated/projections/`, `generated/queries/` — the `mappings` generator
* `generated/statements/` — the `statements` generator (RDF 1.2 lead + OWL downcast)
* `generated/metadata/void.ttl`, `generated/metadata/dcat.ttl` — the `metadata` generator
* `generated/apache/gmeow.conf` — the `apache` generator
* `generated/lpg/`, `generated/schemas/` — the `lpg` and `schemas` generators
* `generated/module-status.md` — the `matrix` generator (the per-slice audit ledger)

`make commit` stages only the generated artifacts above. If you also have source changes (e.g. in `dsl/mappings/`), stage them separately with `git add` before running `make commit`, or amend the commit afterward.

> [!TIP]
> If you suspect generated files are stale but do not want to commit yet, run `make regenerate` followed by `make check-generated` to verify the full gate still passes.

### Release Outputs

```bash
make docs            # Regenerate gmeow.gts docs and extract ontology-docs/
make build           # Build serializations and JSON-LD context into dist/
make project         # Project GMEOW data to external vocabulary profiles
make release         # Regenerate, native-reason, build, report, and emit CrossRef deposit
make release-sign-gts SIGN_KEY=/tmp/gpg/signing-key.asc GTS_OUT=dist/gmeow.gts
```

`make release-sign-gts` signs a freshly regenerated GTS snapshot for release
packaging. The committed `generated/dist/gmeow.gts` remains unsigned unless a
release workflow explicitly writes the signed copy there before packaging.

### Reasoning & Negative Tests

```bash
make reason          # Native Docker-free EL/DL reasoning authority
make verify          # Native reasoned-graph negative tests
make maint-reason-hermit # Full complete consistency check with HermiT (Docker oracle)
make maint-explain   # Explain any unsatisfiable classes (HermiT/Docker oracle)
make maint-verify-docker # ROBOT/ELK reasoned-graph verification
make maint-classic-cross-check # Full non-required Docker/Java oracle lane
```

### Testing & Verification

```bash
make test            # Run the pytest test suite (Python/SPARQL competency tests)
make check           # Run FULL gate: lint, validate, compilation check, reason, verify, tests
make rust-test       # Run the Rust workspace tests (cargo nextest + doctests)
make clippy          # Run cargo clippy on all Rust targets with warnings as errors
make rust-build      # Compile Rust workspace test binaries without running them
make test-fast       # Run the fast Python test lane used by make check
```

### Maintainer Tasks

All maintainer-only work is prefixed with `maint-`. These targets may use
Docker, Java, the network, heavyweight tests, or release/report-only tooling and
are intentionally outside the normal local `make check` path unless a workflow
calls them explicitly.

```bash
make maint-classic-cross-check      # Full Docker/Java oracle lane
make maint-reasoning-cases          # Docker-backed reasoning fixture cases
make maint-statements-docker-check  # Jena/ROBOT statement artifact oracle checks
make maint-crosscheck               # rdflib/native query-answer cross-check
make maint-extract TARGET=foaf      # Import/extract policy for one target
make maint-refresh-target-axioms    # Re-vendor minimal target-axiom snapshots
make maint-wikidata-live            # Network existence checks for Wikidata IDs
make maint-wikidata-coverage        # Report Wikidata mapping coverage by domain
make maint-wikidata-audit           # Audit fixtures/modules for Wikidata misuse
make maint-test-heavy               # Kept Python maintainer tests
make maint-test-network             # Live network tests
make maint-pull-images              # Pull/build pinned Docker oracle images
make maint-quality                  # OOPS! network pitfall scan
make maint-evals-score              # Score committed model emissions
make maint-compliance-report-full   # Full compliance-report emission
```

#### Snapshot goldens (insta, T8 #789)

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

#### Suite quality & the gate-perf budget (benchmarks / coverage / mutation, T9 #790)

The **always-on hard gate is `make rust-test`** (plus `make check`). Benchmarks,
coverage, and mutation testing are SLOW and **report-only** — they run **off the
required PR path**, scheduled + `workflow_dispatch` in
[`.github/workflows/suite-quality.yml`](./.github/workflows/suite-quality.yml),
exactly like the nightly fuzz job (#788) and the HermiT oracle. Keeping slow
tools off-gate IS the documented **gate-perf budget**: nothing here can block a
PR, so the required lane stays fast.

```bash
make bench           # criterion hot-path benchmarks (host-tuned target-cpu=native);
                     #   reasoning (reason_all/el_closure/materialize_core), SHACL
                     #   validate, RDF layout, foundation chase, …
make bench-compare   # report-only perf scoreboard: live criterion run vs committed
                     #   bench/baseline.json (ok|watch|regressed; always exits 0, #668).
                     #   maint-bench-baseline refreshes the committed baseline + leaderboard.
make rust-coverage   # cargo-llvm-cov region coverage (lcov + HTML, --include-ffi); report-only.
                     #   Named NOT `coverage` — that is the Python entity-coverage gate.
make mutants         # cargo-mutants over the logic+validate cores (mutants.toml). Grades whether
                     #   the suite catches regressions. The full logic run is HOURS (nemo) — scope
                     #   locally with MUTANTS_ARGS="-p gmeow-validate -f <file>".
```

A surviving mutant is a real test-strength gap: kill it by strengthening a test
(see the `constitution.rs` `literal_i64`/`literal_string` tests added in #790),
or document why it is acceptable. Coverage/mutation/bench numbers are *evidence
to act on*, never a fabricated metric — report exactly what ran.

#### The 25 s per-test budget (#1045)

**Every test on the always-on gate must complete under 25 s of real wall time.**
The policy is *enforced*, not advisory: `make rust-test` / `make rust-gate` (and the
CI rust shards) run `cargo nextest run --profile ci` and then
`cargo run -p gmeow-test-budget -- target/nextest/ci/junit.xml`, which parses the
JUnit report and **hard-fails if any test exceeds the budget**
(`crates/test-budget`, std-only, no Python). Override the threshold with
`GMEOW_TEST_BUDGET_SECS` only for local experiments — never to weaken the gate.

The nextest `slow-timeout` (`.config/nextest.toml`) stays at the 120 s terminate
cliff purely as a runaway/hang backstop; the 25 s **policy** is the JUnit gate.

A test that is *irreducibly* heavier than the budget (full-fold reasoning, full-DAG
parity, snapshot codec round-trips, Nemo closures, the native-pathological corpus
parity query) does NOT get a per-test timeout override. Instead it is **carved out of the default/ci profiles and runs on
the `maint-heavy` profile** (`make maint-rust-heavy`), so its coverage still runs —
off the per-commit gate, on a maint/scheduled lane. The single source of truth for
what is off-gate is the **`default-filter` expression in `.config/nextest.toml`**;
every excluded group is justified by an inline comment there. Adding a new off-gate
exception requires a comment in that filter AND a one-line entry here.

Default off-gate groups (2026-06-27, #1045): `gmeow-logic::ontology_entailments`
(Nemo RL); the `gmeow-logic`
`sparql_path_parity` binary + `sparql_path_lower::tests` module (#914 S8 property
paths — every case drives `run_scryer`, paying a ~9-10 s Scryer
machine-construction floor that process-per-test cannot share; several reach
19-34 s; engine-construction-bound like `ontology_entailments`); the
`gmeow-pipeline` `end_to_end`/`fold_parity`/full-fold/snapshot-codec/mapping-parity
tests, including the twin EDOAL/SPARQL corpus parity oracles
`edoal_lowering_matches_committed_corpus` +
`sparql_lowering_matches_committed_corpus_modulo_order` (both lower from one shared
get-leg model over the full DSL+ontology merge — irreducibly O(ontology-size));
scoreboards-acceptance
tests; the `gmeow-pipeline`
`dist_jsonld_roundtrips_through_oxigraph` + `product_routing`
`compiler_products_are_first_class_dag_artifacts` tests (both round-trip / route the
full shipped bundle, which now carries the signal-dense per-correspondence
preservation loss ledger — ~5 residue notes per alignment correspondence — so the
artifact is irreducibly large) and `reason_produces_nonempty_artifacts` (the reason
stage's O(ontology-size) full-graph native reason, ~14 s solo, crosses the 25 s
budget under gate contention as the ontology grows — fast per-commit reason coverage
is `make reason` + the gmeow-logic unit reason tests); a Nemo conformance case; a few whole-ontology
`gmeow-slice`/`gmeow-slicetest` emit/closure checks; the off-gate corpus parity
queries (`OFF_GATE_HEAVY` in `crates/rdf/tests/sparql_eval_parity.rs` — now six:
the `class-without-stereotype` anti-join, the ~107 s `ontolex` projection outlier,
and the heaviest generated CONSTRUCT projections `schema-org`/`vcard`/`foaf`/
`missing-definitions` whose per-shard aggregate × CI slowdown blew the budget);
`gmeow-rdf-capi::c_smoke` (self-builds the libpurrdf cdylib, ~33 s cold
compile on CI — build-time-bound, already covered by the dedicated `capi` CI job);
`w3c_rdfc10_heavy_offgate` (the sole RDFC-1.0 negative/poison vector `test074`,
~5.3 s on the call-budget guard — the rest of the W3C suite is sharded+gated, each
shard under 1 s); and the `gmeow-validate`
`deep_surfaces_entailed_inconsistency_tier1_misses_heavy_offgate` test (the consumer
`gmeow validate --deep` AC1: reasons over user data merged with the whole bundled
TBox via the native Nemo chase, ~50 s full-fold — engine-bound like
`ontology_entailments`; the same merge→inconsistency path is covered on-gate by the
fast tiny-TBox `gmeow-logic` unit `reason_all_with_data_*` plus the on-gate
`deep_pass_failure_*`/`deep_false_*` validate tests); and the `gmeow-pipeline`
`stages::yaml_ld::tests::dist_jsonld_roundtrips_through_oxigraph` test (parses the
entire dist JSON-LD into oxigraph and serializes it back — an O(ontology size)
round-trip at ~27 s that crosses the zero-headroom 25 s budget as the ontology
grows, the same growth pattern as the docs live-renders; build/parse-bound, and the
JSON-LD codec is exercised on-gate by the cheaper per-vocab round-trips).

Nearly the whole `gmeow-docs` test cluster is **on-gate**: each test loads a shared
`DocsModel` *and* the rendered site for every available language from the
content-addressed `gmeow_docs::fixture` cache instead of rebuilding or re-rendering.
The cache is primed once before the run — the `prime-docs-fixture` example, run by
the Makefile test lanes and the CI test job ahead of `nextest` — so no test pays the
~12 s build, the cold concurrent-rebuild contention, or a per-language site render. A
plain `cargo test` still works (a miss falls through to a build/render). The exception
is the **live-full-render guards** — `render_site_is_byte_stable` and
`english_carrier_tree_matches_render_site`: each performs a LIVE whole-site render of
the full model and compares it to the cached render. The live render IS the thing
under test (render determinism and the carrier-vs-`render_site` identity), so it
cannot be served from cache; that O(terms) render sits at ~23-26 s and crosses the
zero-headroom 25 s budget as the ontology grows, so the two stay off-gate (still run
on maint-heavy). The language-comparison round-trips (`french_tree`, `chinese_tree`,
and the translated no-dangling-link check) only need a render's *output*, so they read
the per-language cache and are on-gate; every single-page / single-term docs test
stays on-gate.

The bias is **fix, don't off-gate**: prefer making a test fast (shard it like the
corpus parity sweep in `crates/rdf/tests/sparql_eval_parity.rs`, or share an
expensive fixture once per run like the docs-model cluster) over moving it to
maint-heavy. Off-gate is for the genuinely irreducible.

#### GTS engines (moved to the `gmeow-gts` repo)

The four GTS engines (Python, Rust, Go, TypeScript) and the frozen conformance
corpus now live in the standalone
[`gmeow-gts`](https://github.com/Blackcat-Informatics/gmeow-gts) repo. The ontology
consumes the published `gts` Python package (PyPI: `gmeow-gts`) through the
narrow-waist glue in `src/gmeow_tools/gts_*`. The `gts` CLI, when needed, is
available via `cargo install gmeow-gts`, `pip install gmeow-gts`, or
`go install go.blackcatinformatics.ca/gts/cmd/gts@latest`.

> [!IMPORTANT]
> Always run `make check` locally and ensure it passes completely before proposing changes, committing, or submitting a PR.

---

## 3. How the Compilers Work

The Makefile is only a task runner. The actual compiler and validation logic lives in [src/gmeow_tools/](./src/gmeow_tools/), and the `gmeow` CLI is a thin orchestration layer over focused Python modules.

### Mapping Compiler

Mapping compilation runs inside `gmeow regenerate` (the `mappings` generator), implemented by the native Rust pipeline stage in [crates/pipeline/src/stages/mappings.rs](./crates/pipeline/src/stages/mappings.rs) and the `gmeow-slice` emitters.

* **Canonical input**: all Turtle files under [dsl/mappings/](./dsl/mappings/), plus the DSL vocabulary in [dsl/mappings/vocabulary.ttl](./dsl/mappings/vocabulary.ttl).
* **Generated outputs**:
  * `mappings/*.sssom.tsv` — SSSOM term-equivalence rows.
  * `projections/*.edoal.ttl` — EDOAL alignment cells.
  * `projections/functions.fno.ttl` — generated FnO function catalog.
  * `queries/projections/*.rq` — executable SPARQL CONSTRUCT projection queries.
* **Hand-authored companion file**: `dsl/mappings/transforms.fno.ttl` is read by the compiler/lints but is authored, never generated.
* **Important behavior**: the registered generator first renders artifacts into a staging product, runs projection cross-layer invariants, and only then writes generated files. If an invariant fails, nothing is written.
* **Drift check**: `make check-generated` renders into a staging tree, compares against the committed `generated/` artifacts, detects orphans, and enforces the internal-tag leak gate.

The mapping DSL has two main authoring units:

* `gmeow:TermEquivalence` for pure cross-ontology links that compile to SSSOM rows.
* `gmeow:ProjectionMapping` for directional, possibly lossy projections that compile to SPARQL branches and, when applicable, EDOAL/FnO/SSSOM artifacts.

Do not patch a generated SSSOM, EDOAL, FnO, or projection query file directly to satisfy review feedback. Patch the DSL source, re-run the compiler, and include the regenerated artifacts.

### Statement Compiler

Statement compilation runs inside `gmeow regenerate` (the `statements` generator), implemented by the native Rust stage in [crates/pipeline/src/stages/statements.rs](./crates/pipeline/src/stages/statements.rs) and the `gmeow-rdf` statement codec.

* **Canonical input**: all Turtle files under [dsl/statements/](./dsl/statements/), plus the DSL vocabulary in [dsl/statements/vocabulary.ttl](./dsl/statements/vocabulary.ttl).
* **Generated outputs**:
  * `generated/statements/gmeow.rdf12.ttl` — RDF 1.2 / RDF* lead artifact, written natively by the `gmeow-rdf` Rust codec (`gmeow_rdf.project_statements_rdf12`); no Java, no Docker, no SPARQL engine (#667). rdflib cannot parse RDF 1.2 triple terms, so the native codec also supplies the OWL normal form for the round-trip check.
  * `generated/statements/gmeow-statements.owl.ttl` — OWL 2 axiom-annotation downcast consumed by OWL 2 DL reasoners.
* **Important behavior**: the DSL is plain Turtle that structurally mirrors RDF 1.2 reifying statements. The compiler emits the OWL form, projects it to RDF 1.2 natively with `gmeow-rdf`, then normalizes the RDF 1.2 form back to OWL and requires graph isomorphism before writing. Apache Jena re-reads the committed artifact only in the non-required `classic-cross-check` oracle lane.
* **Drift check**: `make check-generated` performs the registered-generator check and fails if committed statement artifacts are stale.

Do not edit `generated/statements/gmeow.rdf12.ttl` or `generated/statements/gmeow-statements.owl.ttl` directly. If metadata is wrong, fix the `gmeow:StatementMetadata` cells in `dsl/statements/`.

### Generated Artifact Rule

Generated files contain a `GENERATED by ... DO NOT EDIT` banner where practical. Treat that as binding:

* Source changes belong in `slices/<group>/<name>/module.ttl`, `dsl/mappings/`, `dsl/statements/`, shapes, queries, tests, or toolchain source.
* Generated artifact changes must be reproducible by `make regenerate`.
* If `make check-generated` reports drift, run `make regenerate` rather than hand-editing the output.
* If a generated artifact is nondeterministic, fix the compiler determinism bug. Do not normalize the artifact by hand.

### Vocabulary Index (llms.txt)

This project automatically generates a single-file, flat index of all classes,
properties, and individuals (with CURIEs, parent classes, and definitions) at
`dist/llms.txt` through the export stage of the registered build pipeline. It
is **not checked in** — run `make regenerate` to produce it on demand.

If you are an agent trying to look up terms, resolve definitions, or discover vocabulary details, generate and ingest `dist/llms.txt` to get a clean, context-efficient overview of the entire ontology.

---

## 4. Directory Layout

**The one rule (#287):** if a path is under `generated/`, a registered generator owns it and you never edit it; if it is under `dist/`, it is ephemeral and never committed; anything else is authored by a human.

**Exception (#440):** `ontology-docs/` at the repository root is a committed generated artifact owned by the `ontology-docs` registered generator. It lives outside `generated/` so it can be hosted directly (e.g. GitHub Pages). The same generator code rebuilds the site independently inside the `gts` generator and embeds it in `generated/dist/gmeow.gts`, so the offline snapshot does not depend on the committed `ontology-docs/` directory.

```text
slices/<group>/<name>/   # THE unit of the ontology: a slice. The <group> segment
                         #   (core/, extensions/) is human organization only —
                         #   manifest.ttl is the SOLE source of identity (IRI) and
                         #   tier. Anatomy (discovered, never configured):
                         #   manifest.ttl, module.ttl, shapes.ttl, mappings/,
                         #   queries/, examples/, tests/, docs.md
slices/vocabulary.ttl    # The slice-manifest authoring vocabulary (spec layer)
ontology/gmeow.ttl       # Root ontology = the CORE profile (generated imports, #330)
dsl/mappings/            # Mapping DSL: vocabulary, foundational bridge, per-target
                         #   projections, shared equivalences, transforms.fno.ttl
dsl/statements/          # Statement DSL (canonical RDF 1.2 statement metadata)
shapes/                  # Authored SHACL (incl. slice-manifest-shapes.ttl)
queries/                 # Authored SPARQL: competency/, verify/, qc/, codecs/
imports/                 # Vendored externals (gUFO + validation snapshots)
docs/                    # Cross-slice doctrine docs (slice guides live IN slices)
generated/               # EVERY committed generated artifact, one root:
                         #   mappings/ projections/ queries/ statements/ schemas/
                         #   lpg/ metadata/ apache/ module-status.md
dist/                    # Ephemeral build products (one .gitignore line)
src/gmeow_tools/         # The toolchain (CLI: `gmeow …`)
                         #   gts_* = narrow-waist glue over the external gmeow-gts package
tests/                   # Cross-slice tests (slice-local tests live IN slices)
```

Slice rules (Principles 15–16): core slices interlink freely and reason as one union; **extension slices depend only on core** (the dependency DAG gate rejects extension→extension edges); every slice names its consumer in the manifest; every term is *declared* in exactly one slice. To add a slice, copy any core slice's anatomy — there is nothing else to learn. The generated `generated/module-status.md` matrix tracks tier, dependencies, and documentation status per slice.

## 5. PR Lifecycle: Integrate, Review, Push

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
them from canonical sources after the merge. `generated/dist/gmeow.gts` is `merge=ours` (it keeps
your branch copy without a conflict marker), so it too must be regenerated and committed:

```bash
make regenerate          # reconcile all generated artifacts on the merged base
make check-generated     # verify no drift remains
```

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
    | jq -r '.[] | select(.path == "src/gmeow_tools/runner.py" and .line == 98) | .body'
```

Read both automated (CodeRabbit, Gemini) and human reviews. Treat actionable automated feedback as binding unless it contradicts the ontology design principles in [CONSTITUTION.md](./CONSTITUTION.md).

### Address feedback

Apply fixes **only in canonical source files** (Principle 4):

| Review target | Canonical source to edit |
|---|---|
| SSSOM / EDOAL / FnO / projection queries | `dsl/mappings/` |
| RDF 1.2 / OWL statement artifacts | `dsl/statements/` |
| Ontology terms, axioms, observation bridges | `slices/<group>/<name>/module.ttl` |
| SHACL shapes | `shapes/` |
| Tests, fixtures | `tests/` |

Never patch generated artifacts by hand. After editing canonical sources, regenerate:

```bash
make regenerate          # after ANY canonical-source change
make check-generated     # verify no drift remains
```

### Validate before pushing

```bash
make check
```

All Docker-free local gates must pass: lint, validate, generated-artifact drift
check, native reasoning, native verify, Rust tests, and Python tests. Run
`make maint-classic-cross-check` separately when you need the full Docker/Java
oracle lane.

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
