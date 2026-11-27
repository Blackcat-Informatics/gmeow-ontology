<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Logic — the `logic:` reasoning layer

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/logic` · **tier: core**
> The maximally expressive, RDF 1.2-native logic in which GMEOW's model is authored, and of which
> OWL, RDFS, SHACL, Datalog, Prolog, N3, SPARQL, and gUFO are typed, lossy projections. BFO,
> DUL, OBO/RO, SUMO, YAMATO, and OpenCyc are explicit commitment-shifting bridge views —
> Constitution **Principle 17**.

This slice is the home of **GMEOW Logic (`logic:`)**, the canonical reasoning language for GMEOW.
The `logic:` namespace is the authoritative ground; the full UFO⁺ foundational vocabulary is
minted here. External formalisms are projections or typed bridge views of that ground, never a
second source of truth.

## Complete reasoning artifacts

`logic:resultNativeExecution` carries a canonical `blake3:` address as `xsd:string`.
The required binary GTS blob uses outer schema `gmeow-native-receipt-v1`, representation
`native-execution-receipt` and media type `application/cbor`. Its ordered record contains
`native_schema`, `execution`, `assessment` and `closure`: the explicitly selected
`gmeow-native-execution-v4` evidence, the constructor-owned assessment commitment, and
the exact optional closure address. The assessment hashes the projection-stable axes,
proposition scope, provenance, budget, contradiction witnesses and declared row schema
under the `gmeow-native-assessment/v1` domain followed by a zero byte. It does not hash
the execution DAG again or certify rich payload fields omitted from the RDF summary.
Readers authenticate the address, each physical payload occurrence, and its exact owning
aggregate before exposing a verdict. A standalone RDF summary cannot authenticate that
receipt. Strict field order, canonical replay and the compact 256 MiB, expanded 512 MiB,
and depth 64 admission bounds cover the complete selected receipt; these are encoding
limits, not host memory measurements. Bare execution envelopes are not receipt bindings.

Operations publishing a complete closure additionally bind `logic:resultClosureBlob`
before computing the aggregate identity. The native receipt selects either no closure
or that exact address. An added, omitted or substituted RDF closure link fails under the
retained receipt, including substitution with an independently valid leaf sharing the
same world catalog. Reusing a receipt preserves its selected link; materializing a full
artifact requires the selected leaf bytes. Its `reasoned-closure` blob is an isolated,
single-segment native GTS leaf containing the asserted and inferred RDF from that same
engine run. Its authenticated world catalog restores empty named graphs. Imported blank
identities use the native transport namespace while original typed identities remain in
the execution receipt. Caller graphs named `graph/reasoning` and source statements that
resemble result metadata remain ordinary closure data. A closure leaf contains no nested
application blobs or external blob references; its decoded limit is 1 GiB.

Same-run RDF closure equivalence uses PurRDF's complete dataset-state digest, including
empty declarations and one global blank bijection. Its equality relies on BLAKE3
collision resistance, and the forward-mapped graph catalog is checked exactly. This
comparison accepts arbitrary source IRIs. Native execution evidence and the actual
transport summary retain separate exact typed comparisons. Publication temporarily
retains the original/imported datasets and native canonical-state workspace; item
budgets exclude those allocations and do not establish a host RSS limit.

Before emission, typed inferred-object summary values are normalized into the selected
native snapshot namespace. This includes blanks inside quoted triples and composite
literals. The standalone runtime prefix and the pipeline carrier namespace are explicit
selections; emitted-byte readback must equal that expected summary. The original native
execution evidence is compared exactly and retains its original identities.

Only the semantic native constructor grants original publication authority over its
immutable inferred rows and exact execution owner. Summary normalization relinquishes
that authority even when every row remains unchanged. Generic serialization carries the
eight semantic result fields without runtime authority. Checked admission against the
retained receipt restores reuse through shared cache handles; it cannot mint a new
original receipt or certify a summary as the complete closure. Constructor-owned partial
conflicts retain their witnesses and information state even when sibling work is exhausted.

The typed result remains authoritative for binding rows, marginals, rich inference fields
and complete native evidence. The transport summary preserves its governed axes and
provenance, and the separate closure preserves RDF. Neither projection invents the fields
the other omits. All payload-bearing frames use the mandated rsyncable level 12 profile.

The vocabulary covers the UFO⁺ foundational sorts
(`Kind`/`SubKind`/`Phase`/`Role`/`Category`/`Mixin`/`RoleMixin`/`PhaseMixin`/`Relator`/`Event`/`Situation`
and the wider superset spine), the foundation relations
(`rigidlyAppliesTo`/`suppliesIdentity`/`mediates`), the world and modal terms
(`World`/`accessibleFrom`/`counterfactualOf` and the typed context algebra), the four normatively
distinct quantitative axes (`probability`/`confidence`/`weight`/`evidenceStrength`), the
probabilistic model vocabulary (`ProbabilityModel`/`FullIndependence`/`DependencyModel` and their
joint-outcome terms), the semantic profiles (as named presets of the reasoning contract), the
OntoUML discipline diagnostics (`violation`/`rigidityViolation`/`dischargeObligation`/`witnessRequiredViolation`),
the preservation-polarity vocabulary (`PreservationKind`/`preservationKind`/`complexityClass` and
their named individuals), and the rule-body structural properties (`negatedBody`/`distinctBody`).
All terms are declared as **standalone terms** — they carry no `gmeow:` parentage, because `gmeow:`
is a generated lossy projection of `logic:`, not its ground.

## The design set

The design is split by genre into the 21 documents under [`design/`](./design/), so it can be
implemented against rather than only read. Every document in that directory is owned by this slice and
appears in the table below. Charters outside the directory are linked peers or consumers, not members
of the set:

- [`docs/GROUNDING.md`](../../../docs/GROUNDING.md) binds this slice to its co-foundational peers.
- [`docs/APPLIED_CATEGORY_THEORY/`](../../../docs/APPLIED_CATEGORY_THEORY/) holds the cited rationale and
  the openEHR worked cases for the correspondence calculus; the calculus document governs where they differ.
- [`WORK-ORCHESTRATION.md`](../../core/work-orchestration/design/WORK-ORCHESTRATION.md) is the domain
  binding of the enactment kernel, owned by the work-orchestration slice. Every `gmeow:`-typed edge lives
  there, because `logic:` takes no axiom edge into `gmeow:`.

The evidence column classifies what backs each document's promises, never the document as a whole:

- **design-only commitment** — the contract is stated; no source or test executes it.
- **source inspection** — an implementation exists in the named source; no test or corpus case executes
  the whole contract.
- **exercised bounded case** — tests, conformance cases or committed fixtures execute the contract over a
  named finite domain. Agreement on that domain is not a law over every input.
- **complete proof** — an exhaustive check or checked derivation covers the whole declared domain.

A document that mixes classes states each boundary in [Evidence boundaries](#evidence-boundaries).

| Document | Genre | Evidence | Contents |
| --- | --- | --- | --- |
| [`design/LOGIC.md`](./design/LOGIC.md) | manifesto | design-only commitment; the claims it makes are evidenced by the documents it indexes | vision, doctrine, lineage, the document index |
| [`design/LOGIC-FOUNDATION.md`](./design/LOGIC-FOUNDATION.md) | charter | exercised bounded case (`conformance/logic/cases/foundation`); a few named engine-selection strategies are not engine-backed | the `gmeow:logic` upper-ontology charter — the gUFO ⊇ baseline, the criticism ledger, the greenfield feature map, the Ithkuil precision ethos, the descriptive graph-box roles |
| [`design/LOGIC-CONTRACT.md`](./design/LOGIC-CONTRACT.md) | configuration | complete proof over the rule-participating facet cross-product (the exhaustive oracle sweep and rule-coverage guard in `crates/logic-compile/src/compat.tests.rs`); a facet no compatibility rule reads is not swept | the reasoning contract — the orthogonal facets a reasoning request selects; named profiles as presets; the compatibility matrix |
| [`design/LOGIC-IR.md`](./design/LOGIC-IR.md) | intermediate representation | exercised bounded case (`crates/logic-compile`) | the typed, full first-order IR every source compiles into and every projection out of; the per-lowering preservation judgment |
| [`design/LOGIC-CORRESPONDENCE.md`](./design/LOGIC-CORRESPONDENCE.md) | correspondence calculus | exercised bounded case for the atomic lens, the finite presentation merge and the quantitative composition axes; design-only commitment for the remaining composition axes and typed openEHR execution — see [Correspondence calculus](#correspondence-calculus) | cross-ontology alignment as the ninth IR node kind: the ordered law-spine, mnemomorphism and recovery cases, the four independent law domains, the separated axes, composition and finite merge, the generated lowerings, the openEHR worked subsumption |
| [`design/LOGIC-SEMANTICS.md`](./design/LOGIC-SEMANTICS.md) | formal semantics | exercised bounded case (the conformance corpus under `conformance/logic/cases`) | the unified core, triple-term/assertion rules, the reasoning result, modality, the typed context algebra, decidability |
| [`design/LOGIC-AGENTIC-MODALITY.md`](./design/LOGIC-AGENTIC-MODALITY.md) | modal composition | exercised bounded case for the finite ground modal fragment and the finite temporal interpretation (`crates/logic/src/modal`); design-only commitment for norms, intentions, protocols and scheduler adoption — see [Agentic modality](#agentic-modality) | the attributed evaluation context, the finite ground modal fragment over typed accessibility, the information coordinates, `gmeow logic evaluate`, the finite journal temporal operators, and the norm, intention and protocol commitments |
| [`design/LOGIC-TRANSACTION.md`](./design/LOGIC-TRANSACTION.md) | state change | exercised bounded case (`crates/logic/src/transaction.rs`, `conformance/logic/cases/transaction`), including `logic:SerializationAnomaly` non-serializable-schedule findings | Transaction Logic — path semantics, serial conjunction, updates as supersession, the state-change facet |
| [`design/LOGIC-TELEOLOGY.md`](./design/LOGIC-TELEOLOGY.md) | goal/action layer | exercised bounded case (`crates/logic/src/teleology.rs`, `conformance/logic/cases/teleology`); the goal kinds are evaluated by this module's own interpreters, not the finite-journal temporal evaluator | goals, intentional modes, structured goal expressions, reified goal evaluation, action schemas, goal decomposition and conflict; the intention → plan → action → transaction-path chain |
| [`design/LOGIC-ENACTMENT.md`](./design/LOGIC-ENACTMENT.md) | charter | exercised bounded case for the integrity laws, the observed-not-derived guard and means–end refinement; design-only commitment for transition-law execution, liveness certificates, knowledge-order monotonicity and refinement depth — see [Enactment](#enactment) | the general prescription → enactment → commitment kernel over the existing `logic:Plan`: immutable versioned prescriptions, immutable journalled enactments, checkpoints and continuation kinds, the seven orthogonal lifecycle axes with their transition and liveness laws, the ⊑ₖ knowledge order over external-effect commitment, hierarchical refinement methods and the declared decidable search fragment, and the reuse ledger |
| [`WORK-ORCHESTRATION.md`](../../core/work-orchestration/design/WORK-ORCHESTRATION.md) | binding charter (core slice, not this one) | exercised bounded case (that slice's `tests` and `conformance/logic/cases/enactment`); governed by the same enactment boundary | the domain binding of the kernel above — durable work clusters: the continuing-cluster thesis, guidance versioning, context assembly, recurrence non-identity, the decision-support bindings, and the review projection with its Principle-17 preservation judgment |
| [`design/LOGIC-SESSION.md`](./design/LOGIC-SESSION.md) | operational façade | exercised bounded case (`crates/logic/tests/reasoning_session_*.rs`) — see [Session checkpoints](#session-checkpoints) | the stateful `ReasoningSession`: the seven-axis identity, the two delta anchors, the hash-linked journal, content-addressed checkpoints with replay restore, the six-way operation outcome, paged world-source composition, semver governance |
| [`design/LOGIC-COGNITION.md`](./design/LOGIC-COGNITION.md) | cognitive assessment | exercised bounded case (`conformance/logic/cases/cognition`) | the multidimensional cognitive-assessment construct — factored dimensions of reasoning quality, reliability, calibration, and metacognitive posture; reasoning quality over the inference modes; reliability over the typed reasoning result |
| [`design/LOGIC-RUNTIME.md`](./design/LOGIC-RUNTIME.md) | runtime | exercised bounded case (`crates/logic/src/physical`, `crates/logic/src/runtime`) | solver architecture, the materialization–resolution seam, graph versioning, generated artifacts |
| [`design/LOGIC-PERFORMANCE.md`](./design/LOGIC-PERFORMANCE.md) | performance doctrine | source inspection; the document's perf ledger records each non-incremental, refused or demoted fragment, and wall-clock timings are advisory, never a gate | the deterministic-performance contract, data-shape and join doctrines, demand transformation, the incremental algebra, the chase-termination ladder, deterministic parallelism, provenance cost bounds, the measurement regime |
| [`design/LOGIC-CONFORMANCE.md`](./design/LOGIC-CONFORMANCE.md) | contract | exercised bounded case (`conformance/logic/cases/*`, `generated/logic/projection-report.ttl`) | the conformance corpus and the loss-ledger preservation contract |
| [`design/LOGIC-VALIDATION.md`](./design/LOGIC-VALIDATION.md) | shape surface | exercised bounded case (`crates/logic-compile/src/projections/shapes.rs` with its SHACL and ShEx tests, `conformance/logic/cases/validation`) | closed-world data-shape validation authored as canonical `logic:` validation shapes and projected to SHACL Core and ShEx, never hand-authored on the surface; the integrity peer of the SHACL-AF derivation projection |
| [`design/LOGIC-SHACL-AF.md`](./design/LOGIC-SHACL-AF.md) | computation surface | exercised bounded case (`crates/logic-compile/src/projections/shacl_af.rs`) | derivation/aggregation (map/reduce) authored as canonical `logic:` rules and projected to a SHACL-AF `sh:SPARQLRule` surface — computation added to the canon and emitted, never bolted onto SHACL; the projectable fragment, the declared exit loss, and the placement/purity rule |
| [`design/LOGIC-PATHS.md`](./design/LOGIC-PATHS.md) | traversal | exercised bounded case (`crates/logic-compile/src/projections/paths.rs`, [`examples/predicate-paths.ttl`](./examples/predicate-paths.ttl)); the predicate wildcard is emit-only because no query surface parses it | named and parametric predicate paths — `logic:PathShape`, the predicate wildcard and bounded `{n,m}` depth that SPARQL §9 lacks, by-name parametric invocation, and the property-path and Datalog projections with their declared exit loss; the pattern substrate of the correspondence `get` and `put` legs |
| [`design/LOGIC-RDFQUERY.md`](./design/LOGIC-RDFQUERY.md) | query surface | design-only commitment — no grammar, parser, vocabulary term or test exists; the language is not committed | RDFQuery framed as a front-end that parses into `logic:` (which already projects out to SPARQL/SHACL/N3/OWL), not a stack bolted onto SPARQL; gated on its named first consumer (Principle 15) |
| [`design/LOGIC-META-SEMANTICS.md`](./design/LOGIC-META-SEMANTICS.md) | meta-semantics | design-only commitment for the thesis; each axis it catalogs is evidenced by the document named in its row | the projection doctrine turned inward: the orthogonal canonical axes against the simplified surfaces generated from them, and the outward lattice of prior formalisms |
| [`design/LOGIC-REFERENCES.md`](./design/LOGIC-REFERENCES.md) | appendix | source inspection (`metadata/references.ttl`) | external standards, theory, and engines cited — staged for the `metadata/references.ttl` ledger |

## Evidence boundaries

Each statement is a fact about what executes. A promise whose evidence is weaker than its prose is
stated here with its boundary, and the document that owns the contract is linked rather than restated.

### Correspondence calculus

The calculus document is the canonical owner. Its consumers are the grounding bridge mappings
([`mappings/grounding-bridges.ttl`](./mappings/grounding-bridges.ttl)), the pipeline mapping and
compile-logic stages, and the `gmeow correspondence` commands.

- **Law domains.** GetPut, PutGet, PutPut and source recovery with the required complement are four laws
  on four input domains ([law domains](./design/LOGIC-CORRESPONDENCE.md#independent-law-domains-and-evidence)).
  The native atomic property lens executes all four: GetPut against a nonempty prior source, PutGet on
  independently edited views, PutPut on successive independent edits, and recovery from the augmented
  view with its residual complement under the `EmptyWithComplement` policy (exercised bounded case:
  `crates/logic/src/correspondence_exec/atomic_lens/tests.rs` and
  `atomic_lens/composition/tests.rs`). The production correspondence program discharges only bounded
  SectionLaw and PutGet evidence over declared seeds
  (`crates/pipeline/src/correspondence_law.rs`), and atomic-path recovery runs against an explicit empty
  initial state. PutPut is discharged for no correspondence. The `lang:` bridge declares GetPut discharged
  for its mnemomorphic (exact) rungs under a syntactic-reachability condition
  (`crates/lang-bridge/src/registry.rs`, `plain_text.rs`, `grammar.rs`, `conllu.rs`), and carries it as
  `ObligationUnknown` for its lossy-lens fallbacks; the mapping program does not execute GetPut. A composite
  path without an authored complete recovery case stays `ObligationUnknown`.
- **Composition.** The four quantitative axes execute as authored `logic:Formula` rules on the native
  engine (`crates/logic/src/correspondence_exec/axes.rs`). The Composition gate checks the rung join
  and the per-law premise bound (`crates/logic-compile/src/projections/correspondence_gates.rs`) and
  certifies no optic type. No rule executes determinacy, preservation polarity, loss and
  unsupported-construct composition, or effect and complement composition; the calculus states them as
  composition-rule obligations, and they are design-only commitments.
- **Finite presentation merge.** `CheckedPushout`
  (`crates/logic/src/correspondence_exec/presentation/pushout.rs`, exercised by
  `presentation/tests.rs`) checks the supplied square, its injections and the factorization over finite
  standpoint-indexed presentations, and the compile stage emits `generated/logic/presentation-merges.json`
  from authored merge declarations. The result is an exercised bounded case in the declared presentation
  category. It is not a rewrite certificate, claims nothing for pushouts of arbitrary theories, and is independent
  of the openEHR evidence: no openEHR fixture executes a merge, and no merge test reads one.
- **openEHR.** The openEHR evidence has separate bounds. The conformance case
  `conformance/logic/cases/correspondence/openehr-bloodpressure-section-retraction` recovers the three
  declared source edges of one query class. The reconstruction test
  `crates/logic-compile/tests/openehr_bloodpressure_roundtrip.rs` rebuilds the committed fixture source
  from the RM slice and its complement with a dedicated JSON reader; it is SectionLaw evidence with a
  source-replacing initial state and executes no typed source/view/complement correspondence. The OPT
  reader is exercised on the vendored `Blutdruck.opt` and `TestAllDatatypes.opt`
  (`crates/logic-compile/src/openehr_opt.walker_tests.rs`), and the half-open interval projection is
  exercised on a constructed interval (`crates/logic-compile/src/opt_lift.tests.rs`). The external
  validator observation is a standalone lane the gate does not run, and holds for the two committed
  compositions, the template and the validator version. The `rchops21` process fixture is projected and
  recovered by the compile stage under an explicit loop bound, and `gmeow correspondence project-plan`
  and `recover-plan` expose the same paths over caller-owned RDF. Typed execution of both cases with
  edited views, nonempty prior state and independent GetPut, PutGet, PutPut and SectionLaw domains is not
  implemented, and no fixture authorizes store replacement or unrestricted composition or fusion.

### Enactment

The enactment charter is normative, and GMEOW describes enactments without performing them. Three
portions execute:

- **Integrity laws.** The authored enactment constraints compile to violation-emitting rules and run on
  the production verification path (`crates/logic/src/reason/enactment.rs`,
  `crates/logic/tests/enactment_gate.rs`, `conformance/logic/cases/enactment`). Each finding names the
  law it breaks.
- **Observed-not-derived guard.** A derivation that types a subject as an effect attempt or receipt, or
  stamps derivation provenance on one, is a hard failure (`reject_banned_heads` in the same module).
- **Means–end refinement.** Eleven authored rules run under a budgeted session and return the typed
  operation outcome (`crates/logic/src/reason/enactment/refine.rs`). The method-cycle refusal under the
  acyclic fragment, ordered-carrier validation, the four rejection kinds, pins and budget-cut
  incompleteness are exercised in `refine.tests.rs`.

Four portions are design-only commitments. The transition formulas of the seven axes are not encoded:
[`examples/axis-transition-laws.ttl`](./examples/axis-transition-laws.ttl) names the positions of axes 1–4
and 7 without the admitted moves; axis 5 is a monotonicity law and axis 6 a minting-and-supersession law,
and neither is encoded there. No liveness certificate is computed, so deadlock-freedom,
reachability and no-orphan-intent are vocabulary. No constraint enforces the monotonicity of
`logic:effectKnowledge` under the ⊑ₖ order. Refinement does not read the declared maximum depth of
`logic:FragmentBoundedDepth`; the step budget bounds the search.

### Agentic modality

The evaluation context, the finite ground modal fragment over typed accessibility, the four information
states, `gmeow logic evaluate` and the finite temporal operators over a committed journal, including the
append monitor, are exercised in `crates/logic/src/modal` (contextual, composite, journal and monitor
tests). The norm, intention and protocol commitments have no vocabulary and no executor: step norms and
contrary-to-duty activation, intent composition and revision, choreography role projection, and
Hohfeldian powers are design-only commitments. The achievement, maintenance, avoidance and deadline goals
are evaluated by `crates/logic/src/teleology.rs` and do not use the finite temporal evaluator the
document requires of them. The production scheduler does not evaluate readiness through the modal
kernel.

### Session checkpoints

A checkpoint stores the seven-axis identity, the authorized EDB generation, the journal head and the
ordered committed deltas, and no circuit state. Restore re-materializes the base closure with `open`,
replays the stored deltas through the ordinary apply path and refuses unless every delta applies and the
replayed head equals the recorded head. The content address covers the identity's descriptor hash, the
generation, the head and the deltas. Head equality shows that the same sequence of accepted transitions
was reproduced and does not hash the closure; closure agreement after restore is exercised by
`ac5_committed_delta_is_refused_after_restart` in `crates/logic/tests/reasoning_session_crash_replay.rs`,
`crates/logic/tests/reasoning_session_restore_rejection.rs` exercises the corrupt-checkpoint refusals, the
identity-mismatch refusal on six of the seven identity axes (the engine-descriptor axis is fixed per build and
is folded only by an in-crate fold test), and the replay-head-divergence refusal. No test in that suite
exercises the refusal of a replayed delta whose outcome is not `Applied`, or the `EngineFailure` refusals of
re-materialization and of committed-delta reconstruction in `facade.rs`. The
[session document](./design/LOGIC-SESSION.md#content-addressed-checkpoints) owns the contract.

### Query surface

RDFQuery is a design-only commitment. The grammar is not committed and no consumer is wired. Until a
query surface parses it, the predicate wildcard of a named path is emit-only.

## What it commits to

- **The logic is canonical; OWL is a projection of it, not its ceiling.** `logic:` is RDF 1.2-native
  and Turing-complete by intent; decidability is a property of a *projection* or a declared
  contract facet, never a cap on what the canonical model may say.
- **A reasoning request is a typed contract over orthogonal facets.** Every reasoning surface
  carries an explicit `logic:ReasoningContract` — a selection of values for consequence relation,
  negation kind, closure assumption, context indexing, state-change mode, uncertainty handling, and
  so on. Named profiles are **presets**: named bundles of facet values that the compiler expands
  before evaluation. Presets are sugar over the canonical facet set; they are not indivisible
  alternatives, and combinations no preset anticipates are expressible directly. The full facet
  vocabulary, preset definitions, and compatibility matrix are in
  [`design/LOGIC-CONTRACT.md`](./design/LOGIC-CONTRACT.md).
- **One typed, full first-order IR.** Every `logic:` source compiles into a single typed
  intermediate representation covering the full first-order fragment and beyond. Every output —
  OWL, Datalog, N3, the Common Logic dialects, the canonical RDF 1.2 serialization — is a
  projection *of* the IR; every external dialect ingested is parsed *into* the same IR. There is
  no separate "Datalog-plus-extensions" form; that fragment is a subset reached by lowering.
  The IR, its node kinds, and the per-lowering preservation judgment are in
  [`design/LOGIC-IR.md`](./design/LOGIC-IR.md).
- **State change is an orthogonal facet, not a separate profile.** Transaction Logic (the Evolution
  = `transaction-path` facet) gives path semantics, serial conjunction, and updates as supersession
  rather than erasure. It composes with any choice of consequence relation — Horn, well-founded,
  stable-model, or probabilistic — because state change and entailment relation are independent
  concerns. The state-change semantics are in [`design/LOGIC-TRANSACTION.md`](./design/LOGIC-TRANSACTION.md).
- **The foundational ontology (UFO⁺) is authored in `logic:`.** gUFO is the primary generated
  down-projection; BFO, DOLCE, and SUMO are generated bridge views, not truth-preserving
  projections. The OntoUML disciplines are `logic:` rules the native solver evaluates; their
  equivalents survive as projection-conformance tests over the gUFO downcast.
- **A single canonical native solver is the normal development authority.** It runs forward
  materialization and backward goal resolution; the classical OWL tools operate as secondary
  validators for exported subsets, not as the authority.
- **Verified by construction.** One shared language-neutral conformance corpus, carried by the
  preservation judgments of every lowering and audited by the loss ledger (Principle 7).

## The reasoning contract

A reasoning request does not select a mode from a fixed list of profiles. It assembles a
`logic:ReasoningContract`: a selection of values across orthogonal, independently-varying facets.
The facets include the model semantics (least-model/Horn, well-founded, stable-model, …), the
negation operators (a *set* drawn from explicit/strong and default/negation-as-failure), the
truth/inconsistency semantics (classical, or a Belnap-family configuration — algebra plus
admissible-valuation policy plus designated set — yielding FDE, LP, K3, …), the closure assumption
(a *map*: open-world, or predicate-scoped closed-world), the context index (a *multi-dimensional*
index: unindexed, world-indexed, standpoint-indexed, time-indexed, path-indexed), the evolution
mode (static, state-transition, transaction-path), the uncertainty handling (a *set* of measures:
none, probabilistic, weighted, fuzzy), and others. There is no single "consequence" facet — what an
entailment means is settled jointly by several of these. Each facet is an open value vocabulary;
new values join without a schema change.

The historical profile names — positive Horn, stratified negation-as-failure, well-founded,
stable-model, procedural Prolog, probabilistic — survive as **presets**: named contracts with
a standard facet bundle that the compiler expands before evaluation. An author may reference a
preset or assemble a contract from facets directly; the canonical form the engine reasons over
is always the facet set.

Not every point in the facet product is implementable or coherent. The compiler holds an explicit
**compatibility matrix** and validates every contract against it before any reasoning begins. The
governing rule: an unsupported combination resolves to `unsupported` — it is never silently
approximated by the nearest supported semantics.

## Selecting a source theory

Use repeatable `gmeow prove --module IRI` options to select one composed theory.
It includes the supplied documents' top-level obligations and every requested
module's actual transitive `logic:imports`. Declare each root and imported target
as a `logic:Module` in the supplied default graph. Missing declarations refuse;
filenames, slice dependencies and graph names do not imply module membership.
Selecting several roots combines their admitted sentences without inventing
import edges. A contradiction across selected modules refutes the composition.

Module membership does not translate a standpoint, time or modality. Those
coordinates retain their separate admission requirements. Ground module claims
can assert through a typed, source-resolved module route without invented
provenance or confidence; evidence-only claims keep their own closed contract.
Both contracts admit default-graph resource annotations `gmeow:graphBoxRole`
and outgoing `gmeow:wasDerivedFrom`, and `gmeow:statedRationale` text, as
descriptive source facts. Preserve the exact role, lineage target, wording,
language and direction. These annotations supply no evidence or assertion
authority. Quotations still assert nothing. Named-graph annotations, values
outside the admitted types, unknown qualifiers, mentions and graph-name uses
retain their boundaries.

An ordinary default-graph `math:hasElement` row can list a carrier in a set
explicitly declared as `math:FiniteSet` with `logic:instanceOf` or `rdf:type`.
Both rows must be selected. An independently admitted claim in the set remains executable, and
a listed quotation remains a quotation. The original membership is available
to rules. Other incoming references still qualify the carrier.

Reports preserve the complete supplied source inventory and identify the roots,
resolved imports, actual consumption and remaining blockers. An unselected module
is outside this requested theory; an unsupported selected owner is still a
blocker. Without `--module`, the top-level operation retains its module-scope
boundaries.

For an accounted native formula, `FormulaEvidence` links each eligible
default-graph `logic:provenance` row to the original formula obligation IDs.
An evidence row on shared syntax can name several selected occurrences.
The source resource remains evidence for those occurrences and supplies no
object-level premise or assertion authority. Text reports connect each
`formula-evidence` row to its `formula-origin` by obligation ID and show the
original source anchor on both lines.

`gmeow prove` evaluates selected `logic:NonEntailmentObligation` declarations
against the retained native execution. The report links the type and field
rows to the exact source obligation IDs and names the assessed world.
When compilation refuses an unread governance field, the source inventory
identifies the field, its original source coordinates and its diagnostic.
The selected authored fields determine the authorized discharge conditions.
Discharge requires a supported absence proof and complete selected source
coverage. A derived forbidden predicate carries its witness. Invalid fields
and unresolved obligations leave named completion boundaries. Logical
inconsistency requires a native or OWL refutation.

Composed FOF output and raw proof export/replay have no composition-bound
admission. A composed `--out` or `--proof-out` request fails its required output
and removes stale output; composed `--check-proof` rejects after resolving and
reporting the request. Output paths must be distinct from selected sources,
proof inputs and one another; aliases refuse before any output changes. Source
discovery runs once, and semantic loading and proof replay read the admitted open
files. Output publication and stale-output removal use held parent directories;
switching a parent symlink cannot redirect them onto a selected input. Writes
check the actual opened target before truncation. The boundary does not cover
arbitrary replacement of entries inside a held directory. The
`PROOF_NOT_RECORDED` required-output refusal removes the stale selected proof. A native
verdict remains an attestation. See
[`LOGIC-SEMANTICS.md`](design/LOGIC-SEMANTICS.md#explicit-module-composition)
for the assertion and context contract.

## The typed context algebra

`logic:` replaces a single generic `logic:World` with a typed algebra of distinct contexts, each
with its own typed accessibility relation:

| Context type | What it is | Typed accessibility relation |
| --- | --- | --- |
| `logic:PossibleWorld` | an alethic possibility | epistemically-possible / alethically-accessible |
| `logic:EpistemicContext` | what an agent knows or believes | doxastically-accessible |
| `logic:Standpoint` | a named perspective truth is relative to | sharpens (a refinement poset) |
| `logic:Scenario` | a hypothesized situation under consideration | scenario-entertains |
| `logic:State` | one state of affairs along a history | (successor within a path) |
| `logic:History` / `logic:Path` | an ordered run of states | temporally-succeeds |
| `logic:ReferenceFrame` | a frame of measurement or canon | frame-relative-to |
| `logic:NarrativeFrame` | an in-universe representational canon | depicts / in-frame |

Deontic and counterfactual reasoning are *uses* of these contexts rather than separate types. The
generic `logic:accessibleFrom` superproperty exists for uniform traversal and provenance, never for
inference: cross-type entailment requires an explicit bridge rule that names both types and states
the consequence relation it carries.

The most consequential rule of the algebra: **an unindexed statement holds in an
`gmeow:unspecifiedStandpoint` — it is unspecified, not universal.** Universality is something the
author asserts explicitly; absence of an index says nothing about what holds where. This is the
context-algebra counterpart of the open-world assumption: silence about context is not a claim of
universal context-independent truth.

## Transaction Logic and state change

State change is the `transaction-path` value of the Evolution facet. A query in this facet is
evaluated over a **path** — an ordered sequence of states — rather than over a single state.
Serial conjunction is a program combinator (do φ, then ψ) typed apart from ordinary conjunction,
which holds at a state. Updates are supersession, never erasure: a `del` retires one active support
in the successor state and records supersession provenance; displayability remains a separate
disclosure policy, and the substrate remains append-only and fully auditable.

The hypothetical operator — testing a transaction without committing its effects — is the value
`HypotheticalExecution` of the orthogonal `logic:ExecutionMode` facet (a sibling of the Evolution
facet, not a value within it), and is distinct from modal possibility (◇): it asks whether a program
*can execute*, not whether a proposition holds in some accessible world. Modal possibility (◇) stays
computed as the standard translation over the appropriate typed accessibility relation
(`logic:epistemicallyPossible` / `logic:doxasticallyAccessible`), never over the bare
`logic:accessibleFrom` superproperty and never reified as a class. Concurrent
Transaction Logic extends the path model to interleaved execution, surfacing non-serializable schedules
as findings.

## Constitutional role

This slice realizes Constitution **Principle 17**: `logic:` is the canonical reasoning language, and
OWL, RDFS, SHACL, Datalog, Prolog, N3, SPARQL, and the gUFO/BFO/DOLCE upper ontologies are generated
lossy projections of it. The `logic:` namespace and the UFO⁺ foundational surface are declared as
standalone terms that add no axioms to the reasoned core until they pass the formalization lifecycle
([`design/LOGIC-FOUNDATION.md`](./design/LOGIC-FOUNDATION.md)). The OntoUML disciplines are `logic:`
rules the native solver evaluates, and the matching enforcement gates live in
[`governance/constitution.ttl`](../../../governance/constitution.ttl).

## Terms

All terms are authored in the `logic:` namespace
(`https://blackcatinformatics.ca/logic/`), **standalone by design**: `logic:` is the canonical
ground and `gmeow:` (with gUFO, OWL, SHACL, …) is a generated lossy projection of it
(Principle 17), so no `logic:` term carries `gmeow:` parentage.

**UFO⁺ sorts and superset spine.** The foundational sorts
(`Kind`/`SubKind`/`Phase`/`Role`/`Category`/`Mixin`/`RoleMixin`/`PhaseMixin`/`Relator`/`Event`/`Situation`)
and the wider superset spine
(`Endurant`/`Perdurant`/`Object`/`Aspect`/`Substantial`/`AbstractIndividual`/`Quality`/`Mode`/
`QualityValue`/`QualitySpace`/`Process`/`Participation`/`Fluent`/`Collection`/`Quantity`/
`FunctionalComplex`/`FixedCollection`/`VariableCollection`/`Individual`/`ConcreteIndividual`).

**Foundation relations.** `rigidlyAppliesTo`/`suppliesIdentity`/`mediates`/`instanceOf`/`orderedType`
and the mereology properties (`partOf`/`properPartOf`/`memberOf`/`temporalPartOf`).

**Reasoning contract and presets.** A `ReasoningContract` is an independent selection across the
orthogonal reasoning facets (formula fragment, model semantics, negation, truth algebra, closure,
revision, resource policy, …). The six historical profile names — `PositiveHornProfile`,
`StratifiedNAFProfile`, `WellFoundedProfile`, `StableModelProfile`, `ProceduralPrologProfile`,
`ProbabilisticProfile` — survive as `ReasoningPreset` named individuals: sugar that `logic:expandsToFacet`
expands into a full contract facet bundle, documented term-by-term in [`module.ttl`](./module.ttl).

**Execution-commitment facet.** The `ExecutionMode` facet is orthogonal to the Evolution facet:
commitment (commit vs. discard) and state-change shape are independent dimensions. The sandbox
operator is realized as `logic:HypotheticalExecution` — a facet value, not modal possibility (◇),
which stays computed as the standard translation over the appropriate typed accessibility relation
(`logic:epistemicallyPossible` / `logic:doxasticallyAccessible`), never over the bare
`logic:accessibleFrom` superproperty, and is never reified as a class. The selector property is
`logic:executionMode`.

| Term | Kind | Description |
|---|---|---|
| `logic:ExecutionMode` | `owl:Class` | The facet value class of the execution-commitment facet — whether a Transaction-Logic program's effects are committed or discarded. Orthogonal to `logic:EvolutionMode`. |
| `logic:CommittedExecution` | `logic:ExecutionMode` | The execution-mode value under which a transaction program's elementary updates are committed — the effects advance the path and persist. The default execution mode. |
| `logic:HypotheticalExecution` | `logic:ExecutionMode` | The sandbox/hypothetical operator: a transaction program is executed to test whether it would succeed, with effects discarded. Distinct from modal possibility (◇), which is computed over the appropriate typed accessibility relation (`logic:epistemicallyPossible` / `logic:doxasticallyAccessible`), not the bare `logic:accessibleFrom` superproperty, and never reified. |
| `logic:executionMode` | `owl:ObjectProperty` | Relates a `logic:ReasoningContract` to the `logic:ExecutionMode` it selects. Single-valued on the contract; independent of `logic:evolution`. |

**World/modal terms.** `World`/`accessibleFrom`/`counterfactualOf` and the typed context
algebra described above. The two kernel typed-context classes are:

| Term | Kind | Description |
|---|---|---|
| `logic:PossibleWorld` | `owl:Class` | A typed alethic context — a world of alethic possibility; the contexts its typed accessibility relation `logic:epistemicallyPossible` ranges over. |
| `logic:Path` | `owl:Class` | An ordered run of states (a history) over which `logic:temporallySucceeds` is read; the Transaction-Logic state sequence whose positions are `gmeow:State` configurations. |

The five typed accessibility relations are:

| Term | Kind | Description |
|---|---|---|
| `logic:epistemicallyPossible` | `owl:ObjectProperty` | The typed alethic accessibility relation between `logic:PossibleWorld`s — a world is epistemically (alethically) accessible from another. |
| `logic:doxasticallyAccessible` | `owl:ObjectProperty` | The typed doxastic accessibility relation between `gmeow:EpistemicContext`s — what an agent's beliefs hold accessible. |
| `logic:deonticallyIdeal` | `owl:ObjectProperty` | The typed deontic accessibility relation: a context is deontically ideal — an ought-world — relative to an issuer. |
| `logic:temporallySucceeds` | `owl:ObjectProperty` | The typed temporal accessibility relation along a `logic:Path`: one `gmeow:State` succeeds another in the ordered run of states. |
| `logic:counterfactuallyCloser` | `owl:ObjectProperty` | The typed counterfactual accessibility relation under a declared closeness ordering (Lewis comparative similarity). |

None of these are asserted `rdfs:subPropertyOf logic:accessibleFrom`; the generic superproperty relationship is in prose only.

**Quantitative axes.** `probability`/`confidence`/`weight`/`evidenceStrength` are four
normatively distinct predicates. A confidence score, a calibrated probability, a solver ranking
weight, and an evidential warrant are not interchangeable; treating arbitrary confidence metadata
as a probability model is a named failure mode the vocabulary exists to prevent.

**Probabilistic model vocabulary.** `ProbabilityModel`/`probabilityModel`/`FullIndependence`/`DependencyModel`/
`correlates`/`JointOutcome`/`jointOutcome`/`outcomeAssignment`/`jointProbability` — the explicit
independence and dependency model machinery that probabilistic inference requires. Probabilistic
inference under `ProbabilisticProfile` is only available when a model is declared; the vocabulary
never silently assumes independence over bare `probability` annotations.

**OntoUML discipline diagnostics.** `Discipline`/`violation`/`rigidityViolation`/`dischargeObligation`/
`witnessRequiredViolation` and the discipline named individuals (`StereotypeCardinality`/`MixIden`/
`FreeRole`/`MixRig`/`RelComp`). These are derived diagnostics entailed by the lowering rules, never
asserted by hand; their presence is a machine-checkable proof that a class or individual breaks the
named discipline.

**Rule-body structural properties.** `negatedBody` (a negation-as-failure body atom, lifting a rule
out of positive Horn) and `distinctBody` (an inequality guard for detecting distinct values, leaving
stratification unchanged).

**Preservation-polarity vocabulary.** `PreservationKind`/`preservationKind`/`complexityClass` and
their named individuals
(`ExactPreservation`/`SoundUnderApproximation`/`CompleteOverApproximation`/`ValidationOnly`/`InconsistencyPreserving`/`InconsistencyReflecting`).
These record the answer-preservation guarantee each projection makes alongside its decidability
class; "lossy" is not enough — a consumer needs to know what a projection guarantees, not only what
it omits.

**Type and meta-type taxonomy.** `Type`/`EndurantType`/`RelationshipType`/`MaterialRelationshipType`/
`ComparativeRelationshipType`/`AbstractIndividualType`/`ConcreteIndividualType`/`Sortal`/`NonSortal`/
`RigidType`/`AntiRigidType`/`SemiRigidType`/`NonRigidType` — the higher-order meta-types that
classify the foundation sorts by identity-supply and modal rigidity axes.

**Scoped open/closed world vocabulary.** `WorldBoundary`/`closedUnder` — the construct that confines
the closed-world assumption to a declared scope while leaving the rest of the model open-world.

**Builtins.** `Builtin`/`invokesBuiltin` and the named primitive individuals
(`builtinStringConcat`/`builtinDateDifference`/`builtinArithmetic`). Builtins are the lightweight
derivational primitives a foundation genuinely needs; heavy domain computation stays external.
Builtin-invoking rules require `ProceduralPrologProfile` and record the loss when projected to a
declarative semantics.

**Property characteristic markers.** `transitiveProperty`/`asymmetricProperty`/`irreflexiveProperty`
— the `logic:` property-characteristic markers that allow `properPartOf` to carry the transitive
∧ asymmetric ∧ irreflexive strict-partial-order combination that OWL 2 DL globally forbids.

**Occurrent refinements (the YAMATO process/event axis).** `causalPartOf` — a transitive
specialization of the mereology relation `temporalPartOf` (causal ⊆ temporal), so a causal
dependency is always a temporal part but not conversely. `occurrentBoundary` over the closed value
vocabulary `OccurrentBoundary` { `Open`, `Closed` } types an occurrent as an on-going action
(*arrive*) or a completed unitary event (*arrival*) — a **value, not a subclass** (Principle 9).
`occurrentConstituent` (range `Process`) is the constitutive "events are made of processes"
relation, deliberately kept distinct from both temporal and causal parthood. The
`OccurrentConstraint` violation family records the entailed diagnostics: `OccurrentChangeAsymmetry`
(a `Closed` occurrent must bear no time-varying `Fluent` — the process≠event change-asymmetry) and
`OccurrentBoundaryMismatch` (an occurrent must not be both `Open` and `Closed`), both enforced by
the native foundation rules.

All terms are documented individually in [`module.ttl`](./module.ttl) and the design set above.

### gmeow:sharpens

The one `gmeow:`-side seam the foundation names: `logic:accessibleFrom` (the
Kripke accessibility relation between worlds) generalizes the standpoint
sharpening poset asserted on the `gmeow:` side via `gmeow:sharpens`. In the
typed context algebra, `gmeow:sharpens` is the typed accessibility relation for
`logic:Standpoint` contexts — a transitive, reflexive refinement poset distinct
from the doxastic, alethic, and temporal accessibility relations of the other
context types. The specialization is declared on the `gmeow:` side, never minted
as an axiom in this module, so the `logic:` foundation stays standalone
(Principle 17).

## Literal language and direction

The natural-language fields `logic:withdrawalReason`, `logic:message`, `logic:fragmentBoundaryReason` admit
plain text with unspecified language, language-tagged text, and RDF 1.2 directional
text. Supplied public or private-use language tags remain part of the literal.
The canonical value restrictions derive the validation view; they do not add a
closed-world requirement for an explicitly asserted subject type. A machine key,
source expression, or byte payload uses its own declared property and datatype.
See [literal policy](../../../docs/LITERALS.md) for the shared contract.

## Comparing bound RDF components

Use `termSameComponent` when an integrity condition needs RDF identity rather
than `termEqual` value equality. Supply four ordered arguments: left term,
left selector, right term and right selector. Selectors are the constant IRIs
`componentWhole`, `componentDatatype`, `componentTripleSubject`,
`componentTriplePredicate` and `componentTripleObject`. Every variable operand must be positively bound;
the predicate binds nothing. The whole selector retains recursive RDF identity,
while datatype and quoted selectors inspect the original native value.

An unavailable component yields false, including when one operand is not a
literal or quoted triple of the requested kind. Negation applies after that
boolean is formed. Missing bindings refuse the program. Wrong arity or selectors
and unsupported native execution contexts refuse or retain explicit loss rather
than interpreting the intrinsic as stored graph data. See the
[bound identity contract](design/LOGIC-VALIDATION.md#bound-rdf-component-identity)
for projection, polarity and source-authority details.
