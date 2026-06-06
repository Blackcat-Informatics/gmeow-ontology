<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: Apache-2.0 -->

# AI Developer Agent Guide (AGENTS.md)

Welcome, AI Agent! This file is your behavioral contract and instruction manual for contributing to the GMEOW repository. Please read and adhere strictly to the rules below.

---

## 1. Project Overview & Architecture

GMEOW is a **reasoning-centric, OWL 2 DL, upper-ontology-grounded super-vocabulary** that unifies document metadata, entity descriptions, legal agreements, contacts, and person-centric data.

Every design decision, code modification, and schema change is governed by the twelve principles of the [CONSTITUTION.md](file:///home/paudley/Active/gmeow-ontology-agents/CONSTITUTION.md). Cite these principles by number (e.g., `"Principle 4"`) in your commit messages, pull requests, and discussions.

### Critical Ontological Rules
*   **One Canonical Source (Principle 4)**: Do not hand-edit generated files. Any change must be made in the canonical source files.
    *   **Mappings**: Authored in [mapping-dsl/](file:///home/paudley/Active/gmeow-ontology-agents/mapping-dsl/) -> compiled to `mappings/`, `projections/`, etc.
    *   **Statements**: Authored in [statement-dsl/](file:///home/paudley/Active/gmeow-ontology-agents/statement-dsl/) -> compiled to `statements/`.
*   **RDF 1.2 / RDF\*-first (Principles 2 & 3)**: Statement-level metadata (provenance, confidence, temporal scope) is authored as native RDF 1.2 / RDF\* in the statement DSL. The logical core stays OWL 2 DL.
*   **Co-equal & Non-privileged (Principles 9 & 10)**: There is no `primaryName`, `preferredGender`, or single-winner preference. A contested fact is represented as coexisting standpoint-indexed claims. A superseded label/deadname is suppressed using `gmeow:displayable false` rather than deleted.

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
make compile-mappings# Compile mapping-dsl/ to mappings/ and projections/ (run after changing DSL)
make compile-statements # Compile statement-dsl/ to statements/ (run after changing statement DSL)
make compile-check   # Validate that committed projection artifacts match mapping-dsl/
```

### Reasoning & Negative Tests
```bash
make reason          # Check ELK consistency (Docker ROBOT)
make reason-hermit   # Full complete consistency check with HermiT (Docker)
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

## 3. Directory Layout

*   `ontology/modules/` — Canonical Turtle ontology modules. Add new terms here.
*   `mapping-dsl/` — DSL files for alignments. Edit mappings here.
*   `statement-dsl/` — DSL files for statement-level metadata. Edit statement provenance here.
*   `shapes/` — SHACL validation shapes.
*   `queries/verify/` — Negative tests/QC SPARQL queries.
*   `src/gmeow_tools/` — The toolchain CLI Python source code.
*   `tests/` — Integration and competency tests.
