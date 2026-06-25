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
  place the same entity at different depths, and both are correct.
- **Emergence is assessed, not asserted.** Emergence is an **EmergenceAssessment** relative to a
  declared *reduction theory* — the claim that a whole's property is not derivable from its parts
  *under that theory*. Failure to derive is **not** proof of irreducibility; the assessment records
  the theory it is relative to so a later, stronger theory can overturn it without contradiction.
- **Downward constraint is structured and non-transitive.** A whole may constrain its parts, but the
  constraint is a typed, directed relation that does **not** chain transitively by default; a
  constraint from level *n* onto level *n−1* says nothing automatic about level *n−2*.
- **Autonomy/integration is a named profile.** The Koestlerian balance of part-autonomy against
  whole-integration is a **declared profile** a holarchy may adopt, not a universal well-formedness
  rule every holon must satisfy.

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
meant to carry:

- **definition** — necessary-and-sufficient identity of a term;
- **necessary** / **sufficient** — one-directional conditions;
- **constraint** — an integrity condition whose violation is a finding, not a derivation;
- **derivation** — a productive rule whose head is entailed;
- **default** / **typicality** — defeasible or generic-by-default content;
- **recommendation** — advisory, never enforced;
- **non-entailment** — a deliberate *non*-assertion (see below);
- **overlap** — a mereological sharing fact;
- **doc-only** — prose that is explanatory and is *not* to be formalized at all.

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
  counterpart.

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

Cross-world rigidity — the world-spanning universal quantifier that no ordinary in-world Datalog rule
expresses — is evaluated as a bounded closure pass over the finite materialized world set, emitting
`logic:rigidityViolation` quads in the world where rigidity persistence fails. The pass fires when at
least two worlds are materialized.

Anti-rigidity's witness obligation — a world of existence where the instance lacks the type — belongs
to counterfactual construction in Stratum C; the `"anti_rigidity_policy"` profile field governs the
instance-level obligation facet. The operational semantics of the foundation are in
[LOGIC-SEMANTICS.md](LOGIC-SEMANTICS.md#operational-semantics-modality-and-identity-supply).
