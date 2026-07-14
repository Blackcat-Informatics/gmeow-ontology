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

* **`gmeow`** ([crates/gmeow-cli](./crates/gmeow-cli)) is the public consumer surface — a native Rust binary. Every command must work from the installed binary alone — backed by the embedded `generated/dist/gmeow.gts` snapshot — with **no source checkout, Docker, generator inputs, or repo-local query trees**. Transpiling a user's own RDF, describing a term, verifying the bundle: consumer operations, so `gmeow`.
* **`gmeow-dev`** ([crates/gmeow-dev-cli](./crates/gmeow-dev-cli)) is repository maintenance. It may read anything in the tree — `dsl/`, `generated/`, `imports/`, `tests/fixtures/` — because it only ever runs inside a checkout. Regenerating artifacts, scoring coverage against the dev corpus, refreshing vendored snapshots: developer operations, so `gmeow-dev`.

When adding a command, ask the razor first. If it needs a repo path that the binary does not bundle, it is `gmeow-dev` — or the data it needs must first be bundled so it can be `gmeow`.

### Environment & Formatting

```bash
make help            # Show the grouped task plan
make install         # Build the Rust CLIs and configure repo-local Git merge drivers
make fmt             # Auto-format Rust sources with cargo fmt
make lint            # Run the issue-ref lint and the pre-commit hygiene suite (Rust fmt/clippy, spelling, YAML, actions, secrets)
make clean           # Remove ephemeral build artifacts and native build stamps
```

`make install` also runs `scripts/bootstrap-git-merge-drivers.sh`, which sets
`merge.ours.driver=true` in the local Git config. That driver backs the
`.gitattributes` rule for `generated/dist/gmeow.gts`: Git keeps the current
side during binary bundle merges/rebases, and the developer regenerates/checks
the bundle from canonical sources afterward.

Every payload-bearing frame in any production-authored GMEOW GTS bundle MUST use
exactly `zstd-rsyncable` at compression level 12. This is a hard distribution
contract: never substitute gzip, plain zstd, identity, or a size-dependent
fallback. The pipeline's `gts_profile` wrapper and committed-bundle frame audit
enforce the transform, while a compile-time assertion pins purrdf's dist level to
12. Header and payload-free transport metadata are not compression frames.

### Validation & Compilation

```bash
make validate        # Validate Turtle syntax, term annotations, and SHACL
make validate-gts    # Validate generated/dist/gmeow.gts
make regenerate # Rebuild ALL committed generated artifacts (the registry; parallel by default)
make check-generated # Drift + orphan + internal-tag-leak check for every registered generator (parallel by default)
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
make docs            # Regenerate external docs: site/book/print/snippets, OKF, YAML-LD
make build           # Build serializations and JSON-LD context into dist/
make project         # Project GMEOW data to external vocabulary profiles
make release         # Regenerate, native-reason, build, report, and emit CrossRef deposit
make release-sign-gts SIGN_KEY=/tmp/gpg/signing-key.asc GTS_OUT=dist/gmeow.gts
```

`make release-sign-gts` signs a freshly regenerated GTS snapshot for release
packaging. The committed `generated/dist/gmeow.gts` remains unsigned unless a
release workflow explicitly writes the signed copy there before packaging.

Documentation projections are never embedded in `gmeow.gts`. The static site,
mdbook sources, print PDF/Typst, prompt snippets, OKF Markdown, JSON-LD, and
YAML-LD are derived artifacts regenerated with `make docs`; Pages CI must use
the source-backed `gmeow-dev export-docs` command. This keeps the logical GTS
carrier separate from large, easily-derived presentation payloads (Principle 4).

### Reasoning & Negative Tests

```bash
make reason          # Native Docker-free EL/DL reasoning authority
make verify          # Native reasoned-graph negative tests
make reason-gate     # One fresh native closure shared by verify + the purrdf-entail oracle
make reason-verify   # Focused native reasoning + reasoned-graph verify
make reason-crosscheck # Focused native subsumption cross-check against purrdf-entail
```

The reasoning cross-check is native and Docker-free: `gmeow-dev reason-crosscheck`
runs the in-process `purrdf::entail` engine (OWL-RL subsumption + OWL-Direct-tableau
consistency, 70/70 W3C-entailment conformance-tested). The aggregate
`make reason-gate` shares one complete native result between verification and that
oracle comparison; the focused commands remain independently runnable. There is no
Java/Docker oracle lane to run separately.

### Testing & Verification

```bash
make check           # Run FULL gate: lint, validate, compilation check, reason, verify, Rust tests
make rust-test       # Run the Rust workspace tests (cargo nextest + doctests)
make clippy          # Run cargo clippy on all Rust targets with warnings as errors
make rust-build      # Compile Rust workspace test binaries without running them
```

The entire toolchain is native Rust; there is no Python test suite. To run a
single crate's tests, use `cargo nextest run -p <crate>`.

### Maintainer Tasks

All maintainer-only work is prefixed with `maint-`. These targets may use
Docker, Java, the network, heavyweight tests, or release/report-only tooling and
are intentionally outside the normal local `make check` path unless a workflow
calls them explicitly.

```bash
make maint-crosscheck               # Native purrdf query-answer cross-check
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

#### The 25 s per-test budget

**Every test on the always-on gate must complete under 25 s of real wall time.**
`make rust-test` / `make rust-gate` (and the CI rust shards) run
`cargo nextest run --profile ci` and then
`cargo run -p gmeow-test-budget -- target/nextest/ci/junit.xml`, which parses the
JUnit report and reports any test over budget (`crates/test-budget`, std-only, no
Python). Override the threshold with `GMEOW_TEST_BUDGET_SECS` only for local
experiments — never to weaken the gate.

**Enforcement is environment-aware, because wall-clock is only trustworthy on a
dedicated runner.** In CI the gate **hard-fails (exit 1)** on any over-budget test
— CI is the authoritative timing environment. On a developer box (many concurrent
worktrees push load far above core count, inflating every heavy test) the gate is
**advisory**: it still prints the offenders as a warning, but returns success, so
contention can't emit false reds that block a local `make check`. This is not a
weakened gate — it measures the right thing in the right place. Enforcement keys on
the platform-guaranteed `CI` variable and is made explicit by the CI step setting
`GMEOW_TEST_BUDGET_ENFORCE=1`; that variable (`1`/`true`/`on` vs `0`/`false`/`off`)
overrides the autodetect in both directions for a local strict check
(`GMEOW_TEST_BUDGET_ENFORCE=1 make rust-gate`) or a CI soft run.

The nextest `slow-timeout` (`.config/nextest.toml`) stays at the 120 s terminate
cliff purely as a runaway/hang backstop; the 25 s **policy** is the JUnit gate.

A test that is *irreducibly* heavier than the budget does NOT get a per-test
timeout override. Instead it is **carved out of the default/ci profiles and runs
on the `maint-heavy` profile** (`make maint-rust-heavy`), so its coverage still
runs — off the per-commit gate, on a maint/scheduled lane. Correctness failures
stay in the normal gate, even when they were first discovered during
`maint-heavy` evaluation. The single source of truth for what is off-gate is the
**`default-filter` expression in `.config/nextest.toml`**; every excluded group
is justified by an inline comment there. Adding a new off-gate exception requires
a comment in that filter AND a one-line entry here.

Default off-gate groups (reevaluated 2026-06-29): `gmeow-validate`
`deep_surfaces_entailed_inconsistency_tier1_misses_heavy_offgate` (30.705 s
locally in `make maint-rust-heavy`; the consumer `gmeow validate --deep` AC1
reasons over user data merged with the whole bundled TBox via the native
chase; the same merge->inconsistency path is covered on-gate by the fast
tiny-TBox `gmeow-logic` unit `reason_all_with_data_*` plus the on-gate
`deep_pass_failure_*`/`deep_false_*` validate tests); the `gmeow-pipeline`
`fold_parity` binary (32.231 s locally; the full real-repo terminal sink vs
committed `generated/dist/gmeow.gts` fold-isomorphism oracle);
`gmeow-rdf-capi::c_smoke` (82.854 s in CI shard 1; it cold-builds the
`libpurrdf` cdylib and compiles/links/runs the real C program, with header and
linkage coverage retained by the dedicated C-API CI lane and `maint-heavy`);
`gmeow-pipeline::end_to_end` (82.351 s in CI shard 1; it runs the real
production spine through `stage-snapshot`, so focused per-stage and carrier
tests keep the failure modes covered on-gate); the `gmeow-pipeline`
`fanout_parity` binary and
`gmeow-pipeline::stages::superset::tests::project_bundle_reconstructs_the_committed_tree_and_gate_is_clean`
(20.4 s / 17.5 s / 16.8 s locally, 24–30+ s in CI; they reconstruct the whole
`generated/` tree from the committed `gmeow.gts` bundle alone — irreducibly
O(bundle size), the same whole-committed-bundle class as `fold_parity`/`end_to_end`;
the `lang:` total-prose-lift surface grew the bundle ~30 % and nudged all three
past budget, and the cost is bundle-size not fixture-dependent; both the superset
reconstruction gate and the fanout reproduction run on every `make check` via
`make check-generated`, which hard-fails on any drift, so no coverage leaves the
gate); and
`gmeow-pipeline::stages::gts_sink::tests::sink_serializes_the_snapshot_carrier_with_blob_inputs`
(25.8 s in CI shard 3; it exercises terminal carrier serialization and import
round-trip work with no remaining CI headroom);
`gmeow-foundation-corpus::acceptance::imported_graph_conforms_to_shapes` (26.065 s
in CI shard 2; whole-ontology SHACL conformance — validates the importer output
unioned with every `slices/*/*/module.ttl` against the entire shape corpus, which
now includes the H8-migrated `sh:sparql` constraint shapes; the importer's
graph-shape parity stays on-gate via the four sibling SPARQL acceptance tests and
whole-ontology SHACL conformance stays on-gate via the ~35 fixture-only domain
`conformance_*` validate tests); the four `gmeow-validate` whole-ontology-union
conformance binaries `conformance_finance`, `conformance_agentic`,
`conformance_ai_claims`, and `conformance_music_analysis` (~8.8 s locally / 24–26 s
in CI; these are exactly the domain conformance binaries whose cases use
`Case::with_ontology()`, unioning the fixture with the WHOLE merged ontology and
validating that entire graph — not just the fixture — against the ENTIRE shape
corpus, so they carry the same H8 `sh:sparql`-constraint cost as
`imported_graph_conforms_to_shapes`; fixture-only conformance cases measure ~0.05 s,
so the cost is the ontology-union validation and is fixture-independent — the four
ride the 25 s cliff together and CI jitter alone decides which trip, so the whole
class is carved out; the ~35 fixture-only `conformance_*` domain tests stay on-gate
against the same shape corpus and the union path stays covered on `maint-heavy`);
the `gmeow-validate` `conformance_math_producers` binary (~20 s per test in CI;
each validates a native math-flagship producer's emitted graph, merged with the
WHOLE base ontology, against the ENTIRE shape corpus via `validate_with_ontology()`
— the identical whole-ontology-union cost, so it joins the same off-gate group;
no coverage leaves the per-commit gate because the five producers are still RUN and
their pinned values asserted on-gate by `crates/pipeline/tests/math_flagship_discharge.rs`,
this binary being the extra cross-crate proof that the producer output validates
SHACL-clean);
the `gmeow-validate` `conformance_affect_producer_union` binary (the 8
whole-ontology-union affect twins — four `Case::with_ontology()` exclusivity/label
twins + four `.shape_union()` projected-cardinality twins — split out of the mixed
`conformance_affect_producer` binary so its ~10 cheap fixture-only + pure-Rust
producer tests stay on-gate; the same whole-ontology-union H8 `sh:sparql` cost as
the domain conformance binaries above, and a deterministic cost-partition bench
(`make maint-bench-instructions`) measured a single twin's whole-graph scan at
~34.8 GB / 54.2M allocations of churn — ~63× the one-time corpus setup and ~25,000×
the fixture-only path — so the per-twin scan is irreducible: no setup cache
amortizes it and folding raises the per-test maximum to setup + 8× the scan, so it
joins the same off-gate group);
`gmeow-pipeline::stages::carrier::quality_assessment_tests::quality_assessment_graph_rides_the_self_description_carrier_heavy_offgate`
(86.2 s; it builds the full self-description carrier, scoring every one of the ~81
slices to attach the `graph/quality-assessment` named graph that folds into
`gmeow.gts` — irreducibly O(slice count), the same whole-repo class as
`end_to_end`/`fold_parity`; the attach↔fanout-path bijection and N-Triples fold form
stay on-gate via the fast sibling units
`quality_assessment_fanout_path_is_registered_and_folds_as_ntriples` and
`superset::tests::quality_assessment_nt_folds_as_ntriples_via_its_own_fanout_graph`,
the folded `generated/quality/gmeow.quality-assessment.nt` is drift-gated on every
`make check` via `make check-generated`, and the exhaustive proof stays on-gate on
`maint-heavy`); and
`gmeow-logic::whole_bundle_coherence_gate_catches_injected_clash` (~95 s locally;
it imports the WHOLE committed `gmeow.gts` bundle and drives the native
chase over it twice, proving the shipped ontology is coherent and that an
injected disjoint-class clash is caught). This exclusion is **budget-exempt,
not gate-exempt**: the test still runs on every `make check` via the dedicated
`make coherence-gate-teeth` target, which selects it with
`--ignore-default-filter` plus an explicit `-E` filter and does not feed the
JUnit budget gate. The `lang:` projection surface (OntoLex-Lemon / CoNLL-U /
SemAF forward projections) plus the native-chase logic growth nudged three
whole-bundle CLI tests past budget —
`gmeow-dev-cli::cli_parity::build_writes_serializations`,
`gmeow-cli::cli::export_respects_language_selector`, and
`gmeow-cli::cli::project_schema_org_view_filter` (6–8 s standalone but
25.5–27.5 s under full-parallelism CI contention; each drives the CLI to
serialize / export / project-a-view over the WHOLE bundle, irreducibly
O(bundle size), the same whole-committed-bundle class as `fanout_parity` /
`end_to_end`; the written serializations stay drift-gated on every `make check`
via `make check-generated`, and all three stay on-gate on `maint-heavy`). The
same lang: graft + correspondence-laws surface growth pushed one whole-site docs
test past budget — `gmeow-docs::extract_roundtrip::rendered_tree_is_disk_faithful`
(32.6 s; it drives the real `render::write_site` over the WHOLE primed
documentation site, writing every page to disk and reading each back to prove
on-disk bytes equal the in-memory Site — irreducibly O(site size), and the render
itself is primed so the cost is the per-file disk round-trip over the grown tree,
not fixture cost; the docs write contract stays drift-gated on every `make check`
via `make check-generated` and the test stays on-gate on `maint-heavy`).
Bringing the grounding-language vocabulary (`math:`/`logic:`/`lang:`, ~1.9k terms)
into the documented-term set grew the rendered site ~40 % and nudged four more
whole-site render tests past budget —
`gmeow-docs::render_golden::render_site_is_byte_stable` (33.1 s),
`gmeow-docs::mdbook_render::book_bodies_are_rewrite_of_single_authority` (34.5 s),
`gmeow-docs::extract_roundtrip::english_carrier_tree_matches_render_site` (32.6 s),
and `gmeow-pipeline::stages::carrier::ustar_tests::build_docs_archive_packs_the_rendered_site`
(32.9 s); each renders / re-renders / re-packs / round-trips the WHOLE site, so all
four are irreducibly O(site size) on page-count alone, not fixture cost; byte-stable
rendering and the docs archive stay drift-gated on every `make check` via
`make check-generated`, and all four stay on-gate on `maint-heavy`.
The `lang:` GMN dialect graft grew the bundle again and nudged the two
whole-bundle inverse-ingest acceptance tests past budget —
`gmeow-pipeline::projections::tests::up_project_recovers_message_superclass_via_prp_dom`
and `gmeow-pipeline::projections::tests::up_project_never_fabricates_subkind_or_sibling`
(6.2 s each standalone but 26.9–27.3 s under full-parallelism contention; each
folds the WHOLE committed `gmeow.gts` bundle and drives the real `up_project`
over its mappings/cells archives, irreducibly O(bundle size), the same
whole-committed-bundle class as the CLI trio above; the up-projection surface
keeps its fixture-scale on-gate tests in the same module, and both bundle-scale
tests stay on-gate on `maint-heavy`).
The `lang:` total-prose-lift + GMN-graft surface plus the conjecture-library
`logic:`/`math:` term growth grew the bundle again and nudged three more
whole-bundle CLI tests past budget —
`gmeow-cli::cli::project_unknown_view_fails`,
`gmeow-cli::cli::describe_env_language_rejected_if_unknown`, and
`gmeow-dev-cli::cli_parity::project_view_over_the_snapshot` (3–5 s standalone but
27.5–32.2 s under full-parallelism CI contention; each drives the CLI to load /
describe / project-a-view over the WHOLE committed bundle before its fast
reject/parity assertion, irreducibly O(bundle size), the identical
whole-committed-bundle class as the `build_writes_serializations` / export /
`project_schema_org_view` siblings above; the CLI project/describe surface keeps
its fixture-scale reject/parity coverage on-gate in the same binaries, the
serializations stay drift-gated via `make check-generated`, and all three stay
on-gate on `maint-heavy`).
`gmeow-pipeline::stages::carrier::term_entailments_tests::term_entailments_are_non_vacuous_on_the_real_repo`
(~100 s locally) proves the B3 entailment-join is non-vacuous against
the REAL ontology by running the real `source_load` → `statements` /
`compile_logic` → `mappings` → `reason` stage chain — a full native-chase
reasoning pass over the whole EDB, irreducibly O(bundle size), the same
whole-repo class as `end_to_end`/`quality_assessment_graph_rides_the_self_description_carrier_heavy_offgate`;
the join logic itself stays on-gate via the fast fixture-only siblings
`term_entailments_from_explanations_populates_matching_term_only` and
`term_entailments_from_upstream_joins_and_hard_fails_on_missing_artifact` in the
same module, and the real-repo non-vacuity proof stays on-gate on `maint-heavy`.
Its two symmetric per-term siblings —
`stages::docs_render::tests::diagnostics_digest_total_is_non_vacuous_on_the_real_repo`
(the diagnostics→term join over the real `source_load` → `validate` / `compile_logic`
chain) and `::term_loss_digest_is_non_vacuous_on_the_real_repo` (the per-term
projection-loss join over the real `compile_logic` → `mappings` chain) — are the same
whole-repo cost class and are likewise carved to `maint-heavy`, with their join logic
proven on-gate by fast synthetic siblings in the same module.
A new GMN-1 Coverage quality axis grew
`slices/core/slice-quality-rubric/module.ttl` — already the largest, most prose-dense
module in the repo, being the self-describing quality rubric itself — to a 13th axis,
nudging its `structural.ttl` datatest case past budget on this shared dev box:
`gmeow-slicetest::run_structural_file::core/slice-quality-rubric/tests/structural.ttl`
(19–42 s depending on contention; a same-scope dataset cache landed in
`run_structural_cell` first, halving the standalone cost, but the module-size x
cell-count product is still irreducibly over budget under contention). No coverage
leaves the gate: the assertions still run on `make maint-rust-heavy`, every other
slice's `structural.ttl` stays on-gate (only this one file's datatest case is
excluded), and the rubric's axis-shape invariants stay drift-gated via
`make check-generated`.
The self-sufficiency parity harness added four more whole-bundle CLI tests —
`gmeow-cli::self_sufficiency::transpile_wheel_mode_equals_repo_mode`,
`::transpile_blinded_lifts_and_fans_out_without_x_gmeow_leak`,
`::project_wheel_mode_equals_repo_mode`, and
`::describe_wheel_mode_equals_repo_mode` (31-53 s locally, `describe` 7.6 s
standalone but 28.0 s under full-gate contention; each drives
`gmeow transpile`/`gmeow project`/`gmeow describe` over the WHOLE embedded
bundle — the transpile parity test runs it twice, blinded-cwd and repo-cwd
legs — the identical whole-committed-bundle class as the
`build_writes_serializations` / `export_respects_language_selector` /
`project_schema_org_view_filter` trio above; the same wheel-mode==repo-mode
parity law stays on-gate at fixture scale via
`self_sufficiency::validate_wheel_mode_equals_repo_mode`, and all four stay on-gate on
`maint-heavy`).
The native release-attestation round-trip added a whole-bundle pair —
`gmeow-pipeline::release_verify_roundtrip::release_bundle_with_coherence_evidence_round_trips_natively`
and `::release_bundle_rejects_an_untrusted_out_of_band_key` (~75 s each locally;
`fold_release_bundle`/`verify_release_bundle` each replay the WHOLE committed
~48 MB `generated/dist/gmeow.gts`, irreducibly O(bundle size), the identical
whole-committed-bundle class as `fold_parity`/`fanout_parity`/`end_to_end`
above; the real crypto-through-the-built-binary requirement — accept a valid
signature, reject an invalid one — stays on-gate via
`gmeow-cli::bundle_smoke`, which builds a small in-process signed `.gts`
rather than replaying the shipped bundle, and `release.rs`'s own unit tests
cover the fold/verify pair's logic against a tiny synthetic snapshot; the full
committed-bundle round-trip stays on-gate on `maint-heavy`).
The AI-agent docs surface (native `validate_local` + `doc_card` tiers + the
`docs_search`/`counter_examples`/`entailments`/`competency_questions` tools, backed
by a `graph/documentation` teaching-content projection) grew the bundle and the MCP
tool surface enough to push two whole-bundle sweeps past budget:
`gmeow-pipeline::stages::governance_floors::tests::generated_axis_floors_are_byte_reconstructible_from_the_bundle`
(27.1 s; reconstructs every axis floor from the committed bundle alone, irreducibly
O(bundle size), the same whole-committed-bundle class as the superset/reconstruction
siblings above) and
`gmeow-pipeline::mcp::tests::json_rpc_protocol_conformance_round_trip`
(25.3 s; dispatches EVERY advertised tool over the whole bundle — `validate_local`
runs the full SHACL surface and the four content tools each query the whole
`graph/documentation` projection — so it grew on tool-surface/bundle size, not
fixture cost). No coverage leaves the gate: the axis-floor bytes are drift-gated on
every `make check` via `make check-generated`; each new MCP tool keeps its own
focused on-gate test (`validate_local` parity+correspondence, the four content-tool
surface tests, and the `tools/list` mode-gating golden), so every tool is still
exercised on-gate; and both whole-bundle sweeps run on `maint-heavy`.
The two exhaustive compile-logic -> mappings integration sweeps are also off-gate:
`gmeow-pipeline::stages::mappings::tests::projection_report_unions_logic_and_correspondence_rows`
(26.768 s in the failing CI shard) and
`gmeow-pipeline::product_routing::compiler_products_are_first_class_dag_artifacts`
(26.026 s in the same shard). Each runs the real pipeline over the WHOLE authored
repository and is irreducibly O(ontology/projection size). No coverage class leaves
the gate: `compile_logic_stage_emits_every_product` proves the compiler artifacts and
in-memory channel, `loss_ledger_and_diagnostics_reach_the_shipped_bundle` proves the
routed bundle graphs, focused projection-report units cover report construction, and
`make check-generated` byte-gates the final committed union; the exhaustive pair runs
on `maint-heavy`.
Resolving grounding-namespace terms (`lang:`/`math:`/`logic:`) across the shipped
`describe` + MCP surfaces added four whole-bundle `describe` tests that ride the same
25 s cliff on bundle size, not fixture cost —
`gmeow-cli::cli::describe_resolves_grounding_curies` (40.6 s) and
`::describe_resolves_grounding_full_iris` (27.4 s) spawn the shipped binary once per
grounding namespace and each spawn pays the full bundle cold-start (the identical
whole-committed-bundle O(bundle size) class as the single-spawn sibling
`describe_env_language_rejected_if_unknown` above, which is 3–5 s standalone but
27–32 s under CI contention — so splitting into single-spawn tests would not clear the
cliff either); `gmeow-cli::self_sufficiency::describe_grounding_terms_wheel_mode_equals_repo_mode`
(59.4 s) runs `gmeow describe` in both the wheel-mode and repo-mode legs (the
grounding-term twin of `describe_wheel_mode_equals_repo_mode` above); and
`gmeow-docs::describe::tests::every_grounding_namespace_has_describable_terms_that_render`
(47.5 s) re-folds the whole bundle once per grounding namespace to prove a term renders
in each. No coverage class leaves the gate: cross-namespace resolution (CURIE / full-IRI /
bare-local plus the ambiguity hard-fail) stays on-gate via the fixture-scale resolver unit
tests in `crates/docs` (`describe.rs`) and `crates/pipeline` (the `export.rs` MCP ambiguity
test), the registry-vs-`PREFIXES_BY_LEN` coherence stays on-gate via the `lpg_prefixes`
coherence gate, and all four whole-bundle describe tests stay on-gate on `maint-heavy`.
The whole-bundle export structural sweep
`gmeow-pipeline::stages::export::tests::export_produces_structurally_valid_artifacts`
is likewise off-gate (23.014 s in snapshot review, 25.579 s under the full-gate
shard). It folds the whole committed bundle and renders all 14 export artifacts, so
its cost is irreducibly O(bundle and vocabulary size) and now rides the 25 s cliff
under normal contention. No export coverage class leaves the per-commit gate: the
focused renderer, schema, and envelope tests remain on-gate, every committed export
is byte-drift-gated by `make check-generated`, and the exhaustive structural sweep
runs on `maint-heavy`.
The typed-IR logic uplift grew the shipped vocabulary, documentation model, and bundle
enough to push eight more whole-site / whole-bundle tests over the budget under the
full CI shard: `gmeow-docs::mdbook_render::book_term_chapter_with_dropped_link_golden`
(30.7 s), `::book_no_relative_link_to_dropped_page` (30.4 s), `::book_toml_golden`
(26.6 s), `::book_zero_term_slice_renders_valid_chapter` (26.4 s),
`gmeow-cli::cli::describe_known_term_renders_prose` (28.7 s),
`::describe_unknown_language_fails_with_available_list` (25.9 s),
`::describe_toon_format_emits_toon` (25.4 s), and
`gmeow-docs::describe_bundle_language::describe_resolves_carrier_tags_against_shipped_bundle`
(25.8 s). The mdBook cases render the WHOLE primed documentation model, while the
describe cases load/fold the WHOLE shipped bundle before making their focused
assertions, so the cost is irreducibly O(site or bundle size), not fixture size.
Focused renderer, link, language, TOON, and fixture-scale describe tests remain
on-gate, committed documentation stays byte-drift-gated by `make check-generated`,
and all eight exhaustive cases run on `maint-heavy`.
Former off-gate groups such as
ontology entailments, SPARQL path parity, RDF/RDFC parity outliers,
correspondence parity, mapping parity, carrier/docs archive tests, scoreboards
acceptance, JSON-LD round-trips, slice/slicetest parity, and
docs live-render guards are in the default/ci profile.

Nearly the whole `gmeow-docs` test cluster is **on-gate**: each test loads a shared
`DocsModel` *and* the rendered site for every available language from the
content-addressed `gmeow_docs::fixture` cache instead of rebuilding or re-rendering.
The cache is primed once before the run — the `prime-docs-fixture` example, run by
the Makefile test lanes and the CI test job ahead of `nextest` — so no test pays the
~12 s build, the cold concurrent-rebuild contention, or a per-language site render. A
plain `cargo test` still works (a miss falls through to a build/render). The
live-full-render guards — `render_site_is_byte_stable` and
`english_carrier_tree_matches_render_site` — measured around 2.1 s locally on
2026-06-29 and are now on-gate too. The language-comparison round-trips
(`french_tree`, `chinese_tree`, and the translated no-dangling-link check) only
need a render's *output*, so they read the per-language cache and are on-gate;
every single-page / single-term docs test stays on-gate.

The bias is **fix, don't off-gate**: prefer making a test fast (shard it like the
corpus parity sweep in `crates/rdf/tests/sparql_eval_parity.rs`, or share an
expensive fixture once per run like the docs-model cluster) over moving it to
maint-heavy. Off-gate is for the genuinely irreducible.

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
  * `generated/statements/gmeow.rdf12.ttl` — RDF 1.2 / RDF* lead artifact, written natively by the `gmeow-rdf` Rust codec (`gmeow_rdf.project_statements_rdf12`); no Java, no Docker, no SPARQL engine. rdflib cannot parse RDF 1.2 triple terms, so the native codec also supplies the OWL normal form for the round-trip check.
  * `generated/statements/gmeow-statements.owl.ttl` — OWL 2 axiom-annotation downcast consumed by OWL 2 DL reasoners.
* **Important behavior**: the DSL is plain Turtle that structurally mirrors RDF 1.2 reifying statements. The compiler emits the OWL form, projects it to RDF 1.2 natively with `gmeow-rdf`, then normalizes the RDF 1.2 form back to OWL and requires graph isomorphism before writing. Apache Jena re-reads the committed artifact only in the non-required `maint-statements-docker-check` oracle lane.
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

**The one rule:** if a path is under `generated/`, a registered generator owns it and you never edit it; if it is under `dist/`, it is ephemeral and never committed; anything else is authored by a human.

**Exception:** `ontology-docs/` at the repository root is an ephemeral generated artifact owned by the `docs` registered generator. It lives outside `generated/` so GitHub Pages can publish it directly, but it is ignored and regenerated on demand with `make docs` or `gmeow-dev export-docs`. It is never embedded in `generated/dist/gmeow.gts`.

```text
slices/<group>/<name>/   # THE unit of the ontology: a slice. The <group> segment
                         #   (core/, extensions/) is human organization only —
                         #   manifest.ttl is the SOLE source of identity (IRI) and
                         #   tier. Anatomy (discovered, never configured):
                         #   manifest.ttl, module.ttl, shapes.ttl, mappings/,
                         #   queries/, examples/, tests/, docs.md
slices/vocabulary.ttl    # The slice-manifest authoring vocabulary (spec layer)
ontology/gmeow.ttl # Root ontology = the CORE profile (generated imports)
dsl/mappings/            # Mapping DSL: vocabulary, foundational bridge, per-target
                         #   projections, shared equivalences, transforms.fno.ttl
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
    | jq -r '.[] | select(.path == "crates/validate/src/repo_static.rs" and .line == 100) | .body'
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
check, native reasoning, native verify (including the on-gate in-process
`purrdf::entail` cross-check oracle), and the Rust tests.

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
