<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — The Correspondence Calculus

> Member of the GMEOW Logic design set ([`LOGIC.md`](LOGIC.md)). This document is the **normative
> canon** for cross-ontology alignment as a first-class `logic:` construct. Its extended rationale
> — the research synthesis, the optic/Galois/institution/GLAV derivations, and the worked
> openEHR use cases — lives in [`docs/APPLIED_CATEGORY_THEORY/take1.md`](../../../../docs/APPLIED_CATEGORY_THEORY/take1.md)
> and the sibling `usecase_*.md` + `fixtures/` there. Where this document and `take1.md` differ,
> **this document governs**; `take1.md` is the cited rationale, not a second source of truth.
>
> **Reading this document.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by the conformance corpus
> ([`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md) § Correspondence). It is not a claim that any
> implementation already realizes X except as the corpus demonstrates.

## The document set

| Document | Role |
|---|---|
| [`LOGIC.md`](LOGIC.md) | manifesto, vision, lineage |
| [`LOGIC-IR.md`](LOGIC-IR.md) | the typed IR `logic:Correspondence` joins as the ninth node kind; the three IR commitments |
| [`LOGIC-RUNTIME.md`](LOGIC-RUNTIME.md) | the native execution engine the calculus lowers to |
| [`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md) | the Correspondence conformance category and its gates |
| `LOGIC-CORRESPONDENCE.md` (this) | the alignment calculus — node kind, law-spine, mnemomorphism, lowerings |

## The thesis

SSSOM, EDOAL, FnO, SPARQL CONSTRUCT, OWL alignment axioms, and the up-projection lift map are **not
alignment sources**. They are **generated target dialects** of one canonical object: a
`logic:Correspondence`, a first-class, RDF 1.2-native, law-bearing node kind in the typed IR. A
correspondence carries typed source/target theories, executable `get`/`put` legs, an algebraic kind
on an ordered law-spine, its claimed laws with discharge status, the separated quantitative/contextual
axes, FOL/SOL caveats, and a preservation judgment. "GMEOW perfectly subsumes vocabulary `V`" is a
**CI-checkable section/retraction law** (`u ∘ d = id`), not a slogan.

This is the project's own doctrine applied to alignment. Principle 4 (one canonical source; everything
else a generated lossy projection) and Principle 17 (the logic is canonical; OWL/Datalog/SHACL/gUFO
are projections) already performed this move for facts and for axioms. The correspondence calculus
performs the identical move for cross-ontology alignment: the alignment layer becomes a generated
projection of `logic:`, peer to the OWL/Datalog/gUFO projections, never a second source of truth.

The mapping DSL is the ergonomic frontend, not a competing semantic layer. A frontend cell may
state its correspondence class, morphism kind, and preservation kind; the compiler folds those
judgments into the typed IR, assigns content-addressed identity, derives the executable legs, and
governs the result through the loss ledger. The up-projection was formerly an independent
SSSOM-reading heuristic authored separately from the down CONSTRUCT; it is now the derived `put` leg
executed natively by `crates/pipeline/src/put_executor.rs`.

The realized grounding instance is
[`slices/grounding/logic/mappings/grounding-bridges.ttl`](../mappings/grounding-bridges.ttl).
Its `logic:GroundingCorrespondence` frontend marker requires explicit class/kind/preservation
judgments and compiles to a `logic:Correspondence` that retains the marker plus named
`logic:sourceEndpoint` and `logic:targetEndpoint`. Those records ship in the
`graph/correspondence-laws` named graph of `gmeow.gts`; SSSOM remains only a generated lowering.

## The ninth node kind

[`LOGIC-IR.md`](LOGIC-IR.md) defines the typed IR as a sum of node kinds. `logic:Correspondence` is a
node kind, defined *in terms of* the existing kinds rather than as a specialization of any one:

- a **meta-formula envelope** — the relation between a source pattern and a GMEOW pattern, with its
  caveats, standpoint index, and quantitative axes. It is meta-level (a statement *about* the
  relationship between propositions) and **stays meta**: it must not leak into object-level closure.
  The IR's stratification rule for any `holds`/truth predicate enforces this — it is how the current
  "never reasoned over" invariant survives while the correspondence itself becomes reasoned data;
- two **executable legs** — `get` (down-projection) and `put` (up-projection), each a
  transaction-program over the path semantics of [`LOGIC-TRANSACTION.md`](LOGIC-TRANSACTION.md)
  (`ins`/`del`, hypothetical execution, supersession-not-erasure). The leg body reuses the existing
  pattern/expression algebra and `logic:PathShape` ([`LOGIC-PATHS.md`](LOGIC-PATHS.md)) — no new
  pattern language is introduced;
- a set of **law constraints** — the laws the correspondence claims (`logic:GetPut`, `logic:PutGet`,
  `logic:PutPut`, `logic:SectionLaw`), each a `logic:LawClaim` whose **status reuses the foundation's
  executable discharge vocabulary** ([`LOGIC-FOUNDATION.md`](LOGIC-FOUNDATION.md), § Typed
  formalization governance): a `logic:DischargeVerdict` (`logic:ObligationDischarged` /
  `logic:ObligationUnknown` / `logic:ObligationViolated`) and, when applicable, the
  `logic:DischargeCondition` under which it was checked. A law proved in a certified fragment is
  `ObligationDischarged` under `logic:DischargeCertifiedFragment`; a refuted law is
  `ObligationViolated` with its countermodel; an authored-but-unchecked or inconclusive law is
  `ObligationUnknown`, carried forward, never silently passed.

A correspondence is a *new top-level kind*, not a meta-formula with an attached transaction-program,
because content-addressed identity and the preservation judgment must attach to the correspondence **as
a unit**: the loss ledger needs one preservation row per correspondence, hashing the relation, the
axes, the caveat, *and both legs* together, so it can attribute a dropped construct to the leg that
dropped it. This is the same reasoning that keeps `constraint` distinct from `derivation-rule`.

Concrete loss evidence is authored as `logic:lossyDrop` literals on the owning
correspondence. Every literal retains its lexical form, datatype, language and RDF 1.2
base direction through the typed carrier, content identity and canonical projection;
the correspondence's standpoint, provenance evidence and legs qualify that same owner.
Human loss reports may display lexical messages while retaining the native evidence.
Caveats and preservation rungs do not imply particular drops, and one cell's evidence
must never be copied to an unrelated cell. A declared judgment with no concrete loss
asserts no fabricated residue: `ExactPreservation` rejects any loss, `Unsupported`
requires actual evidence, and loss evidence without `preservationKind` is rejected.
Identity labels never count as residue. Program-level unowned `lossyDrop` declarations
are unsupported and rejected; each actual distinction must name its correspondence.
These declarations are evidence disclosures, not discharged laws or fusion certificates.

### The relation lattice

The correspondence's relation is a typed `logic:CorrespondenceRelation` on an ordered lattice, not a
free string: `equiv` ⊐ {`subsumes`, `subsumedBy`} ⊐ `overlaps` ⊐ `relatedMatch`, with `disjoint` as
the negative pole. The lattice lets the compiler **derive** the SSSOM predicate, the EDOAL relation
symbol, and the OWL-alignment-axiom strength from one authored relation, so the artifacts cannot drift.

### Frontend versus canonical form

A correspondence *compiles into* the IR and *may carry* FOL/SOL caveats and laws, but authors do not
hand-write raw FOL. The frontend stays ergonomic and slice-local (the existing
`gmeow:ProjectionMapping` ergonomics are a starting frontend); the canonical, reasoned, law-bearing
form is what the compiler produces. This is the standard language/IR separation, and it is what keeps
maximal expressivity at the centre from becoming an unusable surface (the Ithkuil failure mode the
projection doctrine exists to avoid — see [`LOGIC.md`](LOGIC.md)).

## Orientation convention

A correspondence is an asymmetric lens between a rich **source** `S` and a derived **view** `V`, with
`get : S → V` and `put : V × S → S`. This executable `get`/`put` core is the **`logic:Lens`** a
`logic:Correspondence` wraps; the correspondence adds the relation, the quantitative axes, the law
claims and the standpoint envelope around it. **GMEOW is the source `S`; the external vocabulary is the
view `V`.** Therefore down-projection (GMEOW → external) is `get`, and up-projection (external → GMEOW)
is `put`. The view is the smaller, derived thing; `put` folds a (possibly fresh) view back into the rich
source. The ingest-with-no-prior-state case (`S` empty) is exactly where the view alone is insufficient
and a **witness must travel in the view** — the mnemomorphism, below.

This orientation is also an ownership rule: grounding catalogs live in `lang:`, `math:`, or
`logic:` according to their semantic domain, and their external vocabulary is always the target.
For the formal grounding catalog, gUFO, BFO, OBO/RO, SUMO, OWL/RDFS, and SHACL are therefore views
of `logic:`, never sources from which `logic:` is defined.

### The quantity boundary — a peer-owned source endpoint

A catalog is selected by its **external** surface, but a row's GMEOW-side endpoint is selected by
**which slice owns the concept**, and for one row the two land in different slices. SUMO is an
upper ontology, so its boundary is `logic:`-owned and its rows live in
[`mappings/grounding-bridges.ttl`](../mappings/grounding-bridges.ttl). SUMO nonetheless carries a
`Quantity` class, and the [`GROUNDING.md`](../../../../docs/GROUNDING.md) tier rule fixes
`math:Quantity` as the sole class authority for dimensioned magnitude: `logic:` may not mint a
rival. The honest row is therefore `math:Quantity skos:broadMatch sumo:Quantity` — a
`logic:BridgeView` / `logic:CommitmentShiftingBridge` at `logic:ValidationOnly`, because SUMO's
`Quantity` broadly admits numbers and quantifiable entities while `math:Quantity` requires exactly
one explicit `math:Dimension`. A `logic:Quantity → sumo:Quantity` row is the rejected alternative,
and the foundational-bridging conformance suite pins that rejection so it cannot creep back.

This is the single place `logic:` names a grounding peer's term structurally, and it is registered
as such: the **quantity-boundary seam** (`logic:` → `math:`, carrying exactly `math:Quantity`) in
the seam registry of [`../manifest.ttl`](../manifest.ttl). The seam carries one term on purpose —
it sanctions naming the peer-owned quantity authority as a bridge row's source endpoint, and
nothing else. Any further `logic:` → peer reference needs its own registration or, more usually,
belongs in the peer that owns the term.

## The ordered law-spine

A correspondence is classified by **how much invertibility it can lawfully claim**, on one ordered
spine; **each rung caps the laws the correspondence may assert** (the profunctor-optic lattice fused
with the categorical subobject notion):

| Rung | Categorical structure | Laws claimable | Reading |
|---|---|---|---|
| **Isomorphism** | iso (`get∘put = id`, `put∘get = id`) | full round-trip both directions | conf-1.0 equivalence |
| **Section / retraction** | split mono (`put∘get = id_S`; `get∘put` idempotent on `V ⊕ complement`) | source embeds losslessly; augmentation = `S ∖ im(get)`, in the complement | **perfect subsumption** |
| **Well-behaved lens** | asymmetric lens | GetPut + PutGet (PutPut optional) | structured→flat downcast with sound update |
| **Lossy lens** | lens, non-injective `get` | one direction faithful; inverse needs witness/claim/defaults | most schema.org/FOAF downcasts |
| **Prism** | partial map `S → V + S` on a sum/optional | match/build on the in-focus variant only | applies on one variant, passes through otherwise |
| **Affine correspondence** | co-projection onto a shared component | laws on the shared component only | "similar but not quite"; vague-determinacy targets |
| **Bridge view** | commitment-shifting comorphism | *no* satisfaction-preservation claim | BFO / DOLCE / SUMO / YAMATO / Cyc (CycL) |

The two partial-alignment rungs are distinct: a **prism** focuses an optional/sum variant (match-or-pass-through), an **affine correspondence** focuses a sub-structure source and view share. Both sit below the lossy lens and above the bridge view.

Two cross-cutting qualifiers:

- **`logic:morphismKind ∈ {logic:InstitutionMorphism, logic:CommitmentShiftingBridge}`** — the
  institution-theoretic split between a satisfaction-preserving morphism and a commitment-shifting
  bridge (the value is named `logic:CommitmentShiftingBridge` to keep it distinct from the
  `logic:BridgeView` rung it typically accompanies). This is the distinction the foundation already
  draws between gUFO (a truth-preserving down-projection of UFO⁺) and BFO/DOLCE/SUMO/YAMATO (bridge
  views). The loss ledger **refuses** to emit `owl:equivalentClass` for a bridge.
- **`mnemomorphic? ∈ {yes, no}`** — whether the forward leg retains a source witness. Orthogonal to
  the rung; it is the property that lets a correspondence *climb* the spine, because a retained witness
  is what discharges `put∘get = id`.

**Cyc as a bridge-view stress test.** Importing CycL microtheory content is a textbook **bridge
view**: `logic:morphismKind logic:CommitmentShiftingBridge`, no satisfaction-preservation claim, and
— per the refusal already stated above — the loss ledger refuses to emit `owl:equivalentClass` for
it. Cyc is a good calculus stress test because its microtheories (`Mt`s) are not a flat bag of
contexts: CycL's `genlMt` relation orders microtheories by generality, so the set of microtheories
forms a **lattice** under `genlMt`, not merely a partition. The bridge from CycL microtheories into
`logic:` standpoint/context indexing is therefore sharper than a plain commitment-shifting
comorphism between two structureless context sets — it is a **monotone lattice comorphism**: the
map from CycL `Mt`s into `logic:` standpoints preserves the `genlMt` order (more general Mt ↦ more
general standpoint), but it is still *not* an institution morphism, because Cyc's own semantics does
not guarantee satisfaction-preservation across that order (a specialization can locally contradict
the generalization it specializes, which is exactly the "mutually-inconsistent assertions across
contexts" Cyc is prior art for — see [`LOGIC.md`](LOGIC.md) § Lineage and Supersession).

Composition can only weaken the rung, never strengthen it (§ Composition).

## Mnemomorphism

A correspondence is a **mnemomorphism** (μνήμη, *memory*, + -morphism) when its forward map `get`
factors through the **graph of the correspondence** — the source-witness — so that `put` is obtained by
*projecting along the retained witness* rather than synthesizing a plausible source. Equivalently:
`get` carries, in its output, enough of its input that `put∘get = id` holds *because nothing the
retraction needs was discarded*.

The witness is the alignment analogue of a paramorphism's access to the original substructure
(generalized to a histomorphism's cofree-comonad annotation); of a delta/edit-lens trace; of database
provenance (semiring-annotated — see [`LOGIC-RUNTIME.md`](LOGIC-RUNTIME.md) § semiring); and of a TGG
correspondence graph. Mnemomorphism is the dividing line between **subsumption** (recoverable, reaches
the section rung) and **approximation** (reconstructed, falls to lossy-lens/prism and needs a
co-authored `put`-with-claim).

**Backwards-execution is a candidate preimage, not a lawful `put`.** Running a relational leg backward
yields *a* source consistent with the view, not *the* lawful `put`: no GetPut/PutGet guarantee, no loss
tracking, no provenance, and it fails silently on non-injective `get`. It is the amnesic case. Lawful
`put` comes only from (1) a mnemomorphic witness, or (2) a co-authored `put`-with-claim under explicit
mode/tabling/minting declarations and a declared law status. Naive backward-execution is a named
anti-pattern, never the architecture.

### Genuine recovery cases

`logic:RecoveryCase` is the executable evidence for source recovery.  A correspondence links zero or
more cases through `logic:recoveryCase`; each case owns exactly one `logic:recoveryTransform`, an
ordered `∀(source → view)` formula.  The native executor supports the positive-conjunctive binary
RDF-atom fragment: it deterministically instantiates every variable in the complete declared source
pattern, constructs the view, runs the candidate inverse, and compares the recovered RDF atom set to
the source.  Every attached case is conjunctive evidence: all must recover; the first missing or
fabricated atom yields `ObligationViolated` with a deterministic countermodel.

That formula does not replace the correspondence legs.  For every attached case the executor
resolves the actual `logic:getLeg` and `logic:putLeg` transaction bodies, executes their normalized
`logic:LegPath` relations on the same complete source seed, requires the relations to agree under
inversion, and requires every variable-bound endpoint selected by the executable get to survive in
the formula-constructed view.  Constants and predicates may change under the declared transform;
variable bindings may not silently disappear.  A missing, malformed, empty, or unrelated leg body
therefore violates the obligation even when the unchanged recovery formula can invert itself.  The
formula and the resolved bodies are one cross-checked proof object; neither is an independent semantic
source.

Recovery formulas and resolved path bodies lower directly to PurRDF's typed query
algebra. Native algebra admission still applies before execution; constructing
algebra does not waive IRI, variable, path or registry checks. The resolved legs
are prepared once for all cases of a correspondence, while each case executes
against its own immutable source and retains its own countermodel. Atomic path
recovery uses the same typed admission. SPARQL text is an explicit input or output
surface, not transport between these compiler and execution steps.

Compilation derives the correspondence program once and passes that exact program
to native law execution. Gates and returned projection artifacts retain its
executed verdicts, so grading an additional composition does not repeat put
derivation or law execution. Program handle identities bind the complete canonical
correspondence payload, including selected legs, standpoint, determinacy,
mnemomorphic claims and each law's verdict and evidence class. Length-framed
caveats and collection boundaries prevent prose from impersonating identity
structure. Cache hydration must reject a typed payload whose semantic identity
differs from the program re-derived from its backing graph.

The production up-projection program likewise admits fact and claim legs as native
query algebra once per mapping inventory. Its reified-claim layout is shared with
the exported put-query template: mapping evidence, confidence and the distinction
between a claim and an asserted relation have one definition across both surfaces.
Independent inputs share prepared plans while retaining separate execution state
and per-query namespaces for newly created claim cells.

The executor's synthetic view and seed IRIs live only under
`https://blackcatinformatics.ca/logic/recovery#` and
`https://blackcatinformatics.ca/logic/recovery-seed/`.  Authored formulas and leg predicates in
those exact execution namespaces fail closed before seed construction; neighboring canonical
`logic:recovery*` vocabulary remains ordinary usable RDF vocabulary.

A recovery case is deliberately **neutral**.  Strong correspondences carry cases that discharge;
lossy correspondences may carry a refuting case, so changing only the rung or the
`logic:mnemomorphic` boolean cannot manufacture a proof.  The `gmeow:WritingSystem → lang:Script`
case, for example, includes `gmeow:writingSystemType` and `gmeow:textDirection` in the source while
the view omits them, and therefore reds if promoted to section/retraction.

This is a bounded query-class discharge, not a theorem over every possible RDF graph.  The authored
source pattern states the scope and must contain the distinctions on which injectivity depends;
`logic:DischargeBoundedCorpus` records that honesty boundary.  An atomic one-triple path has a
complete synthesized case.  A composite path has hidden intermediate structure and therefore stays
`ObligationUnknown` without an authored complete case — `put = get.invert()` is only candidate
construction, never evidence.

At the **process layer** the in-band witness is realized by a pair of back-references an executed
occurrence carries: `logic:instantiatesSchema` (occurrence → `logic:ActionSchema`, the reusable type)
and `logic:instantiatesPlan` (occurrence → the `logic:Plan` it was executed under, the whole planned
skeleton). Together they are the in-band complement that lets a plan↔execution-record correspondence's
`put` leg recover the planned portion of a run rather than synthesize a plausible plan — the openEHR
Instruction-State-Machine linkage (Instruction → Activity → Action) made canonical. Where the witness
is present the planned portion round-trips; the off-plan reality is an honest loss-ledger entry, never
a failure.

## Independent law domains and evidence

The four laws have separate input domains. Let `s` be an admitted rich source,
`v`, `v₁` and `v₂` independently admitted views, and `s₀` the explicitly selected
initial source for recovery:

| Law | Executed equality | Required input domain |
| --- | --- | --- |
| GetPut | `put(get(s), s) = s` | admitted sources, including nonempty prior state |
| PutGet | `get(put(v, s)) = v` | admitted edited views paired with admitted prior sources |
| PutPut | `put(v₂, put(v₁, s)) = put(v₂, s)` | independently selected successive edits and prior sources |
| SectionLaw | `put(get(s), s₀) = s` | augmented forward views with their required recovery witnesses and declared initial-state policy |

An augmented view includes its load-bearing complement. Every equality compares
the complete declared carrier: graph scope, RDF 1.2 terms, standpoint, provenance,
complements and loss evidence. A missing or corrupt selected complement fails
admission; a `mnemomorphic` flag does not substitute for the witness. Each verdict
binds the exact correspondence, theories, legs, context/caveats, inputs,
`ReasoningContract` and discharge condition.

Checking `get(put(get(s), s₀)) = get(s)` on source-generated views establishes
only that forward-image domain. It does not establish PutGet on independently
edited views, GetPut with prior state, or PutPut. Synthetic recovery cases and
bounded corpora retain `DischargeBoundedCorpus` with their exact coverage.
Unrestricted rewrites require a checked certified-fragment derivation whose
applicability premises hold at the use site. Sample agreement alone cannot
provide that authority.

Synthetic branch-domain admission is also bounded: at most 4,096 distributed
branches, 65,536 pattern occurrences and 128 nested algebra edges. The shared
native query is prepared before law execution; distribution checks its projected
branch and pattern counts before allocating their Cartesian product. The combined
case adds one seed and repeats the admitted pattern inventory once. Malformed or
non-carrier queries and excessive expansion fail admission, rather than yielding
an empty or truncated domain. These limits bound GMEOW's recovery-case synthesis;
they are distinct from the native query runtime's execution budget.

### Native atomic focus and complement execution

An atomic property lens selects ordinary assertions of one source predicate and
renames it to a view predicate, optionally reversing endpoints. Its update is
`put(v, s) = residual(s) ∪ inverseFocus(v)`. The residual retains all assertions
outside the selected focus, statement reifier bindings, annotations, graph
declarations and native source sidecars. Quoted historical statements remain
statements about their original values; updating a magnitude does not rewrite
its earlier provenance record. This primitive does not own edits to metadata.
Acquisition requires native statement classification: a flat wire carrier with
unfolded reifier declarations must cross its import boundary first. Otherwise
table-dependent focus selection could mistake statement annotations for ordinary
assertions. The lens refuses that ambiguity rather than silently normalizing or
reinterpreting the selected source.

The operation has a fixed graph catalogue, including empty named graphs. An
independently edited view must remain within that catalogue and the selected
predicate. A new assertion cannot silently become an annotation of a residual
reifier. Such edits fail admission. Inputs to the native shared-scope update
entry point already share an explicit blank-node identity authority; parsing an
independent document does not establish that authority.

The augmented view carries its required residual as an immutable native handle.
The explicit `EmptyWithComplement` initial-state policy reconstructs the rich
source from that residual and the inverse focus. The residual is a suppression
view over the original native dataset; subsequent edits share it and retain only
the current focus. No complete source is serialized or materialized by get/put.
Complete-carrier materialization used by a bounded law comparison is a separate
evidence boundary. Persisting or exporting this augmented value must preserve
the complement; the projected RDF view alone is not a recovery artifact.

The native executor checks GetPut, PutGet, PutPut and SectionLaw separately.
Atomic path recovery uses this executor with the independently resolved candidate
put and an explicit empty initial state. Its existing synthesized one-triple
domain still yields only bounded section evidence: it neither supplies the other
three domains nor certifies unrestricted rewrites or composite-path recovery.

Native atomic composition executes get left to right and put right to left. Each
intermediate view predicate must match the next source predicate. This is an
admission check on the selected atomic carrier types, not a general theorem about
the correspondence wrapper's theories or quantitative axes. It composes executable
lenses, not `LegPath::Seq` relational paths: following a path through intermediate
nodes does not establish that those nodes can be recovered from its endpoints.
Every original stage retains its own inversion and native retention checks; two inversions
cannot erase a literal-subject refusal at the intermediate stage.

The first complement retains the rich source. Later complements retain only the
fixed graph catalogue: their inputs are typed, freshly projected atomic views,
whose ordinary assertions all belong to the next focus. Those intermediate
complements share one native graph catalogue. The augmented terminal view owns
the ordered complements and checks their exact stage programs and
retention policies at recovery. Discarding an intermediate view does not discard
its recovery information or retain its obsolete payload through a residual base.

On reverse execution, complete-focus reuse requires an operation-local witness:
the residual comes from the typed graph-catalogue boundary, has no ordinary,
reifier or annotation rows, and the native focus and graph catalogue belong to
the actual immutable publication. Only then can the preceding stage consume the
native focus directly instead of compacting the intermediate composite. Every
preceding stage still admits and executes its own put. This check proves the
specific carrier replacement against its complete input; its operational trace
is neither a sampled law discharge nor an unrestricted optimizer certificate.
Whole-carrier law comparisons remain a separate evidence boundary.

## The quantitative and contextual axes

A correspondence carries each axis **separately**, because they answer different questions and the
single `gmeow:confidence` of the old DSL destroys the distinctions:

- `logic:confidence` — the curator's epistemic confidence that the alignment is correct;
- `logic:evidenceStrength` — provenance-derived warrant (manual / lexical / structural / LLM);
- `logic:weight` — solver ranking, when competing correspondences exist for one source;
- `logic:probability` — only under a declared dependency model; most carry none;
- `logic:Determinacy` — whether the *target relationship* is ontically crisp or vague. "Similar but
  not quite" is `determinacy = vague` + `class = affine`, **not** low-confidence equivalence.

Admission to a selected typed representation must distinguish an absent field from
an unrecognized, malformed or conflicting present value. A scalar representation
cannot choose one of several authored values; admitting that multiplicity requires
an explicit contextual lowering that preserves every alternative. Enum values use
their canonical `logic:` identities. Malformed quantitative values cannot erase an
axis, and a malformed witness-retention flag cannot become false. Every declared
law, recovery, caveat and selected-program reference must be accounted for; filtering
an unreadable member out of the collection is not a legal lowering.

Quantitative coordinates retain the complete authored RDF literal, including its
numeric datatype and lexical form, through typed compilation, program identity,
canonical graph projection and cache ingress. Numerically equal literals are not
thereby the same authored RDF term. The native scalar profile admits finite RDF
integer, decimal, float and double values within the native datatype's supported
precision; confidence, evidence strength and probability additionally require
`[0, 1]`. Unsupported precision, a nonnumeric datatype or a conflicting scalar
fails admission. Range validation uses the declared numeric value, without first
rounding an exact decimal through binary64. SSSOM and EDOAL's binary64 measures
are explicit lossy export boundaries whose loss records name the dropped RDF
identity; those export values never become canonical input coordinates.

Caveat ownership survives every typed program boundary. A correspondence's
identity includes its limitations; assembling a new program, deriving a put leg
or attaching law evidence cannot detach them. Named textual caveats preserve
their identity and every authored comment in the correspondence IR. Comments
form an unordered RDF set with no preferred language; their complete lexical
forms, datatypes, languages and base directions survive compilation, cache
transport and faithful projection. Numeric-looking comment text is never
coerced into a quantitative axis. Formula-valued caveats require
their own typed lowering and meta-level admission; retaining prose does not
discharge those obligations.

Every correspondence is **standpoint-indexed** (`gmeow:accordingTo`, the typed context algebra of
[`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md)). An unindexed correspondence holds in
`gmeow:unspecifiedStandpoint` — **unspecified, not universal** — which kills the silent-universality bug where
a curated alignment is applied where it was never validated.

## Composition and merge

Author a named `logic:CorrespondenceComposition` with exactly one
`logic:compositionFirst`, `logic:compositionSecond` and `logic:compositionResult`.
These reference correspondence individuals; acquisition executes first then
second, while updates execute second then first. Every selected declaration
enters the canonical IR, the program's content identity and the production
composition report. `logic:hasComposition` records membership in the projected
program. Malformed declarations retain their original source diagnostics and
fail production admission; a missing member cannot become an empty inspection.

The declaration gate requires explicit source and target endpoint references
on both members and the result. The middle references must match, and the result
must name the chain's outer endpoints. All three `logic:accordingTo` references
must match exactly, including absence: an unspecified standpoint does not grant
universal applicability. Cross-context composition requires an explicit context
transport judgment. These reference checks prove neither theory equivalence nor
execution or fusion. The original source selection retains its annotations and
provenance; the declaration's named identity remains visible in the gate report.

**Composition (sequential, `C₁ ∘ C₂`)** computes each axis in its own algebra: class by optic-lattice
join (monotone-downward — composition only weakens the rung); laws with weakest-status-dominates
(`violated` dominates `unknown`, which dominates `discharged`); `confidence` by a declared t-norm
(product requires declared independence); `evidenceStrength` by weakest-link/min; `weight` solver-additive; `probability` only under a
declared cross-chain model (else `not-evaluated`); loss by Galois-connection composition with union of
the unsupported-construct sets. All computed **by `logic:` rules over correspondence nodes** — dogfooded
and conformance-checked, not buried in compiler arithmetic. The warrant is per-law,
with its exact domains and premises. A refuted premise blocks a compositional
certificate; its countermodel remains a premise witness and is not relabeled as
a countermodel of the final composite without executing that composite.

Quantitative execution selects named `logic:Formula` roots with
`logic:compositionAxisRule`. These references, and the composition-owned
`logic:confidenceIndependenceEvidence` and
`logic:probabilityIndependenceEvidence`, survive canonical IR and graph/cache
transport. Each rule branch binds that exact selection and the ordered
first/second/result records. The executor resolves the already-parsed source
Formula through the original immutable frontend publication; it does not
reparse a rule language or recover authority from a report.

Axis rule bodies use a positive, composition-local input fragment: the three
member references, selected rule references, independence assumptions, four
member coordinates, evidence scales, probability models and evidence sources.
Explicit member bindings admit both aliases and constants. These facts come
from the typed composition and correspondence records; an unrelated record
cannot become available merely because another composition shares its batch.
An unsupported predicate or foreign subject fails admission, even when no
result value is claimed. It must not be interpreted as a missing premise.

A body may read a `composedConfidence`, `composedEvidenceStrength`,
`composedWeight` or `composedProbability` value on that same composition only
when the selected native rules include a producer for that coordinate. Native
derivations supply these values; authored observations never seed them. Rule
selection order does not determine availability. Missing admitted premises can
still yield `not-evaluated` for an unclaimed value, subject to the completion
and claim checks below.

The canonical policies are `composeConfidenceProduct` (requiring the explicit
confidence independence assumption), `composeConfidenceMinimum`,
`composeConfidenceLukasiewicz`, `composeEvidenceMinimum`, `composeWeightAdd`,
and `composeIndependentProbability`. There is no implicit product selection.
Evidence minimum requires all three correspondences to name the same
`logic:evidenceScale`. Probability product requires all three to name
`logic:IndependentCorrespondenceProbabilities` through
`logic:crossChainProbabilityModel`, together with the composition's probability
independence evidence. This is distinct from the existing fact-level
`logic:FullIndependence` model. Other dependency models require their own
explicit source rules; naming an arbitrary model cannot imply multiplication.
An assumption reference records what the curator assumed for these exact
operands and standpoint; it is not a proof of independence.

`logic:evidenceSource` retains the full qualitative justification identity.
The mapping frontend never invents numerical warrant bands from SEMAPV terms
or their local names. An authored score and its warrant scale are separate
claims. The qualitative `manualSemanticReview` example in `take1.md` is an
evidence source, not a numeric `evidenceStrength` value. Observation provenance
in the worked examples must likewise remain qualitative unless an explicit
numerical interpretation is authored.

Native rules emit composition-owned `composedConfidence`,
`composedEvidenceStrength`, `composedWeight` and `composedProbability`.
These observations remain separate from the independently authored result
correspondence. Present result claims must agree numerically with a derived
value; absent prerequisites, conflicting outputs or incomplete execution
cannot validate a claimed value. With no claimed output, missing premises are
reported as `not-evaluated`, and an unselected axis as `not-selected`.
Numerical agreement preserves the original literal identity in the source;
it never rewrites that literal into the computed representation.

The selected numeric datatype retains its precision, rounding and definedness
boundaries. A mathematical t-norm name does not establish reassociation of
finite machine arithmetic: floating-point operations and bounded decimal
intermediates need not be associative. Certified fusion must establish both
value preservation and definedness for the admitted input profile and original
rule order. Bounded example agreement cannot supply that proof.

The native executor groups compatible selections within a standpoint, reuses
bounded formula and prepared-program analyses, and seeds typed values directly
into the native store. It retains the exact source roots, assumptions and
native derivations. A user-authored policy remains a conditional calculation:
executing it does not prove it is a t-norm, a calibrated dependency model or an
unrestricted rewrite law. Neither this result nor agreement on the worked
cases certifies optic fusion. Structural and per-law admission still apply.

The declaration gate therefore checks each claimed law against that same law's
declarations and executed evidence on both premises. A successful GetPut cannot
substitute for missing SectionLaw evidence, and an unrelated unknown SectionLaw
cannot erase supported GetPut evidence. Its aggregate status is a summary of
premise bounds, not a verdict on the final composite. Likewise, the weakest
member's declared rung supplies only a necessary weakening bound; it does not
compute a resulting optic type, approximation polarity or quantitative axis.
Those judgments remain owned by canonical composition rules and typed admission.

**Merge (the colimit/pushout direction)** — combining incoming data from several sources into GMEOW
simultaneously is a **colimit/pushout in the category of theories**, gluing along the shared GMEOW apex
without collapsing distinct, possibly contested, contexts. A pushout that would force `owl:sameAs`-style
collapse of standpoint-indexed claims is ill-formed (Principles 5/9): the merge is a colimit in a
category whose objects carry standpoint indices, so contested claims coexist.

The executable merge category is **finite standpoint-indexed theory
presentations**, with an explicit shared apex and two typed embeddings. Merge
forms the disjoint union and identifies only the corresponding images of apex
symbols at their declared indices. It transports every signed axiom, caveat,
provenance record, complement and loss record. Opposing claims remain distinct,
including when they share a standpoint. A context mismatch cannot be repaired
by silently declaring contexts equal.

The result carries both injections and evidence that the square commutes. Given
a compatible pair of presentation maps into another admitted presentation, its
factorization must agree on the shared apex and be unique on the transported
generators. This is a checkable universal property in the declared presentation
category; it does not claim decidable equivalence or complete inference for
arbitrary first-order theories. Those reasoning boundaries stay explicit in the
selected contract.

The native presentation API makes this category explicit. An object carries a
finite typed signature and **closed canonical `Formula` sentences**, each with
an axiom-or-caveat kind, an independent positive-or-negative support sign, and
an immutable evidence publication. Generator positions belong to that exact
presentation; source names are co-equal historical metadata. Equal names alone
never identify two generators. Each context fixes world, standpoint, time, path,
module and modality; declared empty contexts also survive transport. The entire
selected reasoning contract is retained unchanged.

A presentation map is total on generators, preserves their exact roles and
context, retains every source name, and sends every decorated signed sentence to
a target sentence modulo the shared IR's alpha and connective normalization.
It does not discharge a sentence by entailment. Evidence identity is the exact
immutable native publication, including original provenance, complement and
loss information; separate evidence records cannot be replaced by equal labels
or equal serialized bytes. Historical evidence is interpreted together with the
explicit symbol bindings, rather than rewritten as if it originated downstream.
General maps may identify generators, while the merge span and injections must
be injective. These choices define the executable presentation category rather
than implying decidability for arbitrary theories or every Common Logic surface.

`CheckedPushout` independently checks the supplied square: the injections must
cover every output generator, intersect exactly on the apex images, preserve
exactly the union of source names and contexts, and carry exactly the transported
signed sentences and evidence. This excludes commuting quotients with extra
identifications and commuting extensions with extra axioms. For any admitted
cocone agreeing on the apex, the induced map is checked on both injections;
their joint coverage proves uniqueness on every output generator. This argument
is structural, independent of the size or success of a test corpus.

Sentence analysis is shared with its native formula body, and transport updates
only symbol bindings. Canonical comparison uses the existing IR normalization
under those bindings without constructing a translated formula or reparsing RDF.
Provenance and complements retain their native dataset handles. Deterministic
admission limits refuse excess work rather than truncating a result. The checked
square is an invocation-local witness tied to the exact immutable publications
and native engine descriptor; it is not a serialized rewrite certificate or a
claim that the selected reasoning contract has been executed. Canonical-source
admission and production scheduling must establish their own corresponding
execution judgments before consuming this operation as an optimization.

### Authored finite presentation merges

The source-aware compiler retains the selected `logic:presentationFormula` roots
with their exact source bindings. They remain owned sentences, including when
their support is negative; they do not become global axioms. Native source merge
execution consumes this same immutable compilation and its original document
occurrences. It neither reparses the source nor reconstructs those formulas.

The compiler stores the complete presentation declaration catalog in typed IR:
named contracts, contexts, signatures, signed sentences, evidence, presentations,
maps and selected merges. Standalone declarations remain in this catalog even
when no merge is selected. Formula bodies are shared with their original compiler
owners. Structural declaration fields do not also enter the flat axiom list.
A malformed declaration is an explicit refused IR value, including its original
source roots; caching or exporting it cannot turn refusal into an empty success.

Canonical RDF 1.2, CLIF, CGIF and XCL retain these declarations and their original
named references. The Common Logic metadata writers consume native RDF directly;
XCL serializes its required terminal N-Triples payload once. These projections
carry declarations, not native execution witnesses. OWL, gUFO, Datalog and N3
disclose the presentation and merge operations they cannot express.

Historical blank-node names and evidence origins require both their native label
and source scope. Terminal RDF carries these values in a `ScopedBlankReference`
with `sourceBlankLabel` and `sourceBlankScope`; source lowering reconstructs the
same native resource value. A reference is metadata, so relabeling its anonymous
transport node does not change the historical identity it describes. Direct
native blank resources remain valid authored input. Neither spelling establishes
provenance authority or equates generators: execution still binds the exact
immutable source publication independently. Anonymous transport bindings use
fresh labels disjoint from the emitted dataset and cannot collide with authored
record IRIs.

The source grammar uses named records in the default source graph. A declared
`logic:PresentationMerge`, or a member required by `logic:hasPresentationMerge`,
selects an operation. Missing or mistyped records, ambiguous singleton fields,
and presentation declarations in another graph are explicit failures. Graph
placement is not interpreted as a world or standpoint. Evidence may contain
arbitrary native named graphs and RDF 1.2 metadata, retained in its source owner.

| Record | Required content |
|---|---|
| `PresentationMerge` | `mergeLeft` and `mergeRight`, naming the two embedding maps |
| `PresentationMap` | `mapSource`, `mapTarget`, and one `generatorBinding` per source generator, with `bindingSource` and `bindingTarget` |
| `FinitePresentation` | `presentationContract`, plus explicit `presentationContext`, `presentationSymbol` and `presentationSentence` memberships; any membership set may be empty |
| `PresentationContext` | Named `presentationWorld` and explicit string `presentationModality`; named standpoint, time, path and module coordinates use their corresponding `presentation*` properties when specified |
| `PresentationSymbol` | `symbolContext`, native resource `symbolName` values, and its complete roles: true individual/variadic selectors or fixed relation/function arities |
| `PresentationSentence` | `presentationFormula`, `sentenceContext`, `sentenceSign`, `sentenceKind`, `sentenceEvidence`, and total `sentenceBinding` records with `bindingName` and `bindingSymbol` |
| `PresentationEvidence` | Native `evidenceOrigin`, explicit co-holding `preservationKind` values, and every `unsupportedConstruct` on its existing string-literal surface |

`PositiveSupport` and `NegativeSupport` are independent of `PresentationAxiom`
and `PresentationCaveat`. A source binding translates named Formula symbols,
never literal datatypes or lexical evidence. Fixed relation arities are
nonnegative, fixed function arities are positive, and a nullary function uses
the individual role. A selected boolean role must be true. Temporal coordinates
name resources whose descriptions remain in the original source publication.

The finite presentation index is explicit: no scope is inherited from a map,
binding, sentence or enclosing formula. Additional execution annotations
(`logic:standpoint`, `accordingTo`, `world`, `time`, `path`, `modality`,
`confidence`, `inModule`, `imports`, or `gmeow:accordingTo`) on these selected
records require a contextual lowering and are refused by this admission boundary.
That rule covers nested formula and term-carrier syntax through the shared source
ownership graph. Sorted variables similarly require a sort-preserving lowering;
retaining their original RDF is not a proof that an unsorted sentence preserves
their meaning. Modal source constructors require a lowering bound to the selected
presentation world; a default-world standard translation does not satisfy it.
These refusals do not apply to evidence metadata, whose scope is retained as
evidence rather than interpreted as the sentence's execution index. Named graphs
outside the selected source grammar are never silently unioned into that index.
Scope attached through RDF 1.2 reification of an asserted syntax statement is
also part of admission, including metadata on metadata. A provenance-only
reifier remains evidence; a standpoint annotation on the selected statement
cannot escape admission merely by using a reifier. Unasserted quotations and
reification in another graph do not import scope into the selected source graph.

The same named evidence record shares one immutable native publication across
all selected presentations. Distinct evidence records stay distinct even if
their origin labels agree. Both provenance and complement retain the complete
compiler source owner, including its original-to-canonical term correspondence;
they are not fabricated by projecting the sentence's visible atoms. The selected
contract must have a successful original compiler owner without unresolved
extraction diagnostics. Registry and syntax limits refuse excess work without
publishing a partial merge set.

The production compile stage executes selected source merges and emits
`generated/logic/presentation-merges.json`. This terminal report stores each
original formula and evidence definition once, explicit injection maps, every
result context and generator, and each transported signed sentence's bindings.
Its source document receipts travel with the producer's authenticated source
dependency. The report cannot be hydrated into an optimization certificate:
native witnesses remain tied to the exact source publication and engine, and
any downstream optimization still requires its complete execution judgment.

## A unifying lattice-graded reading

*Design note only — names a vocabulary and a direction; builds nothing here.* Three structures
already present in the calculus, and one from a sibling slice, are coordinates of a single
**lattice-graded correspondence** shape, not three unrelated mechanisms:

- the ordered preservation **law-spine** above (§ The ordered law-spine) — a poset a correspondence's
  claimable laws sit on, weakened monotonically by composition;
- the **confidence** (and sibling quantitative) axis (§ The quantitative and contextual axes) —
  computed in its own algebra under composition (t-norm/min/solver-additive, § Composition and
  merge);
- the `lang:` GMN dialect's **security-ring flow lattice** (`gmeow:GmnSecurityRing`, a separate
  slice — named here by reference, not imported or restructured: see `slices/grounding/lang/`) —
  a level×compartment product order the GMN serialization boundary respects.

Each axis is already computed in its own algebra — a semiring, a t-norm, or a `min` — and each
already weakens monotonically down a poset as correspondences compose (the law-spine itself is that
poset for the class axis; § Composition and merge is the general mechanism). Denning's
information-flow lattice `⟨SC, ⊑, ⊕, ⊗⟩` (security classes, dominance order, join, meet) is
recognizable as the **serialization-boundary instance** of this same shape: a lattice-graded
quantity, monotone under composition, attached to a crossing.

**Forward direction (named, not built here).** The unifying reading points toward two further
extensions of the calculus:

1. a `logic:` **flow-label axis** on the correspondence, alongside `confidence`/`evidenceStrength`/
   `weight`/`probability`/`Determinacy`, so a crossing's information-flow classification composes by
   the same rules as its other quantitative axes;
2. a **parametric round-trip harness** generalizing the calculus's existing byte-teeth gates — the
   narrow-waist superset gate, the RDFC-1.0 round-trip, and the GMN-1 round-trip gate are three
   instances of one round-trip-over-a-crossing shape, and a single parametric harness could discharge
   all three from one implementation.

Naming this direction is the entire scope of this note. The engine itself — the flow-label
vocabulary, its RDF terms, and the parametric harness — is **not** implemented here: `logic:` is
canon, and expanding it with a new foundational axis or a generalized harness is a design decision
that belongs to its own dedicated treatment, not a side effect of naming the direction.

## Preservation is the lens-law framework (reuse, do not reinvent)

The existing preservation machinery *is* the lens-law / abstract-interpretation framework in entailment
dress, and the calculus reuses it verbatim:

| Lens / abstract-interpretation concept | Existing `logic:` machinery |
|---|---|
| `get∘put = id` on the preserved fragment | `ExactPreservation` + the round-trip faithfulness gate |
| under-approximation `α(γ(a)) ⊑ a` | `logic:SoundUnderApproximation` |
| over-approximation `c ≤ γ(α(c))` | `logic:CompleteOverApproximation` |
| law claimed but not machine-verified | `logic:LawClaim` + the `logic:DischargeVerdict` / `logic:DischargeCondition` vocabulary |
| polarities co-holding | "preservation polarities are not mutually exclusive" |
| round-trip is a decidable check | content-addressed canonical-IR identity (graph-iso) |

`logic:LawClaim` reuses these exact individuals via `logic:lawDischargeVerdict` /
`logic:lawDischargeCondition`. The **overclaim gate** fires for alignment: marking a caveated overlap as
`sssom exactMatch`, or a bridge view as `logic:InstitutionMorphism`, is a build failure — strictly
stronger than the old `projection_lint` warning. The three former cross-layer
invariants collapse into this: `fno-type` becomes the FnO back-end's soundness check; `spec-drift`
*disappears* because EDOAL and SPARQL now lower from the same `get` leg.

## The lowerings (target dialects)

Each former artifact is a registered lowering with its own preservation claim, in the *same*
`generated/logic/projection-report.ttl` loss ledger that governs OWL/Datalog/gUFO:

| Target | Lowers from | Typical preservation |
|---|---|---|
| SSSOM | the meta-formula's 1:1 lattice band | exact for `equiv`; else under-approx; drops caveat/laws/legs |
| EDOAL | the `get` leg + relation lattice + measure | under-approx; drops SOL caveats, the `put` leg, world/standpoint scope |
| FnO | transform functions referenced by `get` | exact for signatures; validation-only for entailment |
| SPARQL CONSTRUCT | `get` compiled to the closed algebra | the faithful executable down-projection; profile losses explicit |
| up-lift (replaces the heuristic) | the `put` leg — *derived* for mnemomorphic cells | complete-over for invertible; validation-only (mint-with-claim) otherwise; `unsupported` where `get` is non-injective and no witness exists |
| OWL alignment axioms | the relation, DL-expressible band | under-approx; `unsupported` for caveated overlaps and bridges |
| OAEI / Alignment-API XML | the whole correspondence set | under-approx; carries `align:measure` where SSSOM/OWL drop confidence |
| GMN (`gmeow:gmnModelNotation`, the `lang:` dialect charter) | the GMN-0 narrow-waist normal form | section/retraction, exact preservation, `mnemomorphic true`: aliases invert through the version-pinned codebook bijection (injectivity-gated), confidence and annotations ride by reference (never inlined, never lost); discharged by the executed GMN-1 round-trip gate, total over the grounding slices' GMN-0 now and gated toward full coverage by the GMN-1-coverage quality axis elsewhere; the rate–fidelity contract rides the codebook |

Every lowering is a **legalization** (see [`LOGIC-IR.md`](LOGIC-IR.md) § IR commitments): a total
function into `⟨ legal output ⊕ flagged residue ⟩`; the loss ledger is the residue set.

## The IR commitments

Three commitments are recorded in [`LOGIC-IR.md`](LOGIC-IR.md) and are load-bearing for this calculus:

1. **Lowering is legalization** (`logic:ConversionTarget`) — the target names the legal IR or
   dialect, and partial conversion leaves an illegal construct in place, flagged: the "unsupported
   carried and flagged, never dropped" rule. This is distinct from `logic:ProjectionTarget`, which
   requests an output presentation without defining the legality domain or changing entailments.
2. **Every annotation is typed `logic:loadBearing` or droppable** — a display hint is droppable; the
   in-band complement and the axes are load-bearing (`put` needs them for `u∘d = id`). *Without this
   bit the section/retraction rung cannot be verified*, so it is in the node type from the start.
3. **The `logic:RelationalCore` dialect** — the logical↔physical lowering waist between `logic:` and the
   native execution engine; every execution strategy targets it.

## Execution

The correspondence calculus rides the native execution engine of
[`LOGIC-RUNTIME.md`](LOGIC-RUNTIME.md): a `get` leg is a join plan; a `put`/chase is semi-naive fixpoint
with existentials; a lens-law check is run-get-then-put + content-hash compare. The quantitative axes
are **semiring annotations** computed in one evaluation pass, not N passes over a context cross-product
— the semiring *is* the axis algebra (§ Composition). The `ReasoningContract`
([`LOGIC-CONTRACT.md`](LOGIC-CONTRACT.md)) selects both correctness and the physical plan.

Static legs and shared type, effect, termination, signature and closure analyses
are prepared once for their exact identities. Native datasets and typed results
cross execution boundaries directly. Text serialization belongs at selected
input/output boundaries. Bounded caches may reuse exact immutable inputs;
statistics fingerprints may rank plans but cannot identify cached answers.

Identity elimination, reassociation, common-subexpression reuse, pushdown,
cancellation and fusion each require a checked fragment/effect/contract
certificate. Physical fusion preserves every logical stage contract, required
check, provenance edge, complement and loss obligation. Template-created blank
nodes retain their allocation scope across transformations. If bounded optimizer
search yields no certified improvement, execution uses the original admitted
program. Search limits cannot suppress an output or waive an obligation.

## Conformance

A new **Correspondence** conformance category ([`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md))
generalizes the Common-Logic round-trip gate, with five decisive gates: the **Law gate** (a
correspondence may not claim a law it fails), the **Overclaim gate** (a bridge view cannot emit
equivalence; a claimed rung must be satisfiable by the lowered legs), the **Round-trip gate** (iso and
section/retraction execute complete recovery cases and reproduce the declared source atom set), the
**Mnemomorphism gate** (a recoverability claim must actually recover the source), and the
**Composition gate** (composing may only preserve or weaken claims).

## OpenEHR — the worked subsumption (six layers)

openEHR is the worked instance. It is a six-layer standard, and GMEOW subsumes each layer with the same
projection doctrine — the **data axis** (`DV_QUANTITY` ↔ frame-relative quantity, reaching
section/retraction via an in-band complement) and the **process axis** (openEHR PROC / Task-Planning ↔
`logic:Plan`, a lossy lens for execution). The process axis joins this calculus to the canonical
process model: openEHR Task Planning is one more by-reference projection
target of `logic:Plan`, and the correspondence calculus is its projection mechanism. The YAMATO
refinements that ground both axes (persistent `Quality`; action/event open-closed; causal-vs-temporal
parts) are adopted by-reference (Principle 5; see
[`foundational-bridging.md`](../../../../docs/foundational-bridging.md)). Worked end-to-end against real
GECCO data in [`usecase_openehr_bloodpressure.md`](../../../../docs/APPLIED_CATEGORY_THEORY/usecase_openehr_bloodpressure.md)
and [`usecase_openehr_taskplan_rchops21.md`](../../../../docs/APPLIED_CATEGORY_THEORY/usecase_openehr_taskplan_rchops21.md).

These are target capabilities, not an unrestricted implementation certificate.
The blood-pressure fixtures establish bounded reconstruction and query-class
recovery; their external validator observation is specific to the committed
compositions, template and validator version. The process fixture establishes
authored plan structure and selected native capabilities. Full adoption requires
native typed execution of both cases and independent GetPut, PutGet, PutPut and
SectionLaw domains. An ISM plan/schema link is a recovery witness only when its
complete referenced content is available under an authenticated identity; missing
or corrupt selected witnesses fail admission. Fixture agreement, by-reference
mapping rows and interval-lowering tests do not establish general store
replacement or authorize unrestricted composition/fusion.

## Constitutional alignment

One canonical source; every surface a generated projection carrying an honest preservation judgment
(Principle 4). Maximal bridging by reference, never `owl:sameAs` collapse (Principle 5). The logic —
now including alignment — is canonical; SSSOM/EDOAL/FnO/SPARQL/OWL-alignment are lossy projections
(Principle 17). The correspondence calculus is the third consolidation under this doctrine, peer to the
process model and the typed compositional meta-semantics, all sharing the native execution engine.
