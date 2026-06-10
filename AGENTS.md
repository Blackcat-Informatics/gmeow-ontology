<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# AI Developer Agent Guide (AGENTS.md)

Welcome, AI Agent! This file is your behavioral contract and instruction manual for contributing to the GMEOW repository. Please read and adhere strictly to the rules below.

---

## 1. Project Overview & Architecture

GMEOW is a **reasoning-centric, OWL 2 DL, upper-ontology-grounded super-vocabulary** that unifies document metadata, entity descriptions, legal agreements, contacts, and person-centric data.

Every design decision, code modification, and schema change is governed by the twelve principles of the [CONSTITUTION.md](./CONSTITUTION.md). Cite these principles by number (e.g., `"Principle 4"`) in your commit messages, pull requests, and discussions.

### Critical Ontological Rules

* **One Canonical Source (Principle 4)**: Do not hand-edit generated files. Any change must be made in the canonical source files.
  * **Mappings**: Authored in [mapping-dsl/](./mapping-dsl/) -> compiled to `mappings/`, `projections/`, etc.
  * **Statements**: Authored in [statement-dsl/](./statement-dsl/) -> compiled to `statements/`.
* **RDF 1.2 / RDF\*-first (Principles 2 & 3)**: Statement-level metadata (provenance, confidence, temporal scope) is authored as native RDF 1.2 / RDF\* in the statement DSL. The logical core stays OWL 2 DL.
* **Co-equal & Non-privileged (Principles 9 & 10)**: There is no `primaryName`, `preferredGender`, or single-winner preference. A contested fact is represented as coexisting standpoint-indexed claims. A superseded label/deadname is suppressed using `gmeow:displayable false` rather than deleted.

---

## 2. Core Toolchain & Commands

The repository uses Python (`uv`) and Docker (for Java tools like ROBOT, WIDOCO, and Jena). Always use the following `make` targets to run operations:

### Environment & Formatting

```bash
make install         # Sync the uv environment (runtime + dev dependencies)
make fmt             # Auto-format Python files with ruff
make lint            # Run ruff check, ruff format --check, and mypy
```

### Validation & Compilation

```bash
make validate        # Validate Turtle syntax, term annotations, and SHACL
make compile-mappings # Compile mapping-dsl/ to mappings/ and projections/ (run after changing DSL)
make compile-statements # Compile statement-dsl/ to statements/ (run after changing statement DSL)
make compile-check   # Validate that committed projection artifacts match mapping-dsl/
make statements-check # Validate that committed statement artifacts match statement-dsl/
make wikidata        # Validate Wikidata QID/PID syntax in the mappings (offline)
```

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

`make regenerate` runs the generators in dependency order: `compile-mappings` → `compile-statements` → `metadata` → `apache` → `export`. It refreshes:

* `mappings/`, `projections/`, `queries/projections/` — from `compile-mappings`
* `statements/` — from `compile-statements`
* `metadata/void.ttl`, `metadata/dcat.ttl` — from `metadata`
* `apache/gmeow.conf` — from `apache`
* `dist/lpg/` — from `lpg`

`make commit` stages only the generated artifacts above. If you also have source changes (e.g. in `mapping-dsl/`), stage them separately with `git add` before running `make commit`, or amend the commit afterward.

> [!TIP]
> If you suspect generated files are stale but do not want to commit yet, run `make regenerate` followed by `make check` to verify the full gate still passes.

### Reasoning & Negative Tests

```bash
make reason          # Check ELK consistency (Docker ROBOT)
make reason-hermit   # Full complete consistency check with HermiT (Docker)
make explain         # Explain any unsatisfiable classes (HermiT, Docker)
make verify          # Run reasoned-graph negative tests (SPARQL QC over queries/verify/)
```

### Testing & Verification

```bash
make test            # Run the pytest test suite (Python/SPARQL competency tests)
make check           # Run FULL gate: lint, validate, compilation check, reason, verify, tests
```

> [!IMPORTANT]
> Always run `make check` locally and ensure it passes completely before proposing changes, committing, or submitting a PR.

---

## 3. How the Compilers Work

The Makefile is only a task runner. The actual compiler and validation logic lives in [src/gmeow_tools/](./src/gmeow_tools/), and the `gmeow` CLI is a thin orchestration layer over focused Python modules.

### Mapping Compiler

`make compile-mappings` runs `uv run gmeow compile-mappings`, implemented by [src/gmeow_tools/mapping_dsl.py](./src/gmeow_tools/mapping_dsl.py) and [src/gmeow_tools/mapping_compile.py](./src/gmeow_tools/mapping_compile.py).

* **Canonical input**: all Turtle files under [mapping-dsl/](./mapping-dsl/), plus the DSL vocabulary in [mapping-dsl/vocabulary.ttl](./mapping-dsl/vocabulary.ttl).
* **Generated outputs**:
  * `mappings/*.sssom.tsv` — SSSOM term-equivalence rows.
  * `projections/*.edoal.ttl` — EDOAL alignment cells.
  * `projections/functions.fno.ttl` — generated FnO function catalog.
  * `queries/projections/*.rq` — executable SPARQL CONSTRUCT projection queries.
* **Hand-authored companion file**: `projections/transforms.fno.ttl` is read by the compiler/lints but is not generated by `compile-mappings`.
* **Important behavior**: the compiler first renders artifacts into a temporary tree, runs projection cross-layer invariants, and only then writes generated files. If an invariant fails, nothing is written.
* **Drift check**: `make compile-check` runs the same render into a temp tree with `--check`, compares generated RDF by graph isomorphism and TSV/SPARQL by bytes, and fails if committed artifacts are stale.

The mapping DSL has two main authoring units:

* `gmeow:TermEquivalence` for pure cross-ontology links that compile to SSSOM rows.
* `gmeow:ProjectionMapping` for directional, possibly lossy projections that compile to SPARQL branches and, when applicable, EDOAL/FnO/SSSOM artifacts.

Do not patch a generated SSSOM, EDOAL, FnO, or projection query file directly to satisfy review feedback. Patch the DSL source, re-run the compiler, and include the regenerated artifacts.

### Statement Compiler

`make compile-statements` runs `uv run gmeow compile-statements`, implemented by [src/gmeow_tools/statement_dsl.py](./src/gmeow_tools/statement_dsl.py) and [src/gmeow_tools/statement_compile.py](./src/gmeow_tools/statement_compile.py).

* **Canonical input**: all Turtle files under [statement-dsl/](./statement-dsl/), plus the DSL vocabulary in [statement-dsl/vocabulary.ttl](./statement-dsl/vocabulary.ttl).
* **Generated outputs**:
  * `statements/gmeow.rdf12.ttl` — RDF 1.2 / RDF* lead artifact, emitted through Apache Jena because rdflib cannot yet parse/write native RDF 1.2 triple-term Turtle.
  * `statements/gmeow-statements.owl.ttl` — OWL 2 axiom-annotation downcast consumed by OWL 2 DL reasoners.
* **Important behavior**: the DSL is plain Turtle that structurally mirrors RDF 1.2 reifying statements. The compiler emits the OWL form, projects it to RDF 1.2 with Jena, then normalizes the RDF 1.2 form back to OWL and requires graph isomorphism before writing.
* **Drift check**: `make statements-check` performs the same compile in check mode and fails if committed statement artifacts are stale.

Do not edit `statements/gmeow.rdf12.ttl` or `statements/gmeow-statements.owl.ttl` directly. If metadata is wrong, fix the `gmeow:StatementMetadata` cells in `statement-dsl/`.

### Generated Artifact Rule

Generated files contain a `GENERATED by ... DO NOT EDIT` banner where practical. Treat that as binding:

* Source changes belong in `ontology/modules/`, `mapping-dsl/`, `statement-dsl/`, shapes, queries, tests, or toolchain source.
* Generated artifact changes must be reproducible by the relevant `make compile-*` target.
* If `make compile-check` or `make statements-check` reports drift, run the matching compiler rather than hand-editing the output.
* If a generated artifact is nondeterministic, fix the compiler determinism bug. Do not normalize the artifact by hand.

### Vocabulary Index (llms.txt)

This project automatically generates a single-file, flat index of all classes, properties, and individuals (with CURIEs, parent classes, and definitions) at `dist/llms.txt` when running `make export`. It is **not checked in** — run `make export` to produce it on demand.

If you are an agent trying to look up terms, resolve definitions, or discover vocabulary details, run `make export` and ingest `dist/llms.txt` to get a clean, context-efficient overview of the entire ontology.

---

## 4. Directory Layout

* `ontology/modules/` — Canonical Turtle ontology modules. Add new terms here.
* `mapping-dsl/` — DSL files for alignments. Edit mappings here.
* `statement-dsl/` — DSL files for statement-level metadata. Edit statement provenance here.
* `shapes/` — SHACL validation shapes.
* `queries/verify/` — Negative tests/QC SPARQL queries.
* `src/gmeow_tools/` — The toolchain CLI Python source code.
* `tests/` — Integration and competency tests.

Generated or partially generated areas:

* `mappings/` — Generated SSSOM files from `mapping-dsl/`.
* `projections/` — Generated EDOAL/FnO files, except hand-authored `projections/transforms.fno.ttl`.
* `queries/projections/` — Generated projection CONSTRUCT queries.
* `statements/` — Generated RDF 1.2 lead artifact and OWL 2 downcast from `statement-dsl/`.
* `dist/` and `docs/_generated/` — Build, documentation, metadata, export, and release artifacts.

---

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
| SSSOM / EDOAL / FnO / projection queries | `mapping-dsl/` |
| RDF 1.2 / OWL statement artifacts | `statement-dsl/` |
| Ontology terms, axioms, observation bridges | `ontology/modules/` |
| SHACL shapes | `shapes/` |
| Tests, fixtures | `tests/` |

Never patch generated artifacts by hand. After editing canonical sources, regenerate:

```bash
make compile-mappings    # if mapping-dsl/ changed
make compile-statements  # if statement-dsl/ changed
make regenerate          # full rebuild of all generated artifacts
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
