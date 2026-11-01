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
  `dsl/mappings/` surface, attributed to the DSL surface IRI. Callable signatures
  belong to their owning slices; FnO has no authoring exception.
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
| `countKindTypedAxiom` | gUFO, BFO (BFO-core), the OBO families (RO, IAO, PATO, SO, MFOEM, STATO, OBCS, OBI), DOLCE, SUMO, YAMATO, OpenCyc, SIO, FnO, EDOAL, SSSOM, the process-model catalogs the `logic:` prescription/enactment spine grounds onto (P-Plan, OPMW, BPMN, RO-Crate, Airflow, CWL, WDL, Temporal, Nextflow, openEHR Task Planning), and the math (Data Cube, QUDT, OpenMath, MathML, OM-2, UCUM) and lang (OntoLex, LexInfo, WordNet, NIF, Web Annotation, UD, Lexvo, Glottolog) families | Distinct triples whose predicate or object IRI falls in the vocabulary's namespace(s). Prose (a literal object) never counts. |
| `countKindStructuralAxiom` | RDFS (`rdfs:`) | Distinct triples whose PREDICATE is in the `gmeow:vocabularyCountPredicate` allowlist — the minimum useful structural set `rdfs:subClassOf`/`rdfs:subPropertyOf` (the subsumption taxonomy). Pure annotations (`rdfs:label`/`comment`/`isDefinedBy`/`seeAlso`) and property signatures (`rdfs:domain`/`range`) are NOT counted. |
| `countKindNonRdfSurface` | Datalog, Prolog, N3 | Structurally **0** always — documentary-only registry entries; their rule/clause syntax is not RDF and cannot be hand-authored as TTL triples. Never an enforceable gate. |

## 3. Quantitative scope and authoring purity

The ratchet measures the specific construct set declared by each registry entry.
It is not the authoring-purity gate. An uncounted construct is not thereby a
permitted second semantic source: Principle 17 still makes `logic:` canonical.

- **RDFS** is guarded as `countKindStructuralAxiom` over exactly `rdfs:subClassOf` and
  `rdfs:subPropertyOf` — the subsumption taxonomy, genuine reasoning content that
  should be authored as `logic:` and projected. Annotations (`rdfs:label`/`comment`/
  `isDefinedBy`/`seeAlso`) are never counted (they are metadata, not a second source of
  reasoning truth). Property signatures (`rdfs:domain`/`range`) remain outside this
  numeric unit. They are semantic declarations and belong in canonical
  `logic:domain`/`logic:range`; their exclusion is not a permanent authoring carve-out.
  Adding signatures to the same existing commitments would change what those
  committed counts mean. Do not silently reinterpret the budget, grandfather a
  larger surface, or manufacture compensating headroom. The current metric remains
  a comparable subsumption budget, rather than an assertion that it measures every
  form of semantic authoring debt.
- **OWL is outside the quantitative registry, but its authoring is checked.**
  The unified projection-purity implementation in
  [`repo_static.rs`](../crates/validate/src/repo_static.rs) rejects unbacked OWL
  predicate assertions and type declarations, including declarative constructs.
  It admits exact native correspondence assertions through the typed frontend,
  rather than granting blanket permission to their other subject triples.
  Required annotation metadata remains distinct. `reason-verify` additionally
  checks its supported reasoning fragment; it is not a substitute for the
  authoring-purity gate. A separate numeric OWL count is not introduced.

The [complete remediation inventory](PROJECTION-DEBT-TRIAGE.md) classifies every
current commitment by its canonical-grounding and ownership route. No nonzero
cell is accepted as permanent ungrounded authoring, and no ceiling or axis floor
is raised by this decision.

Each guarded vocabulary carries an explicit, nonempty set of
`gmeow:vocabularyOwner` values drawn only from the `logic:`, `math:`, and `lang:`
grounding slices. A shared external surface may have multiple real grounding
owners; each canonical source term still belongs to its semantic owner. Membership
grants only the existing exemption for a complete validated grounding correspondence,
never blanket permission to author raw external axioms or template atoms. Singleton
declarations retain their existing meaning. Missing owners, non-IRI objects and
domain-slice owners fail admission rather than disappearing from the set.

The registry meta-ratchet rejects any new owner on an existing guard, including an
owner replacement. Removing an owner strengthens its boundary. It also rejects every
change of count kind: different recognizers have no proven strength ordering. New
guards may declare their complete owner set initially; no existing configuration or
ceiling changes merely because the reader supports sets. Owner-neutral measurement
APIs remain explicitly neutral and never choose a preferred owner.

A broad vocabulary guard requires declared grounding authority for its selected
surface. General publication vocabularies such as PROV-O and schema.org also carry
ordinary domain alignments by reference (Principle 5). PROV-O uses source-scoped
native admission for its kernel correspondences. The schema.org HowTo surface
retains its bounded exemption. Neither route reclassifies ordinary domain alignments
as off-owner residue.

Every registered catalog family selects exactly one governance regime: an existing
broad `gmeow:ProjectionVocabulary` guard, a bounded residue exemption, or
`gmeow:NativeKernelCatalogGuard`. Existing broad guards keep their recognizers and
owner boundaries. The native regime also covers the previously catalog-only kernel
families, including OWL and OntoUML, through their existing native frontend forms.
Canonical source ownership comes from original Module-role declarations and their
actual physical homes, including retained public IRIs such as `gmeow:used`. Every
original Mapping occurrence must retain its complete native term-cell or
projection-binding envelope and catalog-owner home. Missing markers, missing native
discriminators, ambiguous ownership and off-owner duplicates refuse. Shared merged
analysis resolves legal pattern and binding references; it cannot fill missing local
grounding metadata. Ordinary graph transformations are not classified by every kernel
property they happen to reference.

Authenticated registry observations carry the original module and mapping paths,
SHA-256 identities, endpoints and complete non-law semantic keys. Shipped-program
acceptance reconciles those keys and canonical owners; law discharges remain a
separate certified lane. Empty or stale proof records refuse. PROV-O's five kernel
rows graduate through this regime and its target minimum rises to five; a sixth
complete owned correspondence is admitted by the same rule without a row roster.

**The carve-out is a tracked record, not this paragraph.** Prose exempts nothing and
counts nothing: with only this section, rows could be bridged onto an unguarded
family one at a time, indefinitely, with nothing measuring the total. Each exemption
is therefore a `gmeow:ResidueRatchetExemption` individual in
`dsl/mappings/catalog-families.ttl` (vocabulary in `dsl/mappings/vocabulary.ttl`),
in the same shape as every other registry commitment here — a typed row naming its
`gmeow:exemptCatalogFamily`, stating its `gmeow:exemptRationale`, and pinning its
`gmeow:exemptRowCeiling`. The mappings stage
(`crates/pipeline/src/catalog_families.rs::check_residue_exemptions`) hard-fails on:

- a shipped count ABOVE the ceiling — the carve-out grew (and the family's raise-only
  `gmeow:catalogTargetMinimum` pins the same count from below, so a silently deleted
  row is equally red);
- an exemption for a family that IS guarded — the record may not outlive its reason;
- an exemption naming no registered family — a dead row exempting nothing.

The remaining exemption is schema.org HowTo (ceiling 7).
`gmeow:exemptRowCeiling` is LOWER-ONLY, the same polarity as
`gmeow:ceilingCount`: it falls as rows are grounded through an owned surface, and
widening the carve-out means editing the ceiling deliberately, in review, with the
reason written down.

## 4. The four ratchet invariants

Let `measured(view, slice, vocab)` be the ungrounded residue from the shared
counter (§2) over a given view (the working tree, or a reconstructed merge base).
Let `effectiveCeiling(view, slice, vocab)` be the explicit `gmeow:ceilingCount` of
the matching `gmeow:ProjectionCeilingCommitment` if one exists for that
`(slice, vocab)`, else that vocabulary's `gmeow:vocabularyDefaultCeiling` (`0` for
every guarded vocabulary). Let `inflow(slice, vocab)` be the residue a
**declared and corroborated relocation** transported *into* that cell (§4.1);
with no relocation declarations `inflow` is identically `0`.

1. **Count gate (working tree).** `measured(working) <= effectiveCeiling(working)`
   for every guarded vocabulary and slice. A failure names the slice, the
   vocabulary, and the excess. With a default ceiling of `0`, this blocks the
   *first* ungrounded use of a vocabulary a slice has never used before.
2. **Monotonicity (base ∩ working).** For every `(slice, vocab)` with a ceiling
   committed in *both* the merge-base and the working tree,
   `ceilingCount(working) <= ceilingCount(base) + inflow`. Raising a ceiling beyond
   its relocation-adjusted base is a hard violation ("projection ceiling RAISED
   n→m"); lowering or removing a ceiling is always allowed. This is the exact
   inverse of `axis_floor_monotonicity` (floors only rise; ceilings only fall).
3. **Grandfather gate (new ceilings only).** For every `(slice, vocab)` whose
   ceiling is *new* in the working tree (absent at the merge base),
   `ceilingCount(working) <= measured(base) + inflow` — a newly-committed ceiling
   may only record residue that already existed at the merge base or arrived by a
   corroborated relocation, never freshly-authored residue. This closes the
   net-new-vocabulary loophole: without it, an author
   could introduce a brand-new ungrounded construct in a previously-clean
   `(slice, vocab)` cell and simultaneously mint a ceiling that grandfathers it in
   the same change. Base `measured` is reconstructed by materializing the base
   tree once (`git archive <base> -- <slice-dirs>`) and scanning it with the very
   same surface scanner the working tree uses: a
   surface **present at working but absent at base** contributes `0` (a
   genuinely new file, not an error); a surface **present but unreadable at
   base** is a HARD-FAIL (stop-and-report) — the two cases are never conflated.
   Invariants 2 and 3 are one rule evaluated by one comparator: a rule that held
   at one ceiling gate and not the other would not be a rule.
4. **Conservation (base ∩ working).** For every guarded vocabulary,
   `Σ ceilingCount(working) <= Σ ceilingCount(base)` summed over exactly the cells
   committed in **both** views. Relocation moves budget between cells; it can never
   create budget, so the aggregate is lower-only. The scoping to `base ∩ working` is
   load-bearing: invariant 3 explicitly *permits* a brand-new ceiling up to
   `measured(base)` (the worked example below is a new slice with pre-existing
   residue committing a matching ceiling), and every such legitimate addition would
   raise an unscoped Σ while violating nothing. New cells are governed by
   invariant 3; deletions only ever lower Σ.

### 4.1 Relocation-aware accounting — the base ceiling is re-projected, never raised

A ceiling budgets **net-new ungrounded authoring**, and that quantity is
*location-independent*: carrying a term from one slice to another moves residue
between two cells without authoring any of it. So before the lower-only comparison
runs, the **base** ceiling of the affected cells is *re-projected* through the
declared-and-corroborated relocation, and the invariant that then runs is unchanged:

```text
working <= relocation_adjusted_base
```

**No tool ever creates headroom.** A raise beyond the adjustment still reds, and is
still a maintainer-only decision authorized out-of-band by merging past that red —
there is no in-repo permit to raise a ceiling, just as there is none to lower a
floor. Every unit of `inflow` must clear four independent tests:

- **Declared.** A `gmeow:CeilingRelocation` — a dated record modelled on
  `gmeow:AxisExemption` — names the moved `gmeow:relocationTerm`s, the
  `gmeow:relocationFromSlice`, the `gmeow:relocationToSlice`, and optionally the one
  `gmeow:relocationVocabulary` the move is scoped to. The declaration is authored by
  a maintainer; no tool writes one.
- **Witnessed.** `departed(src,v) = base_keys(src,v) − working_keys(src,v)` and
  `arrived(dst,v) = working_keys(dst,v) − base_keys(dst,v)`; an edge's capacity is
  `|departed ∩ arrived ∩ declared|` over the residue constructs' relocation-invariant
  subject anchors. **The departure requirement is load-bearing:** without it a
  construct merely *copied* into a second slice — two second-sources-of-truth,
  strictly worse than one, and what the ratchet exists to prevent — would be
  indistinguishable from a relocated one. A construct whose subject is a blank node
  with no named ancestor has no cross-view identity and can never witness anything.
- **Paid.** Feasibility is a **transport problem**, solved per vocabulary as
  max-flow over the bipartite `(source → destination)` graph, not a per-destination
  greedy sum. Source supply is
  `min( max(0, base_ceil − work_ceil), |departed ∩ declared| )`; the `min` is
  load-bearing, because the corpus carries large *stale* headroom and lowering dead
  headroom surrenders no authoring, so it must never buy live headroom elsewhere. A
  greedy sum accepts the case "two destinations each raised 3, one source lowered 3
  whose keys landed in both" and then reds at invariant 4 with a verdict that
  contradicts its own audit lines and names no culprit; the flow rejects exactly one
  destination, names the blocking edge, and prints the residual demand.
- **Pinned.** Every raised destination's `ceilingCount` must *equal* its measured
  working residue. Without this a relocation that also deletes pre-existing residue
  banks durable surplus headroom, spendable forever with no witness.

**A coincident lowering in the same diff is not credit.** Only a lowering whose
witnessed, declared departures actually landed at the raised destination funds a
transfer; two unrelated edits that happen to move opposite directions in one commit
buy nothing.

**Self-cleaning.** A declaration whose relocation is fully **absorbed at base** (its
terms sit at the destination on *both* sides) is dead and reds until deleted —
otherwise declarations accumulate into standing permits, which is what the
doctrine forbids. A declared term that did not move likewise reds.

### 4.2 The floor/ceiling asymmetry — why floors are not netted

A ceiling and an axis floor look like mirror images, and under relocation they are
deliberately **not** treated alike:

- A **ceiling** budgets net-new ungrounded *authoring*. Authoring is an act, not a
  location, so moving an already-authored construct must cost nothing — hence the
  base re-projection above.
- An **axis floor** measures the *documentation quality of the inventory a slice
  currently owns*. That genuinely **is** location-dependent: importing an
  under-documented term really does lower the destination's measured quality, and the
  correct response is to document the term, not to net the loss away against the
  source's improvement.

So a relocation nets on the ceiling side and never on the floor side.
`gmeow-dev slice-quality-relocation-preview` prints the **axis-floor collateral**
alongside the transport plan so a maintainer sees that cost *before* moving.

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
  the ceiling pass (count gate, the one relocation-aware rebalance covering both
  monotonicity and grandfather, and aggregate conservation) and fails `make check`
  on any violation. Every ACCEPTED relocation transfer is minted onto the
  diagnostics ledger with a stable finding IRI, the destination cell as its anchor,
  and the witnessed terms as its antecedents — never a bare printed line.
- **The relocation preview:** `gmeow-dev slice-quality-relocation-preview --term
  <iri>… --from <slice> --to <slice>` — report-only (always exits 0). Per guarded
  vocabulary it prints the transport plan, the residual **unpaid** demand, and the
  residue-conservation reason codes; then, once, the **axis-floor collateral** for
  both slices, because floors are deliberately not netted (§4.2) and the cost must
  be visible before the move rather than after it. If none of the requested terms
  anchors residue in the source slice it says exactly that — a statement about the
  *requested terms*, never about whether the slice carries residue — and then lists
  every term in the source that **does** anchor residue, with the construct count
  each would carry. That listing is the discovery surface: the relocation-invariant
  anchor is a *derived* quantity (a nested anonymous `sh:property` block anchors on
  its nearest named ancestor), so it cannot be read off the Turtle and a maintainer
  has no other way to find a `--term` value.
- **The debt report:** `gmeow-dev slice-quality-projection-debt` — a ranked,
  report-only CLI surface that computes `measured` live via the same shared
  counter the gate uses and joins it against the resident `ceilingCount`, showing
  per-`(slice, vocab)` measured/ceiling/headroom — the migration dashboard for
  finding where grounding effort pays off. Never tuned; report-only.
- **The installed consumer report:**
  `gmeow slice projection-debt /path/to/slice --format tsv` measures the explicitly
  supplied authored slice against the registry and commitments embedded in the
  installed bundle. No checkout, generated tree, producer or fixture is required.
  The directory supplies one `manifest.ttl` identity and a `module.ttl`; optional
  `shapes.ttl` and recursively nested `mappings/*.ttl` are authoring inputs.
  Examples and tests are not authoring inputs. Missing mandatory inputs,
  malformed or unreadable selected files, ambiguous identities and mapping cycles
  fail instead of appearing clean. Readable file links remain usable; dangling
  selected sources fail. Documentary non-RDF registry entries are not reported
  as measured RDF debt. Output supports `human` and `tsv`.
  Each measurable vocabulary has measured/ceiling/headroom fields, including
  explicit zero/default cells and negative deficits. Source-local ceiling
  declarations cannot raise the embedded authority. Both CLI surfaces use the
  exact signed headroom and wide checked totals; neither saturates at the
  signed-64-bit boundary. A deficit is report data and does not edit a commitment.
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
  resident individual, computed only on the source-backed CLI/gate path. An
  installed immutable bundle cannot observe later edits to an author's source
  directory. Supply those sources explicitly for the consumer report; whole-repo
  discovery, shared DSL mapping surfaces and ceiling maintenance remain developer
  operations.
- **Competency query:**
  [`slices/core/slice-quality-rubric/queries/qc/projection-ceilings.rq`](../slices/core/slice-quality-rubric/queries/qc/projection-ceilings.rq)
  — registered as `ex:cqProjectionCeilingsHold` in
  [`slices/core/slice-quality-rubric/tests/competency.ttl`](../slices/core/slice-quality-rubric/tests/competency.ttl),
  finds committed ceilings whose vocabulary lacks a complete descriptor
  (default ceiling / `logic:` subsumer / preservation judgment) — a non-empty
  result is a failure. It proves the ratchet's *commitments* (not the live
  measured/headroom) are queryable straight out of the bundled ontology via
  SPARQL, dogfooding Principle 17.

## Selected native targets and catalog authority

Selected template targets are admitted before any guarded-namespace filter. The
counter resolves `gmeow:tPredFromCorrespondence` against a required independent
grounding catalog, using the same native correspondence, source specialization and
mandatory variable-flow checks as the compiler. A virtual target construct carries
that proof while the original surface remains the denominator: importing the
catalog's quads into the measured surface would change the inventory and is
forbidden. Missing or malformed selection is an error, not zero residue.

Repository catalogs use actual kernel-owned Mapping artifacts and canonical
module-role inputs. A base measurement archives those exact inputs from the base
commit, retains its admitted catalog with the base tree, and reuses it for
destination and relocation comparisons. Working-tree inputs never replace missing
base evidence. A genuinely absent selected directory at base still measures zero;
an existing surface with missing required base grounding inputs fails.

The schema.org procedure family uses the six IRI prefixes `HowTo`, `Recipe`,
`supply`, `tool`, `result` and `step`; `HowTo` also covers `HowToStep`. Its seven
existing correspondences and exemption ceiling seven remain. The separate
`https://schema.org/value` family requires both the Math quantity bridge and Lang
literal bridge, with a shared Lang/Math guard and default ceiling zero. These are
prefixes, not exact-term matches: `valueReference` and other `value` suffixes are
guarded too. Unknown endpoints and overlapping family/guard declarations retain
their existing hard refusals. This partition grants no new headroom and removes
no semantic correspondence.
