---
name: gmeow-ontology-authoring
description: >-
  Helps edit, compile, validate, and reason about the GMEOW ontology modules, mappings, and statement metadata.
  Use when you need to make changes to classes, mappings, properties, or statement provenance.
---

# GMEOW Ontology Authoring & Verification

This skill guides the agent in modifying, compiling, and validating GMEOW ontology resources.

## Guidelines

1. **Constitutional Alignment**:
   - Every design decision must align with [CONSTITUTION.md](file:///home/paudley/Active/gmeow-ontology-agents/CONSTITUTION.md).
   - Cite Constitution Principles (e.g. "Principle 4") in your pull requests and commits.
2. **One Canonical Source (Principle 4)**:
   - **Mappings**: NEVER edit files under `mappings/` or `projections/` by hand. Edit files inside `mapping-dsl/` and run `make compile-mappings`.
   - **Statements**: NEVER edit files under `statements/` by hand. Edit files inside `statement-dsl/` and run `make compile-statements`.
3. **No-Drift Gate (Principle 7)**:
   - Run `make compile-check` or `make statements-check` to verify that compiled output is synchronized with the DSLs.

## Actionable Instructions

- **Edit Ontology Core**:
  Ontology modules are located under `ontology/modules/`. Modify Turtle files directly there.
  After making modifications, run syntax validation:
  ```bash
  make validate
  ```
- **Edit Mappings**:
  1. Open and edit files in `mapping-dsl/`.
  2. Compile the DSL to target artifacts:
     ```bash
     make compile-mappings
     ```
  3. Validate Wikidata syntax and links:
     ```bash
     make wikidata
     ```
- **Edit Statement Provenance**:
  1. Open and edit files in `statement-dsl/`.
  2. Compile statement-level metadata:
     ```bash
     make compile-statements
     ```
- **Run Ontology Reasoning**:
  - Run fast ELK reasoner:
    ```bash
    make reason
    ```
  - Run full HermiT reasoner (gated for releases):
    ```bash
    make reason-hermit
    ```
  - If reasoning fails, explain unsatisfiable classes:
    ```bash
    make explain
    ```
- **Run Negative Verification**:
  Ensure logical constraints are met by running closed-world negative checks:
  ```bash
  make verify
  ```
