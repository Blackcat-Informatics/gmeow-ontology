---
name: gmeow-ontology-authoring
description: >-
  Helps edit, compile, validate, and reason about the GMEOW ontology modules, mappings, and statement metadata.
  Use when you need to make changes to classes, mappings, properties, or statement provenance.
---

# GMEOW Ontology Authoring & Verification

This skill guides agents in modifying, compiling, and validating GMEOW ontology
resources in the current slices-first, Rust-native repository.

## Guidelines

1. **Constitutional Alignment**:
   - Every design decision must align with [CONSTITUTION.md](../../../CONSTITUTION.md).
   - Cite Constitution Principles (e.g. "Principle 4") in your pull requests and commits.
2. **One Canonical Source (Principle 4)**:
   - **Slices**: Author ontology terms in `slices/<group>/<name>/module.ttl`.
     Slice metadata lives in `manifest.ttl`; slice-local docs, tests, examples,
     mappings, and translations live beside the module.
   - **Mappings and correspondences**: Author pure term linkage in the owning
     slice's `mappings/equivalences.ttl`. Author irreducible projection
     enrichment in `dsl/mappings/projections/*.ttl` and shared transform
     function declarations in `dsl/mappings/transforms.fno.ttl`.
   - **Statements**: Author statement-level metadata in `dsl/statements/`.
   - **References**: Author citations in `metadata/references.ttl`.
   - **Generated artifacts**: NEVER hand-edit `generated/`, root
     `ontology-docs/`, `dist/`, or generated mapping/projection/query outputs.
     Regenerate them from canonical sources.
3. **No-Drift Gate (Principle 7)**:
   - Run `make regenerate` after canonical source changes that affect generated
     artifacts.
   - Run `make check-generated` to verify generated output is synchronized.
   - Run `make check` before proposing, committing, or submitting changes unless
     the user explicitly narrows validation.
4. **Logic projection doctrine (Principle 17)**:
   - Treat `logic:` as the canonical formal layer. OWL, SHACL, ShEx, SSSOM,
     EDOAL, FnO, SPARQL projection queries, and OWL alignment axioms are
     generated projection dialects, not sources of truth.
   - Existing hand-authored shape surfaces are transitional. Do not satisfy new
     modeling or slice-quality findings by adding hand-authored SHACL or ShEx
     unless the user explicitly scopes work to the legacy migration path.
   - Cross-ontology linkage and projection should be represented as
     correspondence work: use the current slice-local `gmeow:TermEquivalence`
     and `gmeow:ProjectionMapping` frontend honestly so it can lower to
     `logic:Correspondence` with the right relation, direction, loss, law, and
     preservation claims.

## Actionable Instructions

- **Orient first**:
  Read `.goals`, `.baseline`, `AGENTS.md`, the relevant slice docs, and the
  relevant `slices/grounding/logic/design/` documents before editing ontology
  semantics.

- **Edit ontology terms**:
  Modify the owning slice, normally `slices/<group>/<name>/module.ttl`.
  Keep every GMEOW-namespaced term annotated with `rdfs:label`,
  `skos:definition`, and `rdfs:isDefinedBy`.

  ```bash
  make validate
  ```

- **Edit cross-ontology linkage and projections**:
  1. Put pure identity or match linkage in the slice that owns the
     `gmeow:alignSubject`, usually `slices/<group>/<name>/mappings/equivalences.ttl`.
  2. Put lossy projection legs, profile bindings, guards, transforms, and loss
     notes in `dsl/mappings/projections/*.ttl`.
  3. Choose the honest relation. Do not force equivalence where the mapping is
     an overlap, bridge view, lossy lens, or affine correspondence.
  4. Regenerate and check generated mapping/projection artifacts:

     ```bash
     make mappings
     make check-generated
     ```

  5. Validate Wikidata syntax and links when QIDs/PIDs are touched:

     ```bash
     make wikidata
     ```

- **Edit statement provenance**:
  Open and edit files in `dsl/statements/`, then regenerate and check drift:

  ```bash
  make regenerate
  make check-generated
  ```

- **Assess slice quality**:
  Use the slice-quality advisor at the start and end of slice uplift work:

  ```bash
  make slice-quality SLICE=slices/core/tags
  make slice-quality
  ```

  Interpret projection and linkage findings through the `logic:` /
  correspondence-calculus roadmap. For example, a missing shape surface points
  toward canonical logic constraints and derived validation shapes, not a new
  hand-authored `shapes.ttl`.

- **Run ontology reasoning and verification**:

  ```bash
  make reason
  make verify
  make reason-verify
  ```

- **Run full validation before handing off**:

  ```bash
  make check
  ```
