<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — Foundation Design Charter (gmeow:logic ⊇ gUFO)

> Status: normative design charter for the `gmeow:logic` upper ontology. This is the **charter**
> member of the [GMEOW Logic document set](LOGIC.md#the-document-set): it states what the
> foundation *is*, the documented predecessor weaknesses it refuses to inherit, the greenfield
> primitives it declares, and where each capability is realized. Vision and lineage are in
> [LOGIC.md](LOGIC.md); the formal account of every mechanism named here is in
> [LOGIC-SEMANTICS.md](LOGIC-SEMANTICS.md). Where this charter states a doctrine once, the
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
generated, lossy down-projection of the canonical foundation, and its preservation polarity in the
loss ledger is `logic:ValidationOnly` where it constrains without reasoning and a certified
under-approximation only on the fragment OWL can faithfully hold.

The doctrine, stated once for the rest of the charter to reference:

> **We adopt gUFO as the floor and WILL NOT inherit its compromises.** gUFO is the *minimum* the
> foundation must cover, never the *ceiling* it may reach. Every gUFO restraint — the OWL 2 DL
> expressivity cap, the reification tax, the scaled-back event model, the external SHACL stack — is
> a thing `gmeow:logic` is built to escape, not to reproduce.

## Criticism ledger

Each documented criticism of gUFO and OWL 2 is paired below with the `gmeow:logic` decision that
answers it and a status: **doctrine-exists** (already a stated Constitutional or design commitment),
**declared-this-PR** (a primitive minted in [`module.ttl`](../module.ttl) by issue #663), or
**seamed-to-#664/#665** (the answer is committed but its execution is owned by the compiler /
engine children). The point of the ledger is honesty: it records not just that a weakness is
answered but *where* the answer lives.

### gUFO criticisms

| # | Criticism | The `gmeow:logic` decision | Status |
|---|---|---|---|
| 1 | Reduced expressiveness — the OWL 2 DL ceiling strips UFO's modal and full-FOL axioms | Turing-complete `logic:`; rich modal / FOL / higher-order axioms are canonical; gUFO becomes a lossy projection | doctrine-exists (Principle 17) |
| 2 | Triple bloat / heavy reification — must mint `gufo:Quality` / `Relator` / `Situation` nodes | Native edge properties via RDF 1.2 statement terms; flat-first, reify-on-demand | declared-this-PR |
| 3 | Minimalistic UFO-B — events and processes scaled back | Native 4D: perdurant / process / participation spine + temporal fluents + Principle-11 frame-relativity | declared-this-PR |
| 4 | High complexity / steep learning curve — dual taxonomy of sortals, kinds, phases, roles, mixins | Progressive disclosure (Principle 13): precise underneath, gentle flat on-ramp on top | doctrine-exists |
| 5 | OWL punning + a *secondary* SHACL stack | First-class multi-level modeling via HiLog / F-logic reification; SHACL built in (`crates/shacl`) — one integrated system | declared-this-PR |
| 6 | BFO/DOLCE niche — enterprise-software focus | Bridge views *by reference* (Principles 5 & 17): AI-memory home market + BFO (scientific) + DOLCE (cognitive) seams | doctrine-exists |

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
80 % case, with a reified relator promoted only when period, role, confidence, or standpoint
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

**5 — OWL punning and a secondary SHACL stack.** gUFO leans on OWL punning (treating a class as an
individual) for the multi-level moves UFO requires, and it relies on an *external* SHACL stack for
the constraints OWL cannot express — two systems, loosely coupled. `gmeow:logic` makes multi-level
modeling **first-class** via HiLog / F-logic reification: types are reified as first-order
individuals so that "quantifying over types" is ordinary first-order quantification. The
already-minted `logic:suppliesIdentity` is exactly this move — a *second-order identity-supply
relation reified as a first-order relation* over punned type individuals. And SHACL is **built in**
(`crates/shacl`), so validation and inference are **one integrated system**, not a bolt-on second
stack; multi-level types validate natively rather than through an external bridge.

**6 — BFO/DOLCE niche.** gUFO's lineage is oriented toward enterprise software modelling, which
narrows its reach. `gmeow:logic` spans wider by treating the upper-ontology bridges as
**by-reference** alignments (Constitution **Principles 5 & 17**) rather than imports: its home
market is AI memory, it already carries a **BFO bridge**
([`docs/foundational-bridging.md`](../../../../docs/foundational-bridging.md)) for the scientific
constituency, and it seams to **DOLCE** for the cognitive one. None of these are truth-preserving
projections — they carry genuinely different ontological commitments — and the loss ledger records
that honestly rather than overclaiming a shared foundation.

### OWL 2 criticisms

| # | Criticism | The `gmeow:logic` decision | Status |
|---|---|---|---|
| 7 | Global restrictions for decidability — a property may not be both transitive and asymmetric; property chains barred from cardinality restrictions | None here: `logic:properPartOf` is transitive ∧ asymmetric ∧ irreflexive — a strict partial order with full reasoning; decidability recovered by projection, not by crippling the canon | declared-this-PR |
| 8 | "Too much logic, not enough practical features" — no native string concat, no date/time arithmetic; forced into SWRL / vendor extensions | Native builtins, profile-gated; the Principle-12 line drawn explicitly between derivational builtins (in `logic:`) and heavy domain computation (external by reference) | seamed-to-#664/#665 |

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
date/time arithmetic, no basic math — pushing modellers into SWRL or vendor-specific extensions
that break interoperability. `gmeow:logic` provides **native builtins**, profile-gated to the
`logic:ProceduralPrologProfile`, and it draws the Constitution **Principle 12** line **explicitly**:

- **Derivational builtins live *in* `logic:`** — string concatenation, date arithmetic, and basic
  math, e.g. `legalAge = year(now) − year(birth)` or `fullName = firstName + " " + lastName`. These
  are the lightweight derivations a foundation genuinely needs and that forcing into an external
  engine would make absurd.
- **Heavy *domain* computation stays *external* by reference** — geo / datum transforms, RCC-8 /
  Allen relation-algebra composition, SLAM and trajectory updates, n-dimensional vector operations.
  These remain the solver-boundary concern Principle 12 keeps out of the reasoned core.

The builtin *registry* is declared now (`logic:Builtin`, gated to the procedural profile); its
**execution** is owned by the compiler (#664) and engine (#665).

## Greenfield feature map

Where the criticism ledger says *what we refuse to inherit*, this map says *what we build instead* —
each greenfield feature paired with the primitive declared in [`module.ttl`](../module.ttl) by this
PR and the owner that realizes it.

1. **Native edge properties.** The RDF 1.2 statement-term doctrine carries metadata, temporal scope,
   and qualities directly on the edge (the answer to criticism 2). *Declared* as the statement-term
   authoring form; the materializing **engine is #665**.
2. **First-class multi-level modeling (goodbye punning).** A HiLog reification vocabulary lets types
   be quantified over as first-order individuals, with `logic:suppliesIdentity` as the worked
   example. *Declared* here; **native validation of multi-level types is #665**.
3. **Hybrid open/closed worlds, scoped.** `logic:WorldBoundary` and `logic:closedUnder` let a model
   declare *where* the closed-world reading applies, realized by the built-in SHACL (the CWA lane)
   co-resident with `logic:` (the OWA lane). *Declared* here; **scoped enforcement is #665**.
4. **Native spatiotemporal (4D) and fluents.** The perdurant / process spine, `logic:Fluent`, and
   the frame seam give a real UFO-B (the answer to criticism 3). *Declared* here.
5. **Tractable, parallelizable, neuro-symbolic.** The existing `logic:` profiles supply the
   tractable lanes — Datalog / Horn rule sets are PTIME and parallelizable — and the
   `logic:probability` / `logic:confidence` axes anchor the neuro-symbolic split: ML-approximate
   *classification* alongside symbolic *invariant enforcement*. *Documented* (the profiles and axes
   are already minted; see [LOGIC-SEMANTICS.md § Semantic profiles](LOGIC-SEMANTICS.md#semantic-profiles)).
6. **Integrated algorithmic / string primitives.** A `logic:Builtin` registry, gated to
   `logic:ProceduralPrologProfile` (the answer to criticism 8). *Declared* here; **execution is
   #664/#665**.

## The Ithkuil precision ethos

The clearest proof that `gmeow:logic` already practises its own ethos is the precision it *already*
carries in the four quantitative axes. A calibrated probability, an asserter's confidence, a solver
ranking weight, and an evidential warrant are **four genuinely distinct things**, and a common
failure mode of probabilistic knowledge graphs is collapsing them — treating arbitrary confidence
metadata as if it were a probability model. `gmeow:logic` keeps them apart as
`logic:probability ≠ logic:confidence ≠ logic:weight ≠ logic:evidenceStrength`
([`module.ttl`](../module.ttl); the governing rules are in
[LOGIC-SEMANTICS.md § Confidence, probability, weight, and evidence](LOGIC-SEMANTICS.md#confidence-probability-weight-and-evidence)).
It draws a second distinction the same way: **determinacy ≠ confidence** — whether a value is
ontically crisp, vague, fuzzy, probabilistic, or disputed is held separate from *how sure anyone is
about it*, exactly as Constitution **Principle 9** demands (`gmeow:Determinacy` versus
`gmeow:confidence`).

The ethos extends naturally outward. Just as the quantitative axes refuse to collapse four numbers
into one, the foundation refuses to collapse **configuration / affiliation / perspective** into a
coarse grouping: mereology distinguishes the *collective*, the *count*, and the *quantity* readings
of a whole, and standpoint / vantage keep "from whose perspective" explicit rather than implied.
The single governing principle, stated for the charter:

> **Encode every distinction explicitly; refuse to collapse what is genuinely distinct.** Where two
> notions can come apart in any model the foundation must serve, they are two terms, not one.

This is Ithkuil's orthogonal-factorization lesson turned into a foundational design rule, with its
fatal flaw engineered out: the precision is always *available*, but progressive disclosure
(Principle 13) means it is never *mandatory* before it is needed.

## Four-box organization (built in)

The upper ontology is classified throughout by the **built-in graph-box roles** (`gmeow:graphBoxRole`,
landed across #642 / #650 / #653 and already asserted on this module via `gmeow:boxTBox` and
`gmeow:boxConfigBox`). The four boxes partition the foundation cleanly:

- **TBox** — the sort and type taxonomy: `logic:Kind`, `logic:SubKind`, `logic:Phase`, `logic:Role`,
  `logic:Category`, `logic:Mixin`, `logic:RoleMixin`, `logic:PhaseMixin`, and the
  endurant / perdurant / aspect / quality / mode spine.
- **ABox** — individuals plus their RDF 1.2 edge-property assertions (the native-edge-property
  feature, classified where it belongs).
- **RBox** — relation and property axioms: the `logic:properPartOf` characteristics (transitive,
  asymmetric, irreflexive), `logic:mediates`, and the rest of the relational discipline.
- **CBox** — constraints and shapes, realized by the **built-in SHACL** (`crates/shacl`): this is the
  closed-world lane of the scoped open/closed construct (`logic:closedUnder` / `logic:WorldBoundary`),
  the CWA half of the hybrid-worlds feature.

Every authored term carries its graph-box role, so the partition is machine-checkable rather than
documentary — the coverage gate enforces that no term is left unclassified.

## Scope and seams

This charter is deliberate about what issue #663 delivers versus what is seamed to its children, so
the boundary is auditable rather than implied.

**This PR (#663) delivers:**

- this charter — the normative design statement of the `gmeow:logic` foundation;
- the **gUFO-floor superset vocabulary** — the `logic:` foundational sorts and relations that cover
  every gUFO term as a minimum baseline;
- the **feature-primitive declarations** named in the greenfield map (`logic:WorldBoundary`,
  `logic:closedUnder`, `logic:Fluent`, `logic:Builtin`, the HiLog reification vocabulary,
  `logic:properPartOf` and its characteristics) — bare declarations that add no axioms to the
  reasoned core, exactly as the existing foundation surface was minted;
- the **gUFO ⊇ coverage gate** — the machine check that every gUFO term has a `logic:` counterpart.

**Seamed to the children (compiler #664 / engine #665):**

- **builtin execution** — the `logic:Builtin` registry runs;
- **scoped-CWA enforcement** — `logic:closedUnder` / `logic:WorldBoundary` are actually enforced by
  the built-in SHACL co-resident with `logic:`;
- **discipline rule-authoring** — the OntoUML disciplines as authored `logic:` rules;
- **neuro-symbolic** integration and the **DOLCE / SUMO** bridge views.

And one explicit non-change: **`crates/logic/src/foundation.rs` remains the evaluation authority for
the five type-level OntoUML disciplines** (stereotype cardinality, identity overlap, free role,
mixed rigidity, relator completeness; see
[LOGIC-SEMANTICS.md § Operational semantics](LOGIC-SEMANTICS.md#operational-semantics-modality-and-identity-supply)).
**This PR lifts no rules into the engine** — it declares the foundation's charter and vocabulary;
the rule-authoring and execution stay with the children. The doctrine "Rust is the authority" is
preserved unchanged.
