<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — Vision and Doctrine

> The **manifesto** of the GMEOW Logic design set; it carries the vision, doctrine, and lineage.
> The formal semantics, runtime, and conformance contract live in the sibling documents below.
> Where this document states a thesis once, the siblings make it precise — repetition is replaced
> by cross-reference on purpose. The cross-slice contract binding this slice to its co-foundational
> peers (`lang:`, `math:`) — the seam registry, shared disciplines, and acceptance bar — is
> [`docs/GROUNDING.md`](../../../../docs/GROUNDING.md).

## The document set

| Document | Genre | Contents |
|---|---|---|
| `LOGIC.md` (this) | manifesto | vision, doctrine, lineage |
| [`LOGIC-FOUNDATION.md`](LOGIC-FOUNDATION.md) | charter | the `gmeow:logic` upper-ontology charter — the gUFO ⊇ baseline, the criticism ledger, the greenfield feature map, the Ithkuil precision ethos, the four-box organization |
| [`LOGIC-CONTRACT.md`](LOGIC-CONTRACT.md) | configuration | the reasoning contract — the orthogonal facets a reasoning request selects; named profiles as presets; the compatibility matrix |
| [`LOGIC-IR.md`](LOGIC-IR.md) | intermediate representation | the typed, full first-order IR every source compiles into and every projection out of; the per-lowering preservation judgment; the three IR commitments (legalization, load-bearing annotations, the relational core) |
| [`LOGIC-CORRESPONDENCE.md`](LOGIC-CORRESPONDENCE.md) | correspondence calculus | cross-ontology alignment as the **ninth** IR node kind; the ordered law-spine, the mnemomorphism keystone, the separated axes, the generated lowerings (SSSOM/EDOAL/FnO/SPARQL/up-lift), and the six-layer OpenEHR subsumption |
| [`LOGIC-SEMANTICS.md`](LOGIC-SEMANTICS.md) | formal semantics | the unified core, triple-term/assertion rules, the reasoning result, modality, the typed context algebra, decidability |
| [`LOGIC-TRANSACTION.md`](LOGIC-TRANSACTION.md) | state change | Transaction Logic — path semantics, serial conjunction, updates as supersession, the state-change facet |
| [`LOGIC-PATHS.md`](LOGIC-PATHS.md) | traversal | named & parametric predicate paths — `logic:PathShape`, the predicate wildcard and bounded `{1,n}` depth that SPARQL §9 lacks, by-name parametric invocation, and the property-path / Datalog projections with their declared exit loss |
| [`LOGIC-SHACL-AF.md`](LOGIC-SHACL-AF.md) | computation surface | derivation/aggregation (map/reduce) authored as canonical `logic:` rules and **projected** to a SHACL-AF `sh:SPARQLRule` surface — computation added to the canon and emitted, never bolted onto SHACL; the projectable fragment, the declared exit loss, and the placement/purity rule |
| [`LOGIC-VALIDATION.md`](LOGIC-VALIDATION.md) | shape surface | closed-world data-shape validation authored as canonical `logic:` **validation shapes** and **projected** to a SHACL Core surface and a ShEx surface — shapes emitted from the canon, never hand-authored on the surface; the constraint peer of the SHACL-AF derivation surface, the per-target residue, and the shape half of the purity gate |
| [`LOGIC-RDFQUERY.md`](LOGIC-RDFQUERY.md) | query surface | RDFQuery framed as a front-end that parses **into** `logic:` (which already projects out to SPARQL/SHACL/N3/OWL), not a stack bolted onto SPARQL; P15-gated, language not committed, with the named first consumer |
| [`LOGIC-TELEOLOGY.md`](LOGIC-TELEOLOGY.md) | goal/action layer | goals, intentional modes, structured goal expressions, reified goal evaluation, action schemas, goal decomposition and conflict; the intention → plan → action → transaction-path chain |
| [`LOGIC-COGNITION.md`](LOGIC-COGNITION.md) | cognitive assessment | the multidimensional cognitive-assessment construct — factored dimensions of reasoning quality, reliability, calibration, and metacognitive posture; reasoning quality over the inference modes; reliability over the typed reasoning result |
| [`LOGIC-RUNTIME.md`](LOGIC-RUNTIME.md) | runtime | solver architecture, the materialization–resolution seam, graph versioning, generated artifacts, CLI |
| [`LOGIC-PERFORMANCE.md`](LOGIC-PERFORMANCE.md) | performance | how the native physical engine is made fast without weakening semantics — the deterministic-performance contract, data-shape/join/demand doctrines, the incremental algebra, the chase-termination ladder, deterministic parallelism, provenance cost bounds, the grounding-layer computability seam, the Rust mechanical-sympathy standard, and the measurement regime |
| [`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md) | contract | the conformance corpus and the loss-ledger preservation contract |
| [`LOGIC-META-SEMANTICS.md`](LOGIC-META-SEMANTICS.md) | meta-semantics | the projection doctrine turned inward; the catalog of orthogonal canonical axes against the simplified surfaces generated from them |
| [`LOGIC-REFERENCES.md`](LOGIC-REFERENCES.md) | appendix | external standards, theory, and engines cited — staged for the `metadata/references.ttl` ledger |

> **Reading this design set.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by the conformance corpus
> ([`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md)). It is not a claim that any particular
> implementation already realizes X except as the corpus demonstrates.

## The thesis

GMEOW must not be limited by the expressivity, serialization assumptions, or runtime cost of
OWL-era tools. The current stack uses OWL 2 DL/EL, RDF 1.1 compatibility encodings, Jena, ROBOT,
SHACL and `owlrl` and other mature external tools. That is a
compatibility position, not the semantic ceiling.

`logic:` is the canonical reasoning language for GMEOW. It is RDF 1.2-native, not an OWL syntax
with a new namespace. It accepts **none of its predecessors' restraints** — not OWL's decidability
ceiling, not the DL-safe rule restriction, not the tree-model property, not enforced monotonicity,
not the forced choice between open-world and closed-world semantics. It subsumes those systems as
fragments and exceeds all of them. Then, following the projection doctrine that governs the rest
of the project, it generates lossy compatibility artifacts for the tools and ecosystems that
cannot consume the canonical form.

How much does OWL leave out? The exact fraction is **not asserted here as a slogan** — it is
measured and emitted by `generated/logic/projection-report.ttl` across construct coverage,
competency-question coverage, preserved entailments, validation constraints, explanation/
provenance features, and modal/counterfactual features (see
[LOGIC-CONFORMANCE.md](LOGIC-CONFORMANCE.md)). The durable, qualitative point is what OWL can
*never* hold: logic programming, defeasible and non-monotonic inference, contextual/standpoint/
temporal/modal/probabilistic scope, paraconsistent treatment of contradiction, metalevel
reasoning over statements, and the modal and second-order content of a real foundational
ontology. OWL deliberately trades expressivity for decidability; `logic:` does not.

`logic:` is deliberately **Turing-complete** — a computational substrate, not merely a description
language. It is meant to compute, generate, and search, not only to classify, which is the
dividing line between OWL (sub-Turing by design) and the Prolog/N3 lineage `logic:` belongs to.
Rejecting the decidability *ceiling* is a choice, not an oversight: it means refusing to let
decidability cap the canonical model, then *recovering* termination and tractability where a
consumer needs them — as a projection guarantee and a statically certified profile, never as a
restriction on what can be said. The formal account — the halting problem, decidability-as-
projection, and the certified profiles — is in
[LOGIC-SEMANTICS.md](LOGIC-SEMANTICS.md#turing-completeness-decidability-and-termination).

A reasoning request is a typed, compositional **reasoning contract** over orthogonal facets —
consequence relation, negation kind, closure assumption, context indexing, state-change mode,
uncertainty handling, and so on. Named profiles are presets: bundles of facet values that the
compiler expands before evaluation, never indivisible alternatives. The contract model is canonical
rather than any small fixed set of profiles, because the orthogonal dimensions compose in
combinations no fixed list can fully anticipate. The full definition of the facets, presets,
and compatibility matrix is in [LOGIC-CONTRACT.md](LOGIC-CONTRACT.md).

This is the same doctrine GMEOW already applies everywhere else:

- author once in the canonical model;
- generate weaker views for legacy or surface consumers;
- make loss explicit and machine-readable;
- gate every generated artifact for drift;
- never let a compatibility format become a second source of truth.

`logic:` applies that doctrine to logic itself, and to the foundational ontology.

## Why This Exists

The current reasoning lane has three structural limits.

First, **OWL is not RDF 1.2.** GMEOW's statement layer is already RDF 1.2-first, but OWL reasoners
consume an RDF 1.1-compatible downcast. That downcast is useful, but it cannot be the whole
semantic story for standpoint-indexed, attributed, temporal, confidence-weighted claims.

Second, **OWL captures one fragment and forbids the rest.** OWL 2 DL and EL optimize for decidable
classification. They cannot express logic programming, recursion with negation, value-inventing
rules, defeasible defaults, modal necessity, second-order identity supply, weighted/probabilistic
inference, or reasoning about statements as objects. SHACL adds closed-world validation but no
inference; Datalog adds recursion but no open-world classification; Prolog adds computation but is
not RDF-native and has no open-world reading. **No prior system unifies these, and none is RDF
1.2-native.** GMEOW needs all of them, coherent, in one framework.

Third, **the current Java/Docker reasoning path is expensive.** A classical DL reasoner's
sound-and-complete consistency check over the merged ontology runs in the order of minutes and
grows with the ontology. Such tools are valuable compatibility checkers, but they are too slow
and too far from RDF 1.2 to be the canonical authority.

## Lineage and Supersession

`logic:` is a deliberate superset. Each predecessor contributes a fragment; each imposes a
restraint we reject; `logic:` subsumes the contribution and discards the restraint.

| Predecessor | Contributes | Restraint we reject | How `logic:` exceeds it |
|---|---|---|---|
| RDFS | lightweight subsumption | almost no semantics | full taxonomy + everything below |
| OWL 2 DL | decidable classification, rich class constructors | decidability ceiling, tree-model property | classification is one *mode*; no expressivity cap |
| OWL 2 EL | tractable subset | even narrower | one projection profile, nothing more |
| SWRL | rules over OWL | DL-safe restriction; rules bolted on, not native | rules are first-class and unrestricted |
| RIF | rule interchange | never fully realized; interchange, not a logic | a single native logic, not an exchange format |
| SHACL | closed-world validation, shapes | no inference; validation only | open- and closed-world co-resident |
| Datalog (Soufflé, RDFox) | recursive monotonic rules, fast materialization | monotonic only; no open-world classification; no modality | monotonic *and* non-monotonic; with classification and modality |
| Prolog (SWI, Trealla) | unification, SLD resolution, computation | not RDF-native; no open-world reading; no native probability | logic programming over RDF 1.2 triple terms |
| ProbLog | probabilistic logic programming | a separate tool from one's ontology | probability/confidence scope is first-class (carefully typed — see semantics) |
| F-logic (Flora-2, Ergo) | frame/object reasoning + LP | its own syntax, outside RDF | frame reasoning over native RDF frames |
| N3 Logic (cwm, EYE) | RDF-native rules, **quoted graphs**, builtins, both chaining directions | pre-RDF-1.2 cited formulae; no contextual/modal scope as data | RDF 1.2 triple terms are the modern cited formula, with full contextual scope |
| SPARQL | query, CONSTRUCT | a query language, not a logic | query is a *projection* of goal resolution |
| Common Logic | first-order interchange | no RDF model, no contextual layer | FOL-grade expressivity, RDF 1.2-native, contextualized — and operationalized as generated *and* ingested CLIF, CGIF, and XCL dialects, and independently cross-checked by an external first-order reasoner over the CLIF export (an FOL oracle held to the *native ⊇ oracle* discipline) and by the external TPTP/SZS soundness corpus over the full-FOL IR (see [`LOGIC-CONFORMANCE.md`](LOGIC-CONFORMANCE.md)), not merely cited as an ancestor |
| gUFO | OWL upper ontology, stereotypes | a *lossy OWL realization of UFO* — drops modality and higher-order types | the full foundational theory; gUFO becomes a projection |
| Cyc (CycL, microtheories) | maximal common-sense axiomatization ambition; microtheories as context indexing (CycL contexts holding mutually-inconsistent assertions — prior art for standpoint/context indexing) | truth-as-a-bit + curated-monolith epistemology; hand-curation without statement-level provenance / confidence / revision-as-suppression | typed context algebra with accessibility; attributed, confidence-weighted defeasible claims; conformance-gated projection *as* curation |

The closest living ancestor is **N3 Logic and the EYE reasoner**: RDF-native rules, quoted graphs
(the direct precursor of RDF 1.2 triple terms), builtins, and both forward and backward chaining.
`logic:` is N3's maximal successor — it keeps the RDF-native, quoted-formula, bidirectional core
and adds contextual scope, defeasibility, modality, probability, paraconsistency, and a
foundational ontology, all as first-class data.

### Design influences beyond formal logics — Ithkuil

The ambition — maximal, explicit, factored precision — has a natural-language analog in Ithkuil
(John Quijada), the engineered language built to make cognitive distinctions fully explicit. Four
lessons carry directly into `logic:`:

- **Orthogonal factorization is the master principle.** Ithkuil composes meaning from
  independently-varying categories (Configuration, Affiliation, Perspective, Extension, Essence,
  Valence, Phase, Aspect, Context, Bias, Validation, …) rather than an enumerated lexicon. This is
  GMEOW's orthogonality principle taken to its limit, and a proof that a vast semantic space is
  reachable from a small set of composable primitives. The `logic:` foundational categories and
  contextual scopes are therefore **factored axes you combine**, not a flat list of types — kept
  genuinely orthogonal by the existing disjointness checks
  (`queries/verify/class-in-two-disjoint-axes.rq`) and composed by the existing compound-term
  machinery (`compound_expand`).
- **Evidentiality is obligatory and structural.** Ithkuil's *Validation* category forces every
  statement to mark its knowledge source — observed, inferred, reported, intuited, conjectured.
  `logic:` makes epistemic/standpoint/provenance/confidence scope **obligatory and structural**,
  not an optional annotation: no asserted triple without its epistemic frame.
- **Essence — existential versus representational.** Ithkuil marks whether a referent is actual or
  hypothetical/ideal/representational. `logic:` carries this as a modal/disclosure axis on every
  entity, which is exactly what fiction, depiction, and hypothetical claims require.
- **Factored aspect and configuration.** Ithkuil's dozens of aspects and its systematic part/whole–
  membership system are a blueprint for a factored aspectual algebra (for events and temporal
  reasoning) and a systematic mereology/plurality axis finer than a coarse collective/quantity
  split.

The decisive lesson is cautionary, and it becomes the project's strongest framing. Ithkuil achieves
total precision but is **near-unspeakable** — maximal expressivity with no usable surface. That is
precisely the failure mode a maximal logic risks, and exactly what the projection doctrine
prevents: author once in the maximal canon, then project to speakable, tractable surfaces (OWL,
Datalog, Prolog, SHACL, gUFO) for every consumer.

> **GMEOW Logic is Ithkuil's precision with Ithkuil's fatal flaw engineered out — a maximal canon
> that always carries a lossy, usable projection.**

**The same lessons compact GMN-1, not only `logic:`.** The orthogonal-factorization,
obligatory-evidentiality, and factored-aspect lessons above are applied a second time, downstream,
to the token-compact GMN-1 dialect surface (`design/LANG-GMN.md`, "The factored qualifier slots"):
a closed set of single-token qualifier markers in fixed record positions — a modality slot, an
evidentiality-kind slot, and an `@p`-process-record boundary/iteration pair — each dealiasing to
vocabulary `logic:` (or a `logic:`-adjacent grounding/core slice) already formalizes, never a
GMN-local shadow axis. Where Ithkuil's own failure mode was engineered out of `logic:` by always
carrying a lossy, speakable projection, it is engineered out of GMN-1's compaction by a narrower,
executable razor: a marker is admitted only if it measurably reduces token cost or retires a
named ambiguity class with a falsifiable fixture, and no marker is ever a private symbol — the
same "precision without a usable surface" trap, closed the same way, one level further out on the
projection ladder.

## OWL, gUFO, and the upper ontologies as projections

The canonical statement of the doctrine, made once here and referenced elsewhere: every prior
formalism is a *generated, lossy compatibility target* — useful, documented, reproducible, and not
canonical. OWL DL/EL, Datalog, SHACL, ShEx, SWRL, N3, Prolog, and SPARQL are projections of the logic;
the artifact set, drift gates, and preservation contract are specified in
[LOGIC-RUNTIME.md](LOGIC-RUNTIME.md#generated-artifacts-and-the-compilers-projection-role) and
[LOGIC-CONFORMANCE.md](LOGIC-CONFORMANCE.md).

These formalisms do not all relate to `logic:` the same way, and the doctrine keeps the relations
distinct: `logic:` **is built atop** RDF 1.2 (the substrate, never projected); it **is a superset of**
the definitional formalisms (OWL, RDFS, SKOS, gUFO, UFO — lifted into the IR and projected back, the
crisp fragment at `ExactPreservation`); it **down-projects lossily to** the closed-world validation
surfaces (SHACL, ShEx); and it **derives, with the correspondence/mappings layer**, the alignment
surfaces (SSSOM, EDOAL, FnO). The single lattice that places every formalism on these four relations —
and the shared adapter-lift foundation they share — is in
[LOGIC-META-SEMANTICS.md](LOGIC-META-SEMANTICS.md#the-outward-face-every-prior-formalism-placed-on-one-lattice).

The foundation follows the same doctrine, with one careful distinction. **gUFO is the primary
generated down-projection of UFO⁺** because it is the OWL realization of the same UFO lineage —
truth-preserving for the fragment OWL can express. **BFO, DOLCE, and SUMO are generated
alignment/bridge views, not truth-preserving projections**, unless a specific subfragment is
certified as such in the loss ledger. They carry genuinely different ontological commitments, and
the maximal-source doctrine respects that rather than overclaiming a shared foundation. The
operational semantics of the foundation are in
[LOGIC-SEMANTICS.md](LOGIC-SEMANTICS.md#the-logic-foundation-ufo); its projection discipline is in
[LOGIC-FOUNDATION.md](LOGIC-FOUNDATION.md#foundation-projection-and-discipline).

**Alignment is a projection too.** The cross-ontology alignment layer follows the identical doctrine,
recursed one level further: a `logic:Correspondence` is the canonical alignment object (the **ninth**
IR node kind), and SSSOM, EDOAL, FnO, SPARQL CONSTRUCT, OWL alignment axioms, and the up-projection
lift map are its generated lossy lowerings, each carrying a preservation judgment in the same loss
ledger. This makes "GMEOW perfectly subsumes vocabulary V" a CI-checkable section/retraction law
rather than a slogan. The calculus is in [`LOGIC-CORRESPONDENCE.md`](LOGIC-CORRESPONDENCE.md); it is
the third consolidation under this doctrine, peer to the canonical process model and the typed
compositional meta-semantics, all three sharing the native execution engine
([LOGIC-RUNTIME.md](LOGIC-RUNTIME.md#the-native-physical-engine--execution-and-optimization)).

## Constitutional Alignment

`logic:` is the project's own doctrine applied to logic and to its foundation. The CONSTITUTION
requires a maximal canonical model, maximal linking, explicit and gated projection, and no
compatibility format promoted above the canonical source. The statement layer already realizes this
for facts; `logic:` realizes it for axioms, rules, and the upper ontology. OWL, Datalog, SHACL,
Prolog, N3, SPARQL, and gUFO take their correct places as documented, reproducible, lossy
projections — never second sources of truth.

## End State

The end state is not "OWL, but faster." It is:

- `logic:` is the canonical, maximally expressive logic — a Turing-complete computational substrate,
  not merely a description language; a superset of description logic, logic programming, constraints,
  and contextual/modal/temporal/probabilistic/paraconsistent reasoning, RDF 1.2-native;
- the foundational ontology (UFO⁺) is authored in `logic:`, with its discipline expressed as axioms
  rather than external lint;
- a single canonical native solver is the normal development authority, running forward and backward
  chaining; classical OWL tools (Jena, ROBOT) operate as secondary validators for
  exported subsets;
- OWL, Datalog, SHACL, ShEx, Prolog, N3, SPARQL, gUFO, and the Common Logic dialects (CLIF, CGIF,
  XCL) are generated lossy projections — the SHACL Core and ShEx shape surfaces lowered from the
  canonical `logic:` validation-shape node kind ([LOGIC-VALIDATION.md](LOGIC-VALIDATION.md)), the
  SHACL-AF rule surface from its derivation rules; BFO, DOLCE, SUMO, and YAMATO are generated
  bridge views;
- cross-ontology alignment is the **ninth IR node kind** (`logic:Correspondence`): SSSOM, EDOAL, FnO,
  SPARQL, OWL-alignment, and the up-lift are generated lowerings, and perfect subsumption is a
  CI-checkable section/retraction law ([LOGIC-CORRESPONDENCE.md](LOGIC-CORRESPONDENCE.md));
- projection loss is visible, machine-readable, and tested.

This makes GMEOW's logic match the rest of the project: maximal model, maximal linking, explicit
projection, and no compatibility format — not even OWL, not even gUFO — promoted above the canonical
source.

### The goal/action and cognitive-assessment layers

Two further layers complete the design. The **goal-and-action layer**
([LOGIC-TELEOLOGY.md](LOGIC-TELEOLOGY.md)) carries structured goal expressions, reified goal
evaluation, and action schemas, and binds the intention → plan → action → transaction-path chain:
the transaction layer handles the state-change mechanics, and the goal-and-action layer carries the
goal structure that motivates and evaluates those transactions. The **cognitive-assessment layer**
([LOGIC-COGNITION.md](LOGIC-COGNITION.md)) carries a contextual, factored assessment construct —
subject granularity, task, evaluator, evidence, scale, interval, and independent dimensions — in
place of a single ordinal competence score, which cannot represent cognitive competence across
contexts.
