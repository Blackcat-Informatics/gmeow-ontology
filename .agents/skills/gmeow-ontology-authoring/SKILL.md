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
     slice's `mappings/equivalences.ttl`. Author slice-owned projection cells
     beside that slice, normally as `mappings/projections-<profile>.ttl`.
     Shared or cross-slice projection enrichment may live in
     `dsl/mappings/projections/*.ttl`; shared transform function declarations
     live in `dsl/mappings/transforms.fno.ttl`.
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
     notes in the owning slice's `mappings/projections-<profile>.ttl`; use
     `dsl/mappings/projections/*.ttl` only for shared cross-slice enrichment.
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

- **Migrate the slice's hand-authored shapes (do this whenever you touch a slice)**:
  The **Shape Migration** slice-quality axis lists every hand-authored `sh:NodeShape`
  in the slice's `shapes.ttl` that carries no `logic:formalizes`
  (`slice-quality.projection.ungrounded-shape`). Each is a second source of truth to
  migrate into the canon. Prove and clear them:

  ```bash
  gmeow-dev slice-quality slices/<g>/<s>      # lists the ungrounded shapes
  gmeow-dev shape-equivalence --path slices/<g>/<s>   # EQUIV ⇒ the projector reproduces it
  ```

  Author each obligation in `module.ttl` with a **reasoner-safe** antecedent so the
  projector reproduces the shape, then delete the block — **never** `owl:cardinality` /
  `owl:minCardinality` / `owl:maxCardinality` (out of the EL fragment; they red
  `make reason-verify` and block `make check`):
  - at-most-one → `a owl:FunctionalProperty` (→ `sh:maxCount 1`);
  - existence → `owl:someValuesFrom <Class>` (→ `sh:class`; the dropped `sh:minCount` is a
    design-sanctioned ValidationOnly under-approximation);
  - class/datatype → `owl:allValuesFrom`; disjunction → `owl:unionOf`; faceted range →
    `owl:onDatatype` + `owl:withRestrictions`; cross-node check → a `logic:` FOL assertion in
    `crates/pipeline/src/stages/constraint_shapes.rs`.

  A genuine residue the fragment cannot express (exactly-N cardinality, node-level `sh:or`,
  bespoke cross-node `sh:sparql`) instead **keeps its block but adds `logic:formalizes`**
  naming its canonical `logic:` source — the form the blanket projection-purity gate
  legalizes. `docs/SLICE_GUIDE.md` §9 is the reference.

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
