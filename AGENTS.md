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

The repository uses Python (`uv`) and Docker (for Java tools like ROBOT, WIDOCO, and Jena). Always use the following `make` targets to run operations:

### The CLI razor — `gmeow` vs `gmeow-dev` (#517)

There are two CLIs, and a single razor decides where a command belongs:

> **`gmeow` does not need a repo; `gmeow-dev` does.**

* **`gmeow`** ([src/gmeow_tools/cli.py](./src/gmeow_tools/cli.py)) is the public, PyPI-facing surface. Every command must work from the installed wheel alone — backed by the bundled `generated/dist/gmeow.gts` snapshot — with **no source checkout, Docker, generator inputs, or repo-local query trees**. Transpiling a user's own RDF, describing a term, verifying the bundle: consumer operations, so `gmeow`.
* **`gmeow-dev`** ([src/gmeow_tools/cli_dev.py](./src/gmeow_tools/cli_dev.py)) is repository maintenance. It may read anything in the tree — `dsl/`, `generated/`, `imports/`, `tests/fixtures/` — because it only ever runs inside a checkout. Regenerating artifacts, scoring coverage against the dev corpus, refreshing vendored snapshots: developer operations, so `gmeow-dev`.

When adding a command, ask the razor first. If it needs a repo path that the wheel does not bundle, it is `gmeow-dev` — or the data it needs must first be bundled so it can be `gmeow`.

### Environment & Formatting

```bash
make install         # Sync uv and configure repo-local Git merge drivers
make fmt             # Auto-format Python files with ruff
make lint            # Run ruff check, ruff format --check, and mypy
```

`make install` also runs `scripts/bootstrap-git-merge-drivers.sh`, which sets
`merge.ours.driver=true` in the local Git config. That driver backs the
`.gitattributes` rule for `generated/dist/gmeow.gts`: Git keeps the current
side during binary bundle merges/rebases, and the developer regenerates/checks
the bundle from canonical sources afterward.

### Validation & Compilation

```bash
make validate        # Validate Turtle syntax, term annotations, and SHACL
make regenerate      # Rebuild ALL committed generated artifacts (the #279 registry; parallel by default)
make check-generated # Drift + orphan + internal-tag-leak check for every registered generator (parallel by default)
make constitution-check # Every principle has live enforcement (governance/constitution.ttl, #280)
make wikidata        # Validate Wikidata QID/PID syntax in the mappings (offline)
make native-py       # Build and install all Rust-backed Python extensions (diagnostics-py, logic-py, shacl-py, validate-py, gts-producer-py)
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

### Reasoning & Negative Tests

```bash
make reason          # Check ELK consistency (Docker ROBOT; writes dist/gmeow-reasoned-elk.ttl)
make reason-hermit   # Full complete consistency check with HermiT (Docker)
make explain         # Explain any unsatisfiable classes (HermiT, Docker)
make verify          # Reuse dist/gmeow-reasoned-elk.ttl and run SPARQL QC (Docker)
```

### Testing & Verification

```bash
make test            # Run the pytest test suite (Python/SPARQL competency tests)
make check           # Run FULL gate: lint, validate, compilation check, reason, verify, tests
```

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

Mapping compilation runs inside `gmeow regenerate` (the `mappings` generator), implemented by [src/gmeow_tools/mapping_dsl.py](./src/gmeow_tools/mapping_dsl.py) and [src/gmeow_tools/mapping_compile.py](./src/gmeow_tools/mapping_compile.py).

* **Canonical input**: all Turtle files under [dsl/mappings/](./dsl/mappings/), plus the DSL vocabulary in [dsl/mappings/vocabulary.ttl](./dsl/mappings/vocabulary.ttl).
* **Generated outputs**:
  * `mappings/*.sssom.tsv` — SSSOM term-equivalence rows.
  * `projections/*.edoal.ttl` — EDOAL alignment cells.
  * `projections/functions.fno.ttl` — generated FnO function catalog.
  * `queries/projections/*.rq` — executable SPARQL CONSTRUCT projection queries.
* **Hand-authored companion file**: `dsl/mappings/transforms.fno.ttl` is read by the compiler/lints but is authored, never generated.
* **Important behavior**: the compiler first renders artifacts into a temporary tree, runs projection cross-layer invariants, and only then writes generated files. If an invariant fails, nothing is written.
* **Drift check**: `make check-generated` renders into a staging tree, compares against the committed `generated/` artifacts, detects orphans, and enforces the internal-tag leak gate.

The mapping DSL has two main authoring units:

* `gmeow:TermEquivalence` for pure cross-ontology links that compile to SSSOM rows.
* `gmeow:ProjectionMapping` for directional, possibly lossy projections that compile to SPARQL branches and, when applicable, EDOAL/FnO/SSSOM artifacts.

Do not patch a generated SSSOM, EDOAL, FnO, or projection query file directly to satisfy review feedback. Patch the DSL source, re-run the compiler, and include the regenerated artifacts.

### Statement Compiler

Statement compilation runs inside `gmeow regenerate` (the `statements` generator), implemented by [src/gmeow_tools/statement_dsl.py](./src/gmeow_tools/statement_dsl.py) and [src/gmeow_tools/statement_compile.py](./src/gmeow_tools/statement_compile.py).

* **Canonical input**: all Turtle files under [dsl/statements/](./dsl/statements/), plus the DSL vocabulary in [statement-dsl/vocabulary.ttl](./statement-dsl/vocabulary.ttl).
* **Generated outputs**:
  * `generated/statements/gmeow.rdf12.ttl` — RDF 1.2 / RDF* lead artifact, emitted through Apache Jena because rdflib cannot yet parse/write native RDF 1.2 triple-term Turtle.
  * `generated/statements/gmeow-statements.owl.ttl` — OWL 2 axiom-annotation downcast consumed by OWL 2 DL reasoners.
* **Important behavior**: the DSL is plain Turtle that structurally mirrors RDF 1.2 reifying statements. The compiler emits the OWL form, projects it to RDF 1.2 with Jena, then normalizes the RDF 1.2 form back to OWL and requires graph isomorphism before writing.
* **Drift check**: `make statements-check` performs the same compile in check mode and fails if committed statement artifacts are stale.

Do not edit `generated/statements/gmeow.rdf12.ttl` or `generated/statements/gmeow-statements.owl.ttl` directly. If metadata is wrong, fix the `gmeow:StatementMetadata` cells in `dsl/statements/`.

### Generated Artifact Rule

Generated files contain a `GENERATED by ... DO NOT EDIT` banner where practical. Treat that as binding:

* Source changes belong in `slices/<group>/<name>/module.ttl`, `dsl/mappings/`, `dsl/statements/`, shapes, queries, tests, or toolchain source.
* Generated artifact changes must be reproducible by the relevant `make compile-*` target.
* If `make check-generated` reports drift, run `make regenerate` rather than hand-editing the output.
* If a generated artifact is nondeterministic, fix the compiler determinism bug. Do not normalize the artifact by hand.

### Vocabulary Index (llms.txt)

This project automatically generates a single-file, flat index of all classes, properties, and individuals (with CURIEs, parent classes, and definitions) at `dist/llms.txt` when running `make export`. It is **not checked in** — run `make export` to produce it on demand.

If you are an agent trying to look up terms, resolve definitions, or discover vocabulary details, run `make export` and ingest `dist/llms.txt` to get a clean, context-efficient overview of the entire ontology.

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

## 5. PR Lifecycle: Rebase, Review, Push

When a PR is open and feedback arrives, follow this cycle strictly.

### Rebase onto latest main

```bash
git fetch origin main
git rebase origin/main
```

If conflicts involve generated files, **always resolve by accepting the main branch version** and regenerating afterward:

```bash
git checkout --theirs <generated-file>
git add <generated-file>
git rebase --continue
make regenerate
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

All gates must pass: lint, validate, compilation drift check, ELK reasoning, HermiT reasoning, verify, tests.

### Push

Amend the commit to keep the branch history clean:

```bash
git add -A
git commit --amend --no-edit
git push --force-with-lease origin <branch-name>
```

> [!IMPORTANT]
> Always use `--force-with-lease`, never bare `--force`. This prevents overwriting commits you have not yet fetched.
