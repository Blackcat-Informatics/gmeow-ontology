<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — Foundation Design Charter (gmeow:logic ⊇ gUFO)

> The normative design charter for the `gmeow:logic` upper ontology. This is the **charter**
> member of the [GMEOW Logic document set](LOGIC.md#the-document-set): it states what the
> foundation *is*, the documented predecessor weaknesses it refuses to inherit, and the greenfield
> primitives it declares. Vision and lineage are in [LOGIC.md](LOGIC.md); the formal account of
> every mechanism named here is in [LOGIC-SEMANTICS.md](LOGIC-SEMANTICS.md); how a reasoning request
> is configured is in [LOGIC-CONTRACT.md](LOGIC-CONTRACT.md); the typed intermediate representation
> is in [LOGIC-IR.md](LOGIC-IR.md); state-change semantics are in
> [LOGIC-TRANSACTION.md](LOGIC-TRANSACTION.md). Where this charter states a doctrine once, the
> semantics document makes it precise — repetition is replaced by cross-reference on purpose.
> The realized grounding catalogs for gUFO, BFO, OBO/RO, SUMO, OWL/RDFS, and SHACL Core/AF, plus
> the version-pinned DUL, IAO, PATO, YAMATO, and OpenCyc commitment-shifting bridge catalog, are
> recorded in [`docs/foundational-bridging.md`](../../../../docs/foundational-bridging.md) and
> consumed by the correspondence calculus ([`LOGIC-CORRESPONDENCE.md`](LOGIC-CORRESPONDENCE.md)).

## Thesis

`gmeow:logic` is a maximally precise, RDF 1.2-native foundational logic. It is built to encode the
distinctions other foundational systems collapse and to refuse the compromises those systems made
for a particular tool's convenience. The governing temperament is an *"Ithkuil for ontologies"*
ethos (named at length in [LOGIC.md § Ithkuil](LOGIC.md#design-influences-beyond-formal-logics--ithkuil)):
make the implicit explicit, factor meaning into orthogonal axes, and minimize ambiguity rather
than minimize symbol count. Where a tool's decidability ceiling or serialization assumption forced
a predecessor to drop a real distinction, `gmeow:logic` keeps the distinction and pushes the
compromise out to the projection boundary.

The foundation subsumes gUFO as a **minimum baseline**: every gUFO term has a `logic:` counterpart,
and the `logic:` term is at least as expressive as its gUFO image. It then transcends both gUFO's
and OWL 2's documented weaknesses — gUFO's because it is a deliberately lightweight OWL realization
of UFO that drops modal and higher-order content, and OWL 2's because OWL trades expressivity for
decidability by design. Decidability and tractability are therefore **projection and profile
guarantees**, never caps on the canonical model: a consumer *buys* a decidable view by projecting
down or by certifying a profile, exactly as Constitution **Principle 17** requires. gUFO is a
generated, lossy down-projection of the canonical foundation, validation-only where it constrains
without reasoning and a certified under-approximation only on the fragment OWL can faithfully hold.

The doctrine, stated once for the rest of the charter to reference:

> **GMEOW adopts gUFO as the floor and does not inherit its compromises.** gUFO is the *minimum* the
> foundation must cover, never the *ceiling* it may reach. Every gUFO restraint — the OWL 2 DL
> expressivity cap, the reification tax, the scaled-back event model, the external constraint stack —
> is a thing `gmeow:logic` is built to escape, not to reproduce.

## Criticism ledger

Each documented criticism of gUFO and OWL 2 is paired below with the `gmeow:logic` decision that
answers it. The point of the ledger is honesty: it records not just that a weakness is answered, but
*what shape* the answer takes in the canonical model.

### gUFO criticisms

| # | Criticism | The `gmeow:logic` decision |
|---|---|---|
| 1 | Reduced expressiveness — the OWL 2 DL ceiling strips UFO's modal and full-FOL axioms | Turing-complete `logic:`; rich modal / FOL / higher-order axioms are canonical; gUFO becomes a lossy projection |
| 2 | Triple bloat / heavy reification — must mint `gufo:Quality` / `Relator` / `Situation` nodes | Native edge properties via RDF 1.2 statement terms; flat-first, reify-on-demand |
| 3 | Minimalistic UFO-B — events and processes scaled back | Native 4D: perdurant / process / participation spine + temporal fluents + Principle-11 frame-relativity |
| 4 | High complexity / steep learning curve — dual taxonomy of sortals, kinds, phases, roles, mixins | Progressive disclosure (Principle 13): precise underneath, gentle flat on-ramp on top |
| 5 | OWL punning + a *secondary* constraint stack | First-class multi-level modeling via HiLog / F-logic reification; constraints built in — one integrated system |
| 6 | BFO/DOLCE niche — enterprise-software focus | Bridge views *by reference* (Principles 5 & 17): AI-memory home market + BFO (scientific) + DOLCE (cognitive) seams |

**1 — Reduced expressiveness.** The central, recurring criticism of gUFO is that it is "gentle"
UFO precisely because it removes what OWL cannot hold: the modal distinctions (rigidity is a modal
notion) and the higher-order types (identity supply quantifies over types) of full UFO. `gmeow:logic`
inverts the priority. The canonical foundation is authored in a Turing-complete logic where modal,
first-order, and higher-order axioms are first-class, and gUFO is the *output* of a generated
down-projection rather than the *input* the foundation imports. This is not a new commitment; it is
Constitution **Principle 17** applied to the foundational spine, and it is the load-bearing reason
every other entry in this ledger is even possible.

**2 — Triple bloat / heavy reification.** In gUFO, attaching a quality, a relationship, or a
contextual fact to an entity means minting an intermediate node — a `gufo:Quality`, a `gufo:Relator`,
or a `gufo:Situation` — and wiring several triples to it, even for the common case where no
statement-level metadata is needed. `gmeow:logic` makes the common case a **native edge property**:
metadata, temporal scope, and qualities ride directly on the RDF 1.2 statement that carries the
edge, using the same statement-term machinery the ABox already proves
([LOGIC-SEMANTICS.md § Triple terms](LOGIC-SEMANTICS.md#rdf-12-triple-terms-precisely)). This is
the Constitution's recurring **"flat-first, reify-on-demand"** pattern: a flat shortcut for the
common case, with a reified relator promoted only when period, role, confidence, or standpoint
actually matters. There is **no mandatory `Relator` or `Situation` node** for the common case; the
relator appears when it earns its keep.

**3 — Minimalistic UFO-B.** gUFO scales back the UFO-B account of events and processes to stay
manageable in OWL. `gmeow:logic` restores a **native four-dimensional** treatment: a full
perdurant / process / participation spine, temporal **fluents** whose truth varies across time, and
**Principle-11 frame-relativity** so that every temporal value lives in an explicit reference
system (a calendar + timescale frame) rather than as a bare literal. Events and processes are
first-class occurrents, not annotations bolted onto endurants.

**4 — High complexity / steep learning curve.** The standard objection to any UFO-derived ontology
is that its dual taxonomy — sortals versus non-sortals, kinds versus phases versus roles versus
mixins — is hard to learn and easy to misuse. `gmeow:logic` answers with **progressive disclosure**
(Constitution **Principle 13**): the full precision is available underneath, but a gentle, flat
on-ramp sits on top, and the deep machinery is discovered only at the moment it is needed. This is
the *Ithkuil paradox* made into a design rule — maximal precision is always *available* but never
*mandatory at minute one*. The author who only needs "a person is a kind of thing" never has to
name a rigidity policy; the author who needs cross-world rigidity has it waiting.

**5 — OWL punning and a secondary constraint stack.** gUFO leans on OWL punning (treating a class as
an individual) for the multi-level moves UFO requires, and it relies on an *external* constraint
stack for what OWL cannot express — two systems, loosely coupled. `gmeow:logic` makes multi-level
modeling **first-class** via HiLog / F-logic reification: types are reified as first-order
individuals so that "quantifying over types" is ordinary first-order quantification. The
`logic:suppliesIdentity` relation is exactly this move — a *second-order identity-supply relation
reified as a first-order relation* over punned type individuals. And constraints are **built in**, so
validation and inference are **one integrated system**, not a bolt-on second stack; multi-level types
validate natively rather than through an external bridge.

**6 — BFO/DOLCE niche.** gUFO's lineage is oriented toward enterprise software modelling, which
narrows its reach. `gmeow:logic` spans wider by treating the upper-ontology bridges as
**by-reference** alignments (Constitution **Principles 5 & 17**) rather than imports: its home
market is AI memory, it carries a **BFO bridge** for the scientific constituency, and it seams to
**DOLCE** for the cognitive one. None of these are truth-preserving projections — they carry
genuinely different ontological commitments — and the loss ledger records that honestly rather than
overclaiming a shared foundation.

### OWL 2 criticisms

| # | Criticism | The `gmeow:logic` decision |
|---|---|---|
| 7 | Global restrictions for decidability — a property may not be both transitive and asymmetric; property chains barred from cardinality restrictions | None here: `logic:properPartOf` is transitive ∧ asymmetric ∧ irreflexive — a strict partial order with full reasoning; decidability recovered by projection, not by crippling the canon |
| 8 | "Too much logic, not enough practical features" — no native string concat, no date/time arithmetic; forced into rule extensions | Native builtins, profile-gated; the Principle-12 line drawn explicitly between derivational builtins (in `logic:`) and heavy domain computation (external by reference) |

**7 — Global restrictions for decidability.** To stay decidable, OWL 2 DL imposes *global*
restrictions that have nothing to do with the modeller's intent: a property cannot be declared both
transitive and asymmetric, and property chains are barred from cardinality restrictions, among
others. These restrictions force a modeller to *weaken a correct model* to satisfy the reasoner.
`gmeow:logic` imposes none of them. `logic:properPartOf` is **transitive ∧ asymmetric ∧
irreflexive** — a combination that is illegal in OWL 2 DL but legal and fully reasoned here — giving
a genuine strict partial order (proper-part-of, ancestor) with complete entailment over it.
Decidability is recovered exactly as Principle 17 prescribes: by **projection or profile**, never by
mutilating the canonical model. A consumer who needs an OWL 2 DL view receives the legal downcast,
with the dropped characteristic recorded in the loss ledger.

**8 — Too much logic, not enough practical features.** A frequent practitioner complaint is that
OWL has rich classification but no *practical* computation: no native string concatenation, no
date/time arithmetic, no basic math — pushing modellers into bolted-on rule extensions that break
interoperability. `gmeow:logic` provides **native builtins**, gated to the `logic:ProceduralPrologProfile`
preset, and it draws the Constitution **Principle 12** line **explicitly**:

- **Derivational builtins live *in* `logic:`** — string concatenation, date arithmetic, and basic
  math, e.g. `legalAge = year(now) − year(birth)` or `fullName = firstName + " " + lastName`. These
  are the lightweight derivations a foundation genuinely needs and that forcing into an external
  engine would make absurd.
- **Heavy *domain* computation stays *external* by reference** — geo / datum transforms, RCC-8 /
  Allen relation-algebra composition, SLAM and trajectory updates, n-dimensional vector operations.
  These remain the solver-boundary concern Principle 12 keeps out of the reasoned core.

## Greenfield feature map

Where the criticism ledger says *what we refuse to inherit*, this map says *what we build instead* —
each greenfield feature paired with the primitive it declares.

1. **Native edge properties.** The RDF 1.2 statement-term doctrine carries metadata, temporal scope,
   and qualities directly on the edge (the answer to criticism 2).
2. **First-class multi-level modeling (goodbye punning).** A HiLog reification vocabulary lets types
   be quantified over as first-order individuals, with `logic:suppliesIdentity` as the worked
   example.
3. **Hybrid open/closed worlds, scoped.** `logic:WorldBoundary` and `logic:closedUnder` let a model
   declare *where* the closed-world reading applies — the closed-world lane co-resident with the
   open-world `logic:` lane.
4. **Native spatiotemporal (4D) and fluents.** The perdurant / process spine, `logic:Fluent`, and
   the frame seam give a real UFO-B (the answer to criticism 3).
5. **Tractable, parallelizable, neuro-symbolic.** The certified-fragment presets supply the tractable
   lanes — Datalog / Horn rule sets are PTIME and parallelizable — and the `logic:probability` /
   `logic:confidence` axes anchor the neuro-symbolic split: ML-approximate *classification* alongside
   symbolic *invariant enforcement* (see
   [LOGIC-SEMANTICS.md § The reasoning contract](LOGIC-SEMANTICS.md#the-reasoning-contract)).
6. **Integrated algorithmic / string primitives.** A `logic:Builtin` registry, gated to the
   `logic:ProceduralPrologProfile` preset (the answer to criticism 8).

## Typed and contextual mereology, and holons

A foundation that allows the full strength of OWL's barred combinations must also be far more careful
about *which* mereological axioms fire *where*. The single global parthood relation other
foundations assume — over which weak supplementation, extensionality, and transitivity hold
unconditionally — is itself one of the collapses `gmeow:logic` refuses.

**Parthood is profiled, not universal.** Weak supplementation (a whole with a proper part has another
part disjoint from the first) is a strong axiom that is true of *functional complexes* and
*quantities* but false of many legitimate wholes — singleton-membered collectives, abstract
aggregates, in-progress assemblies. `gmeow:logic` therefore scopes supplementation to a declared
**MereologyProfile**: the axiom holds *within* the profile a whole is declared under, never across
all parthood. And the prerequisites are defined first: **overlap** (sharing a part) and
**disjointness** (sharing none) are primitive notions that must be settled before supplementation can
be stated at all, so the axiom is never applied to wholes for which overlap is undefined.

**Holon-ness is contextual, not a unary type.** Whether an entity is a holon — simultaneously a whole
and a part — is never a bare property of the entity; it is a position it occupies in a particular
holarchy, under a particular context, over a particular interval, along a particular path. The
canonical construct is therefore a relational **HolonicPosition**(*entity, holarchy, context,
interval, path*). The familiar unary "holon" is a **projection** of that position, convenient and
lossy, exactly as the standpoint-modality view (below) is a projection of finer axes.

- **Levels are path-relative.** Holarchies are DAGs, not strict ladders, so an entity has no single
  global "level." A level is meaningful only relative to a path through the holarchy; two paths may
  place the same entity at different depths, and both are correct. The construct is concrete:
  `logic:holonicLevel` is a literal **read off a `logic:HolonicPosition`** (`rdfs:domain
  logic:HolonicPosition`), never an intrinsic attribute of the entity — because a position fixes one
  path, the level it carries is automatically per-path. The per-entity **min/max band** across an
  entity's positions is materialized as `logic:holonicLevelMin` / `logic:holonicLevelMax`, and the
  derived marker `logic:multiplyPositioned` (an entity occupying two or more distinct positions, by
  negation-of-equality over the position set) certifies that a non-trivial band exists — the positive
  companion to `logic:HolonicLevelIncoherence`. **Honest scope (ME9):** the foundation chase is
  all-IRI under `logic:StratifiedNAFProfile`, with no numeric comparison or aggregation, so it grounds
  only the band's *existence* (`logic:multiplyPositioned`); it can neither derive nor check that the
  band endpoints are the true extrema of the borne per-position levels — those endpoints are
  operator-asserted, not engine-verified. The single source remains the per-position `logic:holonicLevel`
  literals; the band is a materialized convenience. Because the native foundation evaluator hard-rejects
  non-IRI (literal) objects, the level literals and the band live OUTSIDE the foundation conformance
  world: the `holonic-band` case (below) proves the engine-verified path-relativity structure IRI-only,
  while the literal band is dogfooded as ABox data in `examples/holonic-band.ttl`.
- **Emergence is assessed, not asserted.** Emergence is an **EmergenceAssessment** relative to a
  declared *reduction theory* — the claim that a whole's property is not derivable from its parts
  *under that theory*. Failure to derive is **not** proof of irreducibility; the assessment records
  the theory it is relative to so a later, stronger theory can overturn it without contradiction.
  The construct is concrete: `logic:EmergenceAssessment` is reified with the role properties
  `logic:assessmentWhole`, `logic:assessmentProperty`, and `logic:assessmentReductionTheory`, and the
  engine *derives* its `logic:assessmentVerdict` — one of the three closed `logic:EmergenceVerdict`
  values `logic:Aggregate`, `logic:Emergent`, `logic:EmergenceUnknown`. A `logic:ReductionTheory`
  carries, via `logic:reductionBasis`, the property-values it treats as part-reducible, and entities
  carry property-values through `logic:bearsProperty`. The verdict is computed by five stratified
  rules — a derivation-grounded marker plus an OWL-projectable `logic:assessmentVerdict` for each of
  **Aggregate** and **Emergent**, and the single **EmergenceUnknown** projection: a whole-property is
  **Aggregate** when the declared theory's basis carries it *and* a proper
  part bears it (a genuine part-reconstruction); it is **Emergent** by negation-as-failure over that
  aggregate derivation *while the assessment still binds a declared theory* — so the verdict is
  theory-relative, never a bare "unflagged" default (the failure mode the foundation explicitly
  guards against); and it is **EmergenceUnknown** when the whole bears the property but the assessment
  declares no reduction theory, so the reducibility question cannot even be posed. Crucially, **no
  rule propagates `logic:bearsProperty` down `logic:properPartOf`**, so an emergent whole-property
  never reaches its parts and is never entailed by the parts' properties — non-inheritance is a
  structural guarantee of the rule set, demonstrated positively in the holonic-emergence conformance
  case (present `bearsProperty(whole, Pv)` + present `properPartOf(part, whole)` + visibly *omitted*
  `bearsProperty(part, Pv)`). The minimal case lands here; the full emergence corpus and the lossy OWL
  projection of the verdict accrete in C5.
- **Downward constraint is structured and non-transitive.** A whole may constrain its parts, but the
  constraint is a typed, directed relation that does **not** chain transitively by default; a
  constraint from level *n* onto level *n−1* says nothing automatic about level *n−2*. The construct is
  concrete and mirrors the emergence calculus above, but for governance flowing *down* rather than
  reducibility flowing *up*: `logic:DownwardConstraint` is reified with the role properties
  `logic:constraintWhole`, `logic:constraintTarget`, `logic:constraintState`, `logic:constraintRegime`,
  and `logic:constraintOverride`, and the engine *derives* its `logic:constraintVerdict` — one of the
  three closed `logic:ConstraintVerdict` values `logic:ConstraintBinding`, `logic:ConstraintOverridden`,
  `logic:ConstraintUnknown`. This is downward **constraint, never material causation**: the governing
  whole *bounds the permissible role/state* of a named proper part, it does not produce or compose the
  part. The constrained `logic:constraintState` is typically the part's functional role or
  `gmeow:Goal` set in the context of its super-whole (the teleology slice, linked via `rdfs:seeAlso`) —
  the downward face of holonic governance is precisely that a part's teleology is fixed relative to the
  holon it serves. Governance is **regime-relative**: a `logic:GovernanceRegime` carries, via
  `logic:activationBasis`, the constrained states it activates as binding, exactly as a
  `logic:ReductionTheory` carries its `logic:reductionBasis` — the same whole may bind a part's state
  under one regime and leave it unknown under another, and a binding can be quieted by a different
  regime without contradiction. The verdict is computed by five stratified rules in the same
  marker→projection shape as emergence: a constraint is **ConstraintOverridden** (the positive,
  derivation-grounded verdict) when it names an override and the target *bears* that declared token
  (`logic:bearsProperty`); it is **ConstraintBinding** by negation-as-failure over that override
  derivation *while the constraint still binds a declared regime whose basis activates the state* — so
  the verdict is regime-relative, never a bare "unconstrained" default; and it is
  **ConstraintUnknown** when the named target is a proper part of the whole but no regime activates the
  constrained state, so the binding question cannot even be posed (the first-class third value;
  failure-to-activate is not constraint). The override settles below the binding NAF (stratum 1 vs
  stratum 3), so an overridden constraint is never read as binding. Crucially — and this is the C3
  analogue of emergence's non-inheritance — **no rule cascades the constraint down
  `logic:properPartOf`**: every verdict rule is gated on an explicit `logic:constraintTarget`
  reification, so a constraint onto a part says nothing automatic about that part's own sub-parts.
  Non-transitivity is therefore a **structural guarantee robust by construction**, not incidental to
  the EDB-only treatment of `properPartOf`: even were the transitive closure of `properPartOf`
  materialized, no verdict could attach to an entity that no `logic:DownwardConstraint` names. It is
  demonstrated positively in the holonic-governance conformance case (a grandchild that is a transitive
  proper part of the governing whole, targeted by no constraint, carries *no* `logic:constraintVerdict`
  in the golden `materialized.nq`). The minimal case lands here; the full governance corpus and the
  lossy OWL projection of the verdict accrete in C5.
- **Autonomy/integration is a named profile.** The Koestlerian balance of part-autonomy against
  whole-integration is a **declared profile** a holarchy may adopt, not a universal well-formedness
  rule every holon must satisfy — a bolt, a file-segment, or a process-phase needs no autonomy, so
  the integrity calculus runs only where it is declared. The construct mirrors the emergence (C2) and
  governance (C3) calculi exactly. Koestler's holon is **Janus-faced**: it carries a *self-assertive*
  tendency (autonomy, as a whole) and an *integrative* tendency (subordination, as a part). These are
  **co-equal vantage facets** under Principle 9 — neither is the primary face — and the foundation
  refuses to privilege one over the other in either the vocabulary or the rule firing order. The
  construct is concrete: `logic:AgencyAssessment` is reified with the role properties
  `logic:agencyHolon` and `logic:agencyProfile`, and the engine *derives* its `logic:agencyVerdict` —
  one of the four closed `logic:AgencyVerdict` values `logic:HolonIntegral`, `logic:AutonomyDeficient`,
  `logic:IntegrationDeficient`, `logic:AgencyUnknown`. Agency is **profile-relative**: a
  `logic:HolonicAgencyProfile` carries, via `logic:selfAssertiveBasis` and `logic:integrativeBasis`,
  the property-values that respectively evidence each tendency, exactly as a `logic:ReductionTheory`
  carries its `logic:reductionBasis` and a `logic:GovernanceRegime` its `logic:activationBasis`; an
  entity carries those values through `logic:bearsProperty`, dogfooding the same bearer relation the
  emergence calculus uses. The verdict is computed by six stratified rules in the same
  marker→projection shape: **two co-equal positive markers** settle first — `logic:selfAssertive`
  (the holon bears a self-assertive basis value) and `logic:integrative` (it bears an integrative
  basis value), built by identical rules so neither facet is privileged; a holon is **HolonIntegral**
  when *both* markers hold (the positive, derivation-grounded verdict); it is **AutonomyDeficient** —
  the first Koestlerian pathology, a "part" with no autonomy — by negation-as-failure over the
  self-assertive marker *while the integrative marker holds*, and symmetrically **IntegrationDeficient**
  — the second pathology, a "whole" refusing to integrate — by NAF over the integrative marker while
  the self-assertive marker holds; and it is **AgencyUnknown** when *neither* marker fires, so the
  integrity question has no positive footing. The two pathology rules are mirror images, settling in
  the same stratum, so the duality is genuinely co-equal rather than one tendency defaulting to the
  other. `AgencyUnknown` is the first-class fourth value (the C4 analogue of `EmergenceUnknown` /
  `ConstraintUnknown`): it subsumes the "cannot pose the question" case, firing both when the holon
  bears no basis value and when the profile declares no basis at all — failure-to-evidence is not
  integrity, and a basis-free profile is *unknown*, not deficient. Crucially, **every verdict rule
  re-binds `logic:agencyHolon` and `logic:agencyProfile`** as a well-formedness existence guard, so a
  malformed assessment (naming no holon, or no profile) provably receives *no* verdict — robustness by
  construction, the C4 analogue of C3's per-target gating. The minimal case lands here (dogfooding C1's
  holon kernel — the assessed holons are wired into a holarchy so `logic:isHolon` co-fires); the full
  agency corpus and the lossy OWL projection of the verdict accrete in C5.
- **C5 addendum — holonic conformance corpus, level-coherence rule, and OWL projection.**
  The seven holonic conformance cases now live under `conformance/logic/cases/holonic/`:
  `holarchy-closure`, `weak-supplementation`, `emergence`, `downward-constraint`,
  `holon-integrity`, `holonic-level`, and `holonic-band`. Every case is validated against a
  derivation-graph golden under the native solver, and the Rust test suite asserts golden
  quad-set parity for each single-world case. `holonic-band` (ME9) is the positive
  path-relativity proof: a holon occupying two `logic:HolonicPosition`s along two distinct
  `logic:positionPath`s on a genuine multi-parent DAG fires the engine-derived
  `logic:multiplyPositioned` (with `logic:isHolon` / `logic:hasHolonicPosition` and, because it
  occupies positions, NO `logic:HolonicLevelIncoherence`). It is IRI-only — the literal
  `logic:holonicLevel` values and the min/max band the all-IRI evaluator cannot hold are
  materialized as ABox data in `examples/holonic-band.ttl`.

  The **holonicLevel coherence rule is position-based**: because the foundation chase is
  all-IRI and `logic:holonicLevel` is a literal read off a `logic:HolonicPosition`, coherence
  is keyed on the IRI-valued canonical construct. A holon declared under a
  `logic:MereologyProfile` that occupies no `logic:HolonicPosition` is charged with
  `logic:HolonicLevelIncoherence` (profile-scoped); a holon outside any profile is
  never charged. **Non-conflation**: `logic:instanceOf` and `logic:orderedType` (the HiLog
  instantiation tower) do not supply a holonic position, so a profiled holon high in the
  instantiation tower but lacking a `logic:HolonicPosition` still fires the incoherence
  verdict — the tower and the position are orthogonal constructs.

  The **lossy OWL projection** (Principle 17 / Principle 4): `logic:Holon`,
  `logic:HolonicPosition`, and `logic:Holarchy` project to `owl:Class`;
  `logic:properPartOf` projects to `owl:ObjectProperty` with `owl:TransitiveProperty`. The
  strict-order characteristics (asymmetric + irreflexive), the five-place
  `logic:HolonicPosition` relation, and the WeakSupplementation axiom are **not** lowered —
  these losses are recorded in `projection-report.ttl`.

This is the Ithkuil discipline applied to systems theory: the loose word "holon" is decomposed into
the position, the path, the reduction theory, and the profile that the word silently conflates.

## Factored claim modality

The earlier model gave a claim a single five-valued standpoint-modality (`unequivocal`,
`conceivable`, `refuted`, `probable`, `bullshit`). Those five values are real, but they are not a
single axis — they bundle several independent decisions into one token, which is precisely the
collapse the foundation exists to prevent. `gmeow:logic` **factors modality into orthogonal axes**,
each of which a claim selects independently:

- **Polarity** — affirm, deny, or suspend the propositional content.
- **Modal force** — necessary, actual, possible, or counterfactual (the alethic dimension).
- **Credence** — an agent's graded degree of belief in the content. Kept distinct from
  *confidence* (a source's or report's confidence in a statement), from *probability* (a quantity
  in a probabilistic model), and from *weight* (a solver ranking) — these are separate axes, not
  one.
- **Assertoric force** — assert, conjecture, assume, or retract: the *act-level* commitment, not the
  content.
- **Truth-directedness** — truth-aimed, truth-indifferent, or strategic: whether the claim is even
  *trying* to track truth.
- **Support-status** — supported, defeated, or undetermined under the prevailing epistemic standard.

The single five-value standpoint-modality survives only as a **generated convenience view** over
these axes — the nearest named bundle, produced for surfaces that want one token, never the canonical
form. Two consequences fall out cleanly once the axes are separated:

- **Bullshit is truth-indifference, not a truth value.** It lives on the *truth-directedness* axis —
  a claim made with no regard for whether it is true — and is therefore orthogonal to whether it
  happens to be true, deniable, or refuted. Treating it as a fifth truth value (as the bundled
  modality had to) was the collapse.
- **Refutation is a standpoint's committed denial, not terminal truth.** It is *polarity = deny* held
  *with assertoric force* by a standpoint — a first-class, world-indexed claim that can itself be
  contested — never a global "this proposition is false" verdict.

## Proposition, claim token, attitude, and evaluation

The most consequential separation the foundation makes is among four things the word "claim"
routinely conflates. Keeping them apart is what lets one proposition be asserted by many sources, one
act express several propositions, and a retraction withdraw a token without erasing content.

- **Propositional content** — the *what* that is claimed: a truth-apt content, abstract, sourceless,
  and timeless. The same content can be entertained, asserted, denied, and feared by different agents
  at different times.
- **Claim / assertion act (claim token)** — a *particular* act of putting content forward, by an
  agent, at a time, under an assertoric force. A token is an event with provenance; it can be
  retracted, and its retraction is itself a recorded act that leaves the content untouched.
- **Held attitude (doxastic state / intentional mode)** — the standing relation of an agent to a
  content: believing, doubting, intending, hoping. An attitude need not be expressed by any token,
  and a token need not reflect the agent's true attitude — the gap between the held and the projected
  is exactly what the deception apparatus reads.
- **Evaluation** — a judgment *of* a content, token, or attitude against some standard: true/false,
  warranted/unwarranted, sincere/insincere. The evaluation is a separate claim with its own
  standpoint and provenance.

Because these are four constructs and not one, the relationships are many-to-many by construction: a
proposition is asserted by many tokens; a token may express several propositions; an attitude may
ground many tokens or none. The universal "observation" structure that earlier unified all reporting
becomes a **projected union view** over these four — a convenience surface, generated, never the
canonical record.

## Argumentation and epistemic standards

Claims do not stand alone; they support and attack one another, and "knowledge" and "justification"
are not the undifferentiated primitives informal usage treats them as. `gmeow:logic` gives both a
factored account.

**Arguments carry typed attacks.** An argument is content plus the relations it bears to other
arguments, and an attack is never generic — it names *what* it attacks:

- **undermine** — attack a premise (deny something the argument assumes);
- **undercut** — attack the warrant (grant the premises but deny that they support the conclusion);
- **rebut** — attack the conclusion directly (argue for its contrary).

Which arguments survive is decided under a **named acceptability semantics** (the Argumentation facet
of [LOGIC-CONTRACT.md](LOGIC-CONTRACT.md)), and **support accrues**: several independent supports for
a conclusion combine rather than counting once.

**Knowledge is locally factive.** Knowing-that is factive *relative to a world and an epistemic
standard*: knowing *P* in a world entails that *P* holds *in that world*, never that *P* holds
globally across all worlds. This keeps factivity honest in a paraconsistent, world-indexed setting —
an agent can know contested facts in its own world without the model claiming them everywhere.
**Non-factive knowledge-attribution** — "they take themselves to know" — is a *separate* claim about
an attitude, never collapsed into the factive relation.

**Justification is not one thing.** It splits into independent components a single word usually
hides:

- **available evidence** — what the agent has access to;
- **basing** — which evidence the belief is actually founded on (available ≠ used);
- **support under a standard** — how strongly the evidence warrants the content *relative to a named
  epistemic standard*;
- **adequacy** — whether that support meets the standard's threshold;
- **defeat** — whether a defeater has since undermined or undercut the support.

**Deductive steps are contract-local indefeasible.** A deductive inference is indefeasible *within a
contract* — granted its premises and the applicability of its rule, the conclusion cannot be defeated
— yet **premise-acceptance and rule-applicability remain defeasible**: the step is sound, but whether
you should have taken it is still open to challenge. **Inference-to-best-explanation preserves the
field**: ranked and tied hypotheses are carried with their ranking intact; the runner-up is never
auto-suppressed, so a later defeater of the leader can promote the next without re-deriving.

## Typed formalization governance

A foundation this expressive could turn every prose sentence into an axiom, and that is a hazard, not
a feature: an over-eager formalization asserts more than the design intends and contaminates the
reasoned core. `gmeow:logic` therefore governs the prose-to-axiom transition explicitly. A prose
constraint becomes a formal axiom **only** by passing through a **FormalizationCandidate** lifecycle,
in which the candidate is *typed by category* and reviewed before it is admitted as canonical. The
categories are themselves a precision instrument — they record what *kind* of force the prose was
meant to carry. They are an **eleven-member closed set** (`logic:FormalizationCategory`); each is a
distinct atomic kind, never collapsed (Principle 9), so that "necessary" and "sufficient" — and
"defeasible default" and "typicality" — are each their own category rather than a bundled pair:

- **equivalence-definition** — necessary-and-sufficient identity of a term;
- **necessary-condition** — a one-directional necessary condition;
- **sufficient-condition** — a one-directional sufficient condition;
- **integrity-constraint** — an integrity condition whose violation is a finding, not a derivation;
- **derivation-rule** — a productive rule whose head is entailed;
- **defeasible-default** — defeasible content that a more specific fact may override;
- **typicality** — generic, what-normally-holds content (distinct from a default's override force);
- **recommendation** — advisory, never enforced;
- **non-entailment-obligation** — a deliberate *non*-assertion (see below);
- **deliberate-overlap** — a mereological sharing fact deliberately admitted;
- **documentation-only** — prose that is explanatory and is *not* to be formalized at all.

A candidate additionally carries a **semantic-risk class** (`logic:SemanticRiskClass`, a closed
set) recording how far a wrong formalization would propagate: `core-contaminating` (a bad axiom
would corrupt the reasoned core), `projection-only` (the blast radius is confined to a lossy
projection surface), or `advisory` (no entailment consequence at all). The risk class is what makes
the reviewer gate proportionate — a `core-contaminating` candidate demands the strictest review, an
`advisory` one the lightest — and it is recorded explicitly rather than left implicit (the
refuse-to-collapse ethos applied to review effort).

Crucially, **deliberate non-assertions are first-class and executable.** A `NonEntailmentObligation`
records a conclusion the foundation must *never* draw. Because the canonical logic is semi-decidable,
however, "absence of a proof" is not generally decidable — claiming to have *proved* that the engine
does not derive a forbidden fact would be an overclaim. A non-entailment obligation is therefore
**conclusively discharged only when** one of the following conditions holds:

- the check runs within a **certified complete fragment** (e.g., a Datalog stratum or EL profile over
  which the chase terminates and is complete);
- a **finite closure** is available (the materialized derivation graph is complete and does not
  contain the forbidden predicate);
- a **syntactic dependency / reachability analysis** over the rule heads demonstrates that the
  forbidden predicate is unreachable — no rule head unifies with it, directly or through any chain
  of rule applications;
- a **conservative-extension** proof establishes that the added axioms cannot introduce the forbidden
  conclusion; or
- the obligation is **explicitly bounded** to a declared corpus or mutation-test space, and
  exhaustive checking within that bound is complete.

Outside these conditions the discharge result is **`unknown` / `not-discharged`**, never
"proved absent." The obligation remains active and is carried forward to the next evaluation cycle
or stronger fragment.

Two standing examples:

- **intent must not be derived from structural deception** — the presence of a held/projected gap is
  *evidence*, never *entailment*, of deceptive intent; the foundation is obligated not to close that
  inference automatically. This obligation is enforced by **both**: (1) static taint and reachability
  analysis over rule heads, which verifies that no rule head chain can unify with the deceptive-intent
  predicate from structural-gap premises alone; and (2) adversarial conformance fixtures that
  actively attempt to produce the forbidden conclusion against the live rule set. Both checks must
  pass; either failure triggers an obligation violation.
- **counterpart must not become transitive** — cross-world counterpart identity must remain
  non-transitive; the foundation is obligated to block the chaining a careless rule would introduce.
  This is dischargeable by syntactic reachability over the counterpart rule heads.

A non-entailment obligation is not a comment that the axioms happen to respect a constraint; it is
an executable obligation the foundation is held to, with a declared discharge condition. The
*absence* of an inference is as machine-checked as its presence — within the stated fragment or
bound, and carried as `unknown` everywhere else.

This machinery **is** the foundation's guard against over-typing (Principle 9's refusal to
over-assert). A candidate whose formalization would entail a deliberately withheld conclusion —
one touching a `NonEntailmentObligation`'s forbidden predicate — is surfaced as
`logic:ObligationViolated` and returned to review, never silently asserted into the reasoned core
nor silently skipped. The review of an over-typing collision *is* the non-entailment-obligation
check (Arm A syntactic reachability over the rule heads, Arm B finite closure over the derived
edges), realized through the typed candidate lifecycle rather than as a separate advisory flag: the
category `CategoryNonEntailmentObligation` records the intent, the linked obligation names the
forbidden predicate, and the two arms enforce it.

The deliberate-non-assertion boundaries the foundation commits to are each recorded as a reviewed
candidate flagged `logic:candidateDeliberateNonAssertion`, so the whole set is enumerable rather
than tacit — queryable as one report over the accepted candidates regardless of their
`logic:candidateCategory` (the flag cuts across the category facet, so an integrity-constraint
boundary is enumerated alongside the obligations rather than lost among ordinary integrity
constraints). The four enforced boundaries each carry a caught-violation fixture that proves the
boundary holds against a live chase; the fifth — the deliberately-preserved overlap — carries a
positive coherence case and its countermodel instead, since an allow-by-design non-assertion has
no runtime violation to catch:

- **deceptive intent is attributed, never entailed**, and **cross-realm counterpart identity never
  becomes transitive** — two `CategoryNonEntailmentObligation` candidates, each carrying its
  standing `NonEntailmentObligation` (the forbidden predicate is `gmeow:deceptiveIntentClaim` /
  `gmeow:counterpartOf`), discharged by the two arms above;
- **a measured value is frame-relative and ill-formed without its reference frame** (Principle 11),
  and **an instance carries at most one ultimate Kind** (Principle 9) — two `CategoryIntegrityConstraint`
  candidates that wrap the measurement-frame and identity-overlap (`MixIden`) disciplines whose
  `logic:violation` materialises the finding;
- the **`SocialObject` ∩ `InformationObject` overlap is deliberately preserved** — a
  `CategoryDeliberateOverlap` candidate that records the reviewed decision *not* to assert disjointness.

The proposed notion of a standalone over-typing-review flag is subsumed by this lifecycle: the review
of an over-typing collision is the obligation or discipline check itself, surfaced through the typed
candidate, never a separate advisory property.

The lifecycle is a **closed four-state machine** (`logic:CandidateLifecycleState`):
`proposed` → `under-review` → `accepted` or `rejected`. An extraction — an LLM's most of all —
*always* enters at `proposed`; `accepted` (the canonical, axiom-bearing state) is reachable only
through `under-review` with a recorded reviewer decision, never directly. A candidate asserted as
`accepted` without that review is a checked governance violation, not a soft warning. Each obligation
check records a **discharge verdict** from a closed three-value set (`logic:DischargeVerdict`):
`discharged`, `unknown` (carried forward, never "proved absent"), or `violated`. Of the five
discharge conditions above, the foundation engine wires two — **syntactic reachability** over the
rule strata and **finite closure** over the materialized derivation graph — because those are the
conditions the two standing obligations rest on. The other three (certified-fragment,
conservative-extension, bounded-corpus) are named in the closed set but not yet engine-backed; an
obligation that declares one of them is a hard error ("no executable discharge path"), so an unwired
condition can never be mistaken for a silent pass.

## Conjecture and refutation

The typed-formalization surface above governs the *static* prose-to-axiom transition: a reviewer
asserts a candidate's positive and negative cases and adjudicates its promotion. The conjecture
library is that surface's **dynamic, testable specialization**. A `logic:Conjecture` is a
`logic:FormalizationCandidate` (`rdfs:subClassOf`), so it inherits the whole governance spine — all
eight universal carriers (source hash, extraction provenance, scope, reasoning contract, formalization
category, candidate lifecycle, projection behaviour, semantic risk) still apply. What *specialises* it
is one genuinely new axis: a conjecture's cases are **engine-produced**, not reviewer-asserted. That
axis is carried explicitly by `logic:verdictProvenance` over the closed
`logic:VerdictProvenanceKind` set (`VerdictEngineProduced` / `VerdictReviewerAsserted`), so an
engine-derived verdict and a hand-adjudicated one are never confused.

**Symmetric test.** A conjecture names a candidate formula (its `logic:conjectureFormula`, the
alpha-normalised content identity of the full first-order AST) and tests it symmetrically against its
**constructed strong negation** `¬φ` as two *independent* legs: a **support-for-`φ`** leg (the standpoint's
KB entails `φ` — the candidate is redundant given the KB) and a **support-for-`¬φ`** leg (the KB refutes
`φ`). The negation is genuinely constructed, not merely gestured at; for the `∀`-Horn case `φ = ∀x. body → head`
the negation `¬φ = ∃x. body ∧ ¬head` is existential and chase-inexpressible to *lower*, yet is decided
soundly **and** completely without lowering it, because `KB ∪ {φ} ⊨ ⊥ ⟺ KB ⊨ ¬φ`: asserting the rule
and detecting the `owl:Nothing` clash materialises the body-instance witness that forces the head false.
Because the two legs are independent, the four Belnap quadrants are all reachable — in particular a KB
that entails `φ` (the `φ` leg fires) *and* refutes it (the `¬φ` leg fires) yields the glut `Both`. That
co-support is a within-standpoint contradiction **localised to the candidate proposition** (the base
entails `φ` while its disjointness / negative-property axioms refute `φ`); it is a genuine, testable
refutation carrying a `logic:ContradictionWitness`. A base contradictory for reasons *unrelated* to the
candidate — one that neither entails `φ` nor genuinely refutes it — is a hard error instead: *ex falso*
would make every proposition both entailed and refuted, so no meaningful test can run against it.
Because the canonical logic is standpoint-relative, the verdict is **always scoped to a reified standpoint**
(`logic:conjectureStandpoint`, a required IRI). Standpoint scoping here is reification, never a named
graph, and it is load-bearing: Principle 9 refuses a global-false verdict, so a refutation is always
"the formula is refuted *from this standpoint*", never simpliciter.

**Two orthogonal lifecycles.** A conjecture carries two lifecycle axes that Principle 9 forbids
collapsing. The **governance** lifecycle it inherits (`logic:candidateLifecycle`:
proposed / under-review / accepted / rejected) records *who has adjudicated its promotion*. The
**epistemic** lifecycle (`logic:conjectureLifecycleState`: open / corroborated / refuted-in-standpoint /
withdrawn) records *what the test found*. A conjecture may be governance-accepted yet epistemically
open, or epistemically corroborated yet governance-proposed; the two never fuse into one status field.

**The Belnap-to-lifecycle-and-discharge projection.** The engine returns a Belnap truth value for the
formula from its standpoint; conclusiveness reuses the existing `logic:DischargeVerdict` value class
(carried as `logic:conjectureDischargeVerdict`) rather than minting a parallel notion. The projection
is total:

| Belnap origin | Epistemic lifecycle | Discharge verdict | Witness |
| --- | --- | --- | --- |
| Supported (true) | `ConjectureCorroborated` | `ObligationDischarged` | — |
| Opposed (false) | `ConjectureRefutedInStandpoint` | `ObligationDischarged` | **required** |
| Both (contradictory) | `ConjectureRefutedInStandpoint` | `ObligationDischarged` | **required** |
| Neither, conclusive | `ConjectureOpen` | `ObligationDischarged` | — |
| Undetermined / budget-exhausted | `ConjectureOpen` | `ObligationUnknown` | — |
| (author action) | `ConjectureWithdrawn` | — | — |

Two facts about the table are enforced, not merely documented. First, **a timeout is never a
refutation**: an undetermined or budget-exhausted run carries `ObligationUnknown` and *must* stay in
`ConjectureOpen` — a conjecture carrying `ObligationUnknown` in any other state is a hard error. The
"Neither, conclusive" row (a genuine independence proof inside a complete fragment) is distinguished
from it purely by the discharge verdict: same open state, `ObligationDischarged` vs `ObligationUnknown`.
Second, **a refutation must be replayable**: a `ConjectureRefutedInStandpoint` conjecture *must* name a
concrete `logic:ContradictionWitness` — the individual forced to `owl:Nothing`, the world it was found
in, and each jointly-inconsistent premise as a serialized triple. A refutation with no witness is an
unfalsifiable claim, not a refutation, and fails both the SHACL shape and the verify query. Withdrawal
is the one state that is never engine-produced: it is an author action, and a withdrawn conjecture
carries `VerdictReviewerAsserted`.

**Two symmetric promotion legs.** A conjecture that survives feeds forward, and so does one that dies.
A corroborated formula becomes eligible for the **positive** leg: `logic:conjecturePromotionCandidate`
points to a `logic:FormalizationCandidate` proposing to promote the formula to a canonical axiom —
still gated through the ordinary reviewer decision, because corroboration is provisional support, never
proof. A robustly refuted formula feeds the **symmetric anti-conjecture** leg:
`logic:antiConjectureObligationCandidate` points to a candidate `logic:NonEntailmentObligation`
forbidding the formula. A refuted formula is not merely discarded; it becomes a first-class commitment
that the foundation must never draw. The two legs mirror each other exactly under negation —
corroborated → axiom candidate, refuted → non-entailment candidate.

**Lakatos refinement.** A refutation need not end the inquiry. A weakened successor conjecture can link
back to its refuted predecessor through `logic:conjectureRefinedFrom` and forward to the witness it now
excludes through `logic:survivesCounterexample` — the "monster-barring" move, made auditable: the
corpus can show a refinement chain in which each successor demonstrably survives a counterexample that
killed its parent, rather than silently relabelling the claim.

**The bounded corpus pre-order.** Conjectures relate to one another through `logic:conjectureEntails`
(the formula φ entails the formula ψ). Its purpose is contrapositive propagation: if ψ is refuted and
φ entails ψ, then φ cannot stand and belongs on the re-test frontier. The relation is a pre-order *in
intent* (reflexive and transitive), and each edge is itself a conjecture test — that φ genuinely
entails ψ — not a bare assertion. This layer is **vocabulary plus the propagation competency
query**: the pre-order is carried edge by edge and queried directly, and its full transitive closure
over the corpus is not materialized as a stored relation — contrapositive propagation reasons over the
edges without the engine computing that closure as a persisted extension.

Corroboration rank and the AGM **entrenchment** ordering are kept independent by design. Corroboration
rank is a natural feed into entrenchment — the more a conjecture has survived, the more entrenched the
belief it grounds could be, and the more costly to give up under a counterfactual revision — but the
conjecture layer does not couple the two: corroboration is recorded on the conjecture and does not alter
the entrenchment that governs counterfactual revision. Coupling them would change the resolution
semantics of every counterfactual test at once, so the two orderings remain separate and the conjecture
vocabulary carries no entrenchment side effect.

## The Ithkuil precision ethos

The clearest proof that `gmeow:logic` already practises its own ethos is the precision it carries in
the four quantitative axes. A calibrated probability, an asserter's confidence, a solver ranking
weight, and an evidential warrant are **four genuinely distinct things**, and a common failure mode
of probabilistic knowledge graphs is collapsing them — treating arbitrary confidence metadata as if
it were a probability model. `gmeow:logic` keeps them apart as
`logic:probability ≠ logic:confidence ≠ logic:weight ≠ logic:evidenceStrength` (the governing rules
are in
[LOGIC-SEMANTICS.md § Confidence, probability, weight, and evidence](LOGIC-SEMANTICS.md#confidence-probability-weight-and-evidence)).
It draws a second distinction the same way: **determinacy ≠ confidence** — whether a value is
ontically crisp, vague, fuzzy, probabilistic, or disputed is held separate from *how sure anyone is
about it*, exactly as Constitution **Principle 9** demands (`gmeow:Determinacy` versus
`gmeow:confidence`).

The ethos extends naturally outward, and the sections above are its application: factored claim
modality refuses to collapse six axes into one token; the proposition/token/attitude/evaluation split
refuses to collapse four constructs into "claim"; typed mereology refuses to collapse profile-local
supplementation into a universal axiom. Just as the quantitative axes refuse to collapse four numbers
into one, the foundation refuses to collapse **configuration / affiliation / perspective** into a
coarse grouping: mereology distinguishes the *collective*, the *count*, and the *quantity* readings
of a whole, and standpoint / vantage keep "from whose perspective" explicit rather than implied. The
single governing principle, stated for the charter:

> **Encode every distinction explicitly; refuse to collapse what is genuinely distinct.** Where two
> notions can come apart in any model the foundation must serve, they are two terms, not one.

This is Ithkuil's orthogonal-factorization lesson turned into a foundational design rule, with its
fatal flaw engineered out: the precision is always *available*, but progressive disclosure
(Principle 13) means it is never *mandatory* before it is needed, and the named convenience views
(the bundled standpoint-modality, the unary holon, the union "observation") are *generated from* the
factored canon, never substituted for it.

## Four-box organization (built in)

The upper ontology is classified throughout by the **built-in graph-box roles** — the annotation
property `gmeow:graphBoxRole`, whose four box values are `gmeow:boxTBox`, `gmeow:boxABox`,
`gmeow:boxRBox`, and `gmeow:boxCBox`, with a fifth meta role, `gmeow:boxConfigBox`, marking the
module's own header and configuration rather than a reasoned term. The four boxes partition the
foundation cleanly:

- **TBox** — the sort and type taxonomy: `logic:Kind`, `logic:SubKind`, `logic:Phase`, `logic:Role`,
  `logic:Category`, `logic:Mixin`, `logic:RoleMixin`, `logic:PhaseMixin`, and the
  endurant / perdurant / aspect / quality / mode spine.
- **ABox** — individuals plus their RDF 1.2 edge-property assertions (the native-edge-property
  feature, classified where it belongs).
- **RBox** — relation and property axioms: the `logic:properPartOf` characteristics (transitive,
  asymmetric, irreflexive), the profiled mereology relations, `logic:mediates`, and the rest of the
  relational discipline.
- **CBox** — constraints and shapes, realized by the **built-in** closed-world lane: the closed-world
  half of the scoped open/closed construct (`logic:closedUnder` / `logic:WorldBoundary`), and the
  home of `FormalizationCandidate` constraints and `NonEntailmentObligation` checks.

Every authored term carries its graph-box role, so the partition is machine-checkable rather than
documentary — no term is left unclassified.

## Scope and seams

This charter states the conceptual foundation; the mechanisms it names are made precise elsewhere in
the document set, and the boundary between *what the foundation declares* and *how the engine
realizes it* is deliberate rather than implied.

**The charter declares:**

- the normative design statement of the `gmeow:logic` foundation;
- the **gUFO-floor superset vocabulary** — the `logic:` foundational sorts and relations that cover
  every gUFO term as a minimum baseline;
- the **feature primitives** of the greenfield map (`logic:WorldBoundary`, `logic:closedUnder`,
  `logic:Fluent`, `logic:Builtin`, the HiLog reification vocabulary, `logic:properPartOf` and its
  characteristics, the profiled mereology and `HolonicPosition` constructs, the factored claim-modality
  axes, the proposition/token/attitude/evaluation constructs, the typed-attack argumentation
  vocabulary, and the `FormalizationCandidate` / `NonEntailmentObligation` governance constructs) —
  declarations that add no axioms to the reasoned core until they pass the formalization lifecycle;
- the **gUFO ⊇ coverage discipline** — the standing requirement that every gUFO term have a `logic:`
  counterpart, enforced natively by the `meta:gate-logic-gufo-superset` gate
  (`crates/logic/tests/gufo_superset.rs`).

**Realized in the engine and the rest of the set:**

- **builtin execution** — the `logic:Builtin` registry runs;
- **scoped closed-world enforcement** — `logic:closedUnder` / `logic:WorldBoundary` enforced by the
  built-in closed-world lane co-resident with `logic:`;
- **discipline rule-authoring** — the OntoUML disciplines as authored `logic:` rules, whose
  operational lowering (rigidity as bounded world-quantification, identity supply as HiLog
  reification) is specified in
  [LOGIC-SEMANTICS.md § Operational semantics](LOGIC-SEMANTICS.md#operational-semantics-modality-and-identity-supply);
- **neuro-symbolic** integration and the **DOLCE / SUMO** bridge views.

The foundation declares its charter and vocabulary; the formal meaning of each mechanism is in
[LOGIC-SEMANTICS.md](LOGIC-SEMANTICS.md), the configuration of a reasoning request in
[LOGIC-CONTRACT.md](LOGIC-CONTRACT.md), the typed compilation target in [LOGIC-IR.md](LOGIC-IR.md),
and state-change semantics in [LOGIC-TRANSACTION.md](LOGIC-TRANSACTION.md).

## Foundation projection and discipline

UFO⁺ is authored canonically in `logic:`; the upper ontologies are generated, and they are not all
the same kind of projection.

**gUFO** is the primary generated down-projection of UFO⁺ — the OWL realization of the same UFO
lineage, truth-preserving for the fragment OWL can express, validated by running the full set of
OntoUML anti-pattern checks over the downcast. The downcast satisfies all five disciplines:
stereotype cardinality, identity overlap, anti-rigidity, free-role integrity, and relator mediation.

**BFO, DOLCE, and SUMO** are generated alignment/bridge views, not truth-preserving projections,
unless a specific subfragment is certified as such in the loss ledger. They carry different
ontological commitments, and the maximal-source doctrine respects that rather than claiming a shared
foundation. A bridge view is labelled in [LOGIC-CONFORMANCE.md](LOGIC-CONFORMANCE.md) so no consumer
mistakes it for a sound projection.

### Disciplines as rules; cross-world rigidity; the witness obligation

The OntoUML disciplines are `logic:` rules, not external lint. The native evaluator derives
`logic:violation` facts reproducing, class-for-class, the offending sets the discipline checks
describe — five violation labels from four checks: `logic:StereotypeCardinality`, `logic:MixIden`,
`logic:FreeRole`, `logic:MixRig`, and `logic:RelComp`. The lowering certifies under
`logic:StratifiedNAFProfile`. The discipline checks are the regression specification of the lowering;
the lowering is the enforcement mechanism.

Beyond that endogenous regression specification, the disciplines are graded against an *external*,
independently-authored corpus of models in the OntoUML metamodel vocabulary, each carrying the
community's documented anti-pattern verdict as engine-agnostic ground truth (the external
foundation-discipline soundness oracle in the conformance design). Agreement establishes soundness,
not merely stability; a clean model that fires any discipline is a false positive; and a documented
anti-pattern the disciplines cannot yet reproduce is an honest gap that feeds this section's
formalization backlog rather than a silent pass. The OntoUML stereotype vocabulary is subsumed by
reference through the alignment stack (`skos:exactMatch` puns between the `logic:` stereotypes and
their `ontouml:` counterparts), so the catalog vocabulary is dogfooded as aligned individuals.

Profiled-mereology constraints are a *second* violation family, kept deliberately separate from the
OntoUML disciplines. Weak supplementation — a whole with a proper part has another proper part
disjoint from the first — is not a foundation-wide structural anti-pattern but an axiom that holds only
*within* a declared `logic:MereologyProfile` (see the typed-and-contextual-mereology-and-holons section
above). It is therefore a `logic:MereologyConstraint`, not a `logic:Discipline`, even though it is
emitted through the same uniform `logic:violation` predicate. Its lowering, also under
`logic:StratifiedNAFProfile`, is a three-step stratified Datalog chain over the asserted strict
parthood relation `logic:properPartOf`: `logic:overlaps` is derived positively (two entities that share
a proper part overlap, and a proper part overlaps its whole); `logic:disjoint` and the helper
`logic:hasDisjointCopart` are its negation-as-failure complements, range-restricted to co-parts of a
common whole so disjointness is only ever asserted where overlap is defined; and
`logic:violation logic:WeakSupplementation` fires for a whole that is supplementation-scoped (declared
under a profile via `logic:underMereologyProfile`) and has a proper part with no disjoint co-part. The
unary holon projection rides the same parthood relation: `logic:isHolon` is derived for any entity that
is simultaneously a proper part of some whole and itself a whole of some part — the lossy unary
projection of the relational `logic:HolonicPosition`. All these rules are inert on inputs that carry no
`logic:properPartOf` facts, so the OntoUML-discipline cases are unaffected.

Quality/quantity constraints are a further violation family, governing the stratification of a
`logic:Quality` into a frame-independent generic quality, the anti-rigid role it plays in a
bearer-context, and a frame-relative measured value — the YAMATO refinement adopted by-reference
(persistent quality identity, the generic-quality→quality-role ladder, and unit-independent true
quantity; `docs/foundational-bridging.md`). Like the mereology constraints they are
`logic:QualityConstraint` individuals emitted through the uniform `logic:violation` predicate, and
their lowerings certify under `logic:StratifiedNAFProfile`. Two are minted, each a single stratified
rule with a negation-as-failure complement. `logic:QualityRoleWithoutGeneric` fires for a quality
carrying a `logic:qualityRole` but no `logic:genericQuality` — a frame-relative value standing
without the frame-independent structure it refines (Principle 11 in role terms), the NAF complement
ranging over `logic:genericQuality`. `logic:MeasurementFrameMissing` fires for a measurement bearing a
`logic:unit` but no `logic:referenceFrame` — a value expressed in a unit without its frame, ill-formed
rather than merely under-specified (Principle 11), the NAF complement ranging over
`logic:referenceFrame`. Because the foundation chase is all-IRI (it carries no literal facts), the rule
keys on the IRI-valued `logic:unit` witness rather than the literal `logic:measuredValue` it qualifies —
the same move the holon coherence rule makes in keying on `logic:hasHolonicPosition` rather than the
literal `logic:holonicLevel`. The domain measurement predicates `gmeow:unit` and `gmeow:referenceFrame`
are sub-properties of `logic:unit` and `logic:referenceFrame`, so a domain measurement lifts into the
constraint's scope without the rule mentioning any `gmeow:` term — the same domain-grounding lift the
occurrent refinements use. Both rules are inert on inputs carrying no quality stratification, so existing cases
are unaffected.

Cross-world rigidity — the world-spanning universal quantifier that no ordinary in-world Datalog rule
expresses — is evaluated as a bounded closure pass over the finite materialized world set, emitting
`logic:rigidityViolation` quads in the world where rigidity persistence fails. The pass fires when at
least two worlds are materialized.

Anti-rigidity's witness obligation — a world of existence where the instance lacks the type — belongs
to counterfactual construction in Stratum C; the `"anti_rigidity_policy"` profile field governs the
instance-level obligation facet. The operational semantics of the foundation are in
[LOGIC-SEMANTICS.md](LOGIC-SEMANTICS.md#operational-semantics-modality-and-identity-supply).
