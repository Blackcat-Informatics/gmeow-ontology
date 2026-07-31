<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The projection-vocabulary ratchet

A `make check` gate that caps hand-authored growth in the vocabularies `logic:`
subsumes as lossy projections, and forces new validation/reasoning logic to be
authored as `logic:` instead. It is the *enforcement* complement to the
measure-only `gmeow:axisShapeMigration` axis (see
[`docs/SLICE_QA.md`](./SLICE_QA.md#migrating-a-slices-hand-authored-shapesttl-to-logic)
and [`docs/MIGRATING-SHAPES-TO-LOGIC.md`](./MIGRATING-SHAPES-TO-LOGIC.md)), and the
inverse-polarity twin of the raise-only `gmeow:AxisFloorCommitment` floor system.

## 1. Doctrine

`logic:` is the canonical reasoning language of this repository. Per
**Principle 17**, OWL, SHACL, Datalog, Prolog, N3, gUFO, BFO, DOLCE, and the
alignment stack (SSSOM, EDOAL, FnO) are **generated lossy projections** of
`logic:`, each carrying a preservation judgment (`SoundUnderApproximation`, etc.)
in the loss ledger. Validation/reasoning logic belongs in `logic:` and is
*projected outward*; it is never authored twice.

A hand-authored construct in one of those projection surfaces that carries no
back-reference to the `logic:` axiom it was derived from is a **second source of
truth** — the same shape or axiom now exists in two places that can silently
drift apart. That is the smell this gate measures and caps.

## 2. The counted quantity — the ungrounded residue

For every `(slice, vocabulary)` pair the gate guards:

```text
residue(slice, vocab) = |hand-authored constructs in vocab's namespace(s)|
                       − |constructs carrying a RESOLVABLE logic:formalizes back-ref|
                       − |validated correspondence cells at the vocab's OWNER boundary|
```

- **Surfaces scanned:** every slice-owned `*.ttl` — `module.ttl`, `shapes.ttl`, and
  every `.ttl` under a `mappings/` directory (scanned RECURSIVELY, matching the base
  reconstruction so working and base never diverge) — plus the repo-level
  `dsl/mappings/` surface (the one intentional hand-authored FnO carve-out path
  `dsl/mappings/transforms.fno.ttl`), attributed to the DSL surface IRI.
- **Grounding subtraction:** a construct is excluded only if its `logic:formalizes`
  back-reference resolves to an **appropriately-typed grounding construct** — a
  target whose `rdf:type` is a `logic:` axiom class (`logic:Formula`/`logic:Rule`/
  `logic:*Assertion`) or a named `owl:AllDisjointClasses`. A dangling back-ref, or one
  to an untyped/non-axiom target, does NOT ground (migration debt cannot be faked with
  a rubber-stamp). (`logic:grounds` was a phantom predicate the ontology never defined;
  it is gone.)
- **By-reference alignment carve-out:** a triple is subtracted as a legitimate bridge
  ONLY when it is the asserted base triple `<s> <p> <o>` of a **native alignment cell**
  — a reified RDF-1.2 match statement carrying the `gmeow:sssomFile` discriminator
  (predicate ∈ `skos:*Match` / `owl:equivalent*` / `owl:sameAs` / `rdfs:sub*Of`). A
  cell whose reifier carries a complete grounding envelope (a
  `logic:GroundingCorrespondence` with `logic:sourceEndpoint`/`targetEndpoint` and its
  morphism/preservation judgments) is exempt ONLY on the vocabulary's
  `gmeow:vocabularyOwner` (grounding-slice) surface — off its owner it counts, and the
  external term may be authored only at that boundary. An ordinary (non-grounding)
  alignment cell over the structural `rdfs:` taxonomy is a first-class correspondence
  record and is exempt on any surface, EXCEPT a purely internal `gmeow`-to-`gmeow` cell,
  which stays in the residue as a genuine second source. A raw
  `rdfs:subClassOf`/`owl:equivalentClass` with no reifier — even in `module.ttl` — is
  NOT a correspondence cell and is never exempt.

### Per count-kind

| `gmeow:vocabularyCountKind` | Applies to | What is counted |
|---|---|---|
| `countKindShape` | SHACL (`sh:`) | STRUCTURAL ROLE: typed `sh:NodeShape`/`sh:PropertyShape`, plus subjects of `sh:path`, objects of `sh:property`/`sh:node`, and subjects of `sh:sparql`/`sh:rule` — so an anonymous nested `sh:property [ sh:path … ]` block embedded in `module.ttl` is caught, not only a typed top-level shape in `shapes.ttl`. |
| `countKindTypedAxiom` | gUFO, BFO (BFO-core), the OBO families (RO, IAO, PATO, SO, MFOEM, STATO, OBCS, OBI), DOLCE, SUMO, YAMATO, OpenCyc, SIO, FnO, EDOAL, SSSOM, and the math (Data Cube, QUDT, OpenMath, MathML, OM-2, UCUM) and lang (OntoLex, LexInfo, WordNet, NIF, Web Annotation, UD, Lexvo, Glottolog) families | Distinct triples whose predicate or object IRI falls in the vocabulary's namespace(s). Prose (a literal object) never counts. |
| `countKindStructuralAxiom` | RDFS (`rdfs:`) | Distinct triples whose PREDICATE is in the `gmeow:vocabularyCountPredicate` allowlist — the minimum useful structural set `rdfs:subClassOf`/`rdfs:subPropertyOf` (the subsumption taxonomy). Pure annotations (`rdfs:label`/`comment`/`isDefinedBy`/`seeAlso`) and property signatures (`rdfs:domain`/`range`) are NOT counted. |
| `countKindNonRdfSurface` | Datalog, Prolog, N3 | Structurally **0** always — documentary-only registry entries; their rule/clause syntax is not RDF and cannot be hand-authored as TTL triples. Never an enforceable gate. |

## 3. OWL and RDFS

The permanent OWL/RDFS derive-source carve-out is **removed** — treating OWL as a
forever-exception contradicts Principle 17 (OWL is a projection of `logic:`). In its
place the ratchet guards a **minimal, bounded** slice of it:

- **RDFS** is guarded as `countKindStructuralAxiom` over exactly `rdfs:subClassOf` and
  `rdfs:subPropertyOf` — the subsumption taxonomy, genuine reasoning content that
  should be authored as `logic:` and projected. Annotations (`rdfs:label`/`comment`/
  `isDefinedBy`/`seeAlso`) are never counted (they are metadata, not a second source of
  reasoning truth), and property signatures (`rdfs:domain`/`range`) are RBox
  declarations, not migration debt — also uncounted.
- **OWL is not guarded at all.** `owl:cardinality` (exact/unqualified procedural OWL)
  is already gated at `reason-verify` per `LOGIC-VALIDATION.md`, and the remaining
  declarative OWL is left untouched by this ratchet. Guarding it would flood the seed
  with annotation-like debt for no reasoning benefit.

Each guarded vocabulary carries a `gmeow:vocabularyOwner` — the one grounding slice
(`logic:`, `math:`, or `lang:`) at whose mapping boundary its external terms may be
authored; every other slice must reference the owner's grounding-vocabulary term.

## 4. The three ratchet invariants

Let `measured(view, slice, vocab)` be the ungrounded residue from the shared
counter (§2) over a given view (the working tree, or a reconstructed merge base).
Let `effectiveCeiling(view, slice, vocab)` be the explicit `gmeow:ceilingCount` of
the matching `gmeow:ProjectionCeilingCommitment` if one exists for that
`(slice, vocab)`, else that vocabulary's `gmeow:vocabularyDefaultCeiling` (`0` for
every guarded vocabulary).

1. **Count gate (working tree).** `measured(working) <= effectiveCeiling(working)`
   for every guarded vocabulary and slice. A failure names the slice, the
   vocabulary, and the excess. With a default ceiling of `0`, this blocks the
   *first* ungrounded use of a vocabulary a slice has never used before.
2. **Monotonicity (base ∩ working).** For every `(slice, vocab)` with a ceiling
   committed in *both* the merge-base and the working tree,
   `ceilingCount(working) <= ceilingCount(base)`. Raising a ceiling is a hard
   violation ("projection ceiling RAISED n→m"); lowering or removing a ceiling is
   always allowed. This is the exact inverse of `axis_floor_monotonicity` (floors
   only rise; ceilings only fall).
3. **Grandfather gate (new ceilings only).** For every `(slice, vocab)` whose
   ceiling is *new* in the working tree (absent at the merge base),
   `ceilingCount(working) <= measured(base)` — a newly-committed ceiling may only
   record residue that already existed at the merge base, never freshly-authored
   residue. This closes the net-new-vocabulary loophole: without it, an author
   could introduce a brand-new ungrounded construct in a previously-clean
   `(slice, vocab)` cell and simultaneously mint a ceiling that grandfathers it in
   the same change. Base `measured` is reconstructed by enumerating the slice's
   base fileset (`git ls-tree <base> <slice-dir>`, not a single `git show
   <base>:<path>`) and reading each surface via `git show <base>:<file>`: a
   surface **present at working but absent at base** contributes `0` (a
   genuinely new file, not an error); a surface **present but unreadable at
   base** is a HARD-FAIL (stop-and-report) — the two cases are never conflated.

**Back-ref integrity.** As stated in §2, a construct is excluded from the residue
as "grounded" only if its `logic:formalizes` back-reference *resolves* to an
appropriately-typed `logic:` axiom (§2). A dangling or non-axiom back-ref does not ground.
Parse or read failure anywhere on the gate's path is itself a HARD-FAIL — the
counter never falls back to a clean residue of `0` on error.

## 5. The guarded-vocabulary table

Ontology-resident as `gmeow:ProjectionVocabulary` individuals in
`slices/core/slice-quality-rubric/module.ttl` — a dogfooded, data-driven guard
list, never a hardcoded set in Rust.

| Prefix | Vocabulary | Owner | Count-kind | Guard |
|---|---|---|---|---|
| `sh` | SHACL | logic | `countKindShape` | Enforced |
| `rdfs` | RDFS (subsumption taxonomy) | logic | `countKindStructuralAxiom` | Enforced |
| `gufo` | gUFO | logic | `countKindTypedAxiom` | Enforced |
| `bfo` | Basic Formal Ontology (`obo/BFO_`) | logic | `countKindTypedAxiom` | Enforced |
| `ro`/`iao`/`obi`/`pato`/`so`/`mfoem` | OBO families | logic | `countKindTypedAxiom` | Enforced |
| `stato`/`obcs` | OBO statistics | math | `countKindTypedAxiom` | Enforced |
| `dul` | DOLCE Ultra-Lite | logic | `countKindTypedAxiom` | Enforced |
| `sumo` | SUMO | logic | `countKindTypedAxiom` | Enforced |
| `fno`/`edoal`/`sssom` | alignment stack | logic | `countKindTypedAxiom` | Enforced |
| `qudt`/`openmath`/`mathml`/`om2`/`ucum` | math families | math | `countKindTypedAxiom` | Enforced (airtight) |
| `ontolex`/`lexinfo`/`wordnet`/`ud`/`lexvo`/`glottolog` | lang families | lang | `countKindTypedAxiom` | Enforced (airtight) |
| `datalog`/`prolog`/`n3` | non-RDF surfaces | logic | `countKindNonRdfSurface` | Documentary-only |
| `owl` | OWL | — | — | Not guarded (already gated at `reason-verify`, §3) |

Every enforced vocabulary carries `gmeow:vocabularySubsumedBy` back to the
`logic:` core (the Principle 17 witness) and a `gmeow:vocabularyPreservation`
judgment (`logic:SoundUnderApproximation`) shared with the rest of the loss
ledger. Each guarded vocabulary also declares `gmeow:vocabularyDefaultCeiling 0`,
so a slice with no explicit commitment is held at zero for that vocabulary.

## 6. Authoring workflow

- **Author new validation/reasoning logic as `logic:` and project it outward.**
  Never hand-author a SHACL shape or a gUFO/BFO/DOLCE/FnO/EDOAL/SSSOM axiom
  directly unless it is a genuine, `logic:formalizes`-backed `ValidationOnly`
  residue.
- **Lower a ceiling only after a genuine measured migration** — equivalence
  before deletion: re-express the hand-authored construct as a `logic:`
  axiom/constraint, prove the projected form reproduces the same behavior
  (counter-examples still fail identically), retire the hand-authored construct,
  *then* lower the `gmeow:ceilingCount` to match the new measured residue. A
  ceiling is never lowered speculatively, and never raised back up (invariant 2).
- **Worked example — a slice at its ceiling, passes.** A slice with an explicit
  `gmeow:pcc-<slice>-sh gmeow:ceilingCount 2` and exactly two ungrounded,
  non-bridge `sh:PropertyShape` constructs in its surfaces: `measured == 2 <=
  effectiveCeiling == 2` — the count gate passes.
- **Worked example — over ceiling, count gate reds.** The same slice gains a
  third ungrounded `sh:property [ sh:path … ]` block with no ceiling edit:
  `measured == 3 > effectiveCeiling == 2` — `make check` fails, naming the slice,
  `sh`, and the excess of `1`.
- **Worked example — monotonicity reds.** An author edits the commitment itself,
  raising `gmeow:ceilingCount` from `2` to `3` without grounding anything:
  `ceilingCount(working) == 3 > ceilingCount(base) == 2` — the gate reports
  "projection ceiling RAISED 2 → 3" and fails, regardless of what is measured.
- **Worked example — grandfather reds (net-new vocabulary).** A slice with no
  prior `gufo:` usage (base `measured(gufo) == 0`) gains one ungrounded
  `rdf:type gufo:Kind` triple plus a brand-new
  `gmeow:pcc-<slice>-gufo gmeow:ceilingCount 1` in the same change:
  `ceilingCount(working) == 1 > measured(base) == 0` — the grandfather gate
  rejects the attempt to smuggle a fresh construct in as though it were
  pre-existing residue.
- **Worked example — grounded, not counted, passes.** The same `sh:PropertyShape`
  block instead carries a `logic:formalizes` triple that resolves to a real
  `logic:` axiom already present in the slice: the construct is subtracted from
  the residue entirely, `measured` drops by one, and the gate passes without
  touching the ceiling.

These scenarios are illustrative only (not wired as slice-resident
`tests/conformance-fixtures/`, because nothing in the current TTL-surface scan —
`module.ttl` / `shapes.ttl` / `mappings/*.ttl` — would exercise a standalone
fixture file dropped under `tests/`, and no existing Rust harness treats
projection-ceiling scenarios as pass/fail cells the way the SHACL
example-conformance harness does); the gate's own unit tests
(`crates/slice-quality/src/gate.rs`, `crates/gmeow-dev-cli/src/dev_slice_quality.rs`)
exercise each invariant directly against inline fixtures.

### Tooling surfaces

- **Seed emitter:** `gmeow-dev slice-quality-seed-ceilings` — prints
  `gmeow:ProjectionCeilingCommitment` TTL seeded strictly from the shared counter
  (never hand-typed, never tuned) for every `(slice, vocab)` with a nonzero
  residue.
- **The gate:** `gmeow-dev slice-quality-gate` — runs the four floor checks plus
  the ceiling pass (count gate, monotonicity, grandfather) and fails `make check`
  on any violation.
- **The debt report:** `gmeow-dev slice-quality-projection-debt` — a ranked,
  report-only CLI surface that computes `measured` live via the same shared
  counter the gate uses and joins it against the resident `ceilingCount`, showing
  per-`(slice, vocab)` measured/ceiling/headroom — the migration dashboard for
  finding where grounding effort pays off. Never tuned; report-only.
- **Generated projections:**
  `generated/governance/slice-quality-projection-ceilings.tsv`
  (`<slice-iri>\t<vocab-prefix>\t<count>`) and
  `generated/governance/slice-quality-projection-vocabularies.tsv`
  (`<prefix>\t<namespaces>\t<count-kind>\t<default-ceiling>\t<preservation>`) —
  read-only, byte-deterministic `SoundUnder` projections of the resident
  `gmeow:ProjectionCeilingCommitment` / `gmeow:ProjectionVocabulary` individuals,
  regenerated by `make check`. Never hand-edited; a ceiling is raised or
  lowered by editing the commitment individual in
  `slices/core/slice-quality-rubric/module.ttl`, not the TSV. Measured/headroom
  are never projected here — they are empirical scan results entailed by no
  resident individual, computed only on the live CLI/gate path.
- **Competency query:**
  [`slices/core/slice-quality-rubric/queries/qc/projection-ceilings.rq`](../slices/core/slice-quality-rubric/queries/qc/projection-ceilings.rq)
  — registered as `ex:cqProjectionCeilingsHold` in
  [`slices/core/slice-quality-rubric/tests/competency.ttl`](../slices/core/slice-quality-rubric/tests/competency.ttl),
  finds committed ceilings whose vocabulary lacks a complete descriptor
  (default ceiling / `logic:` subsumer / preservation judgment) — a non-empty
  result is a failure. It proves the ratchet's *commitments* (not the live
  measured/headroom) are queryable straight out of the bundled ontology via
  SPARQL, dogfooding Principle 17.
