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
   - Run `make check` after canonical source changes that affect generated
     artifacts: it materializes them through the single producer and then gates
     the result, so it is the whole no-drift proof in one pass.
   - Run `make check` before proposing, committing, or submitting changes unless
     the user explicitly narrows validation.
4. **Logic projection doctrine (Principle 17)**:
   - Treat `logic:` as the canonical formal layer. OWL, SHACL, ShEx, SSSOM,
     EDOAL, FnO, SPARQL projection queries, and OWL alignment axioms are
     generated projection dialects, not sources of truth.
   - Existing hand-authored shape surfaces are transitional. Do not satisfy new
     modeling or slice-quality findings by adding hand-authored SHACL or ShEx
     unless the user explicitly scopes work to the legacy migration path.
   - A slice still shipping a hand-authored `shapes.ttl` (or contributing to a
     root `shapes/*.ttl`) carries per-slice migration debt, surfaced by the
     `slice-quality.projection.ungrounded-shape` advisory. Discharge it by
     re-expressing each shape as a `logic:Constraint` / OWL axiom in `module.ttl`
     that PROJECTS to `generated/shapes/*`, proving parity, then retiring the
     hand-authored file — the exact procedure in
     [`docs/MIGRATING-SHAPES-TO-LOGIC.md`](../../../docs/MIGRATING-SHAPES-TO-LOGIC.md).
     Migrate under equivalence-before-deletion: never delete a shape whose check
     is not yet reproduced by the projected union.
   - Cross-ontology linkage and projection should be represented as
     correspondence work: use the current slice-local native alignment cell
     (a reified `skos:*Match` statement carrying
     `gmeow:sssomFile`/`gmeow:justification`/`gmeow:confidence`) and
     `gmeow:ProjectionMapping` frontend honestly so it can lower to
     `logic:Correspondence` with the right relation, direction, loss, law, and
     preservation claims.
5. **Reasoner-driven flagship counter-examples (Principle 17/18)**:
   - When authoring or improving a slice's flagship acceptance scenarios
     (`gmeow:FlagshipScenario`), drive each guarding counter-example to the
     reasoner: the malformed input should make the native solver observe the
     missing or failed entailment at reasoning-runtime, raising the scenario's
     named failure class — not merely trip a structural/SHACL well-formedness
     shape. Because SHACL is a lossy projection of the canonical `logic:` core
     (Principle 17), a counter-example that bites only at the projection surface
     leaves the negative space unproven; run it through the native solver
     (Principle 18) so the failure is raised in the core.
   - The structural/SHACL proxy is the floor; the reasoner-driven counter-example
     is the depth target. Mark each scenario honestly with
     `gmeow:counterExampleDischarge` (`gmeow:structuralDischarge` today,
     `gmeow:reasonerDrivenDischarge` once the solver is actually run over the
     counter-example) — never assert reasoner-driven for a counter-example the
     solver is not run over. The `flagship_counterexample_depth` slice-quality
     axis reads that marker and surfaces which counter-examples remain
     structural-only.
   - See
     [`slices/grounding/logic/design/LOGIC-CONFORMANCE.md`](../../../slices/grounding/logic/design/LOGIC-CONFORMANCE.md)
     (Tests as ontology data) for the counter-example depth expectation.

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

- **Edit translations and the per-slice glossary**:
  A term's annotation coat and its fr/zh translations are **one term-batch** and land
  together — never add a coat and defer its translations. A new coat grows the
  localizable-literal denominator, so an unpaired coat dilutes measured translation
  coverage and reds `make slice-quality-gate` against the slice's raise-only
  `axisTranslationCoverage` floor; ratchet that floor to the freshly measured value on
  each landing. Translations live in `slices/<group>/<name>/i18n/{fr,zh}.po`; they are
  the only home for non-English ontology prose.

  The per-slice terminology **glossary** is a *derived* object — the pipeline folds
  every reviewed `.po` pair into a `gmeow:Glossary` in `graph/lang-glossary-corpus`,
  resident in `gmeow.gts` (never a hand-authored sidecar). `make i18n-lint`
  hard-rejects cross-batch terminology drift — one term translated two different ways
  across batches (`lang:GlossaryTermInconsistency`) — unless the source is an explicit
  `lang:DeclaredTerminologyHomograph`. That cross-node consistency check is authored as
  a `logic:Constraint` + `logic:Formula` in `module.ttl` (the functional-dependency
  dual of the GMN alias-injectivity bijection) and enforced by the Rust detector; never
  hand-author a `sh:NodeShape` for it.

  ```bash
  make i18n-lint
  ```

- **Edit cross-ontology linkage and projections**:
  1. Put pure identity or match linkage in the slice that owns the
     match subject, usually `slices/<group>/<name>/mappings/equivalences.ttl`.
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
  make check
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

  The axis (`axisShapeMigration`) climbs the ladder Grounded `0.60` / Linked `0.75` /
  Exemplified `0.85` / Maximal `0.95`, and each slice is floor-gated at its own committed
  score with **monotonic non-regression** — the gate forbids the measured score falling below
  the committed floor, it does not force ascent. The floor is **ontology-resident**: a
  `gmeow:AxisFloorCommitment` individual (`gmeow:floorSlice` / `gmeow:floorAxis` /
  `gmeow:floorValue`) authored in `slices/core/slice-quality-rubric/module.ttl`, projected
  **read-only** to `generated/governance/slice-quality-axis-floors.tsv` (never hand-edit the
  TSV — it is a generated projection). Raising a committed floor is a deliberate hand-edit of
  that individual (raise-only, hard-fail-enforced), made once the measured score has genuinely
  risen — never bump it ahead of a real uplift. To seed a not-yet-floored axis, emit its
  commitment at the live measured score and paste it into the rubric `module.ttl`:

  ```bash
  gmeow-dev slice-quality-seed-floors --axis axisShapeMigration   # or --all-axes
  ```

  The seeder is emit-only and one-shot per axis: it refuses to lower an already-committed
  floor, so re-running it never regresses a guarantee.

- **Raise the slice's GMN-1 coverage (touch this whenever you extend a slice's vocabulary)**:
  The **GMN-1 Coverage** slice-quality axis (`axisGmn1Coverage`) reports the fraction of a
  slice's own GMN-0 normal-form vocabulary (`module.ttl` + `examples/*.ttl`) the GMN-1 codec
  (`crates/lang-bridge/src/gmn1_codec.rs`) can losslessly round-trip
  (`slice-quality.gmn1-coverage.uncovered`). It realizes `gmeow:gmnCorrNormalToGmn`'s
  `logic:mnemomorphic true` claim, whose declared domain is ALL of GMN-0: the grounding
  slices (`slices/grounding/{logic,lang,math}`) are hard-gated at a committed floor of `1.0`;
  every other slice is floor-gated at its own committed score — an **ontology-resident**
  `gmeow:AxisFloorCommitment` individual authored in
  `slices/core/slice-quality-rubric/module.ttl` (projected read-only to
  `generated/governance/slice-quality-axis-floors.tsv`), with **monotonic non-regression** —
  the gate forbids falling below the committed floor, it does not force ascent toward `1.0`.

  ```bash
  gmeow-dev slice-quality slices/<g>/<s>      # names every uncovered GMN-0 quad
  gmeow-dev slice-quality-gate                # the per-axis floor gate (make slice-quality-gate)
  ```

  Round-trip each named construct by extending the codec's covered fragment, or file a
  named codec-coverage gap against `LANG-GMN.md` if it is genuinely out of scope. Raising this
  slice's committed floor — the `gmeow:AxisFloorCommitment` individual in the rubric slice's
  `module.ttl` (projected read-only to `generated/governance/slice-quality-axis-floors.tsv`),
  seedable at the live score with `gmeow-dev slice-quality-seed-floors --axis axisGmn1Coverage`
  — is a separate, deliberate hand-edit made once the measured score has genuinely risen —
  never bump the floor ahead of a real uplift.

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
