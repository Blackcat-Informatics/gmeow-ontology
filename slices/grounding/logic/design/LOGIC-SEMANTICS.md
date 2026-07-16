<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Logic — Formal Semantics

> The formal-semantics member of the
> [GMEOW Logic document set](LOGIC.md#the-document-set). It defines the unified core, the
> triple-term/assertion rules, the reasoning contract's semantic meaning, the typed reasoning
> result, the typed context algebra of worlds and modality, the foundation's operational semantics,
> and the decidability stance. Vision is in [LOGIC.md](LOGIC.md); how a request is configured is in
> [LOGIC-CONTRACT.md](LOGIC-CONTRACT.md); the typed intermediate representation is in
> [LOGIC-IR.md](LOGIC-IR.md); state-change semantics are in [LOGIC-TRANSACTION.md](LOGIC-TRANSACTION.md);
> the engine that realizes these semantics is in [LOGIC-RUNTIME.md](LOGIC-RUNTIME.md). The
> cross-ontology correspondence calculus that builds on these semantics — including the fact that the
> preservation polarities double as lens / Galois-connection soundness conditions, and the standpoint
> index governs every correspondence — is in [LOGIC-CORRESPONDENCE.md](LOGIC-CORRESPONDENCE.md).

## The Unified Logic Core

`logic:` is one coherent model in which the following coexist rather than compete.

- **Open-world classification and closed-world constraints, co-resident.** Logical structure (what
  follows) is evaluated open-world; constraints (what is ill-formed for a purpose) are evaluated
  closed-world. A model declares which reading applies where, instead of being forced into one.
  This unifies the classification lane, the validation lane, and the negative-query lane into a
  single semantics, selected by the contract's Closure facet (see
  [LOGIC-CONTRACT.md](LOGIC-CONTRACT.md)).
- **Monotonic and non-monotonic rules.** Monotonic derivation *and* defeasible defaults,
  negation-as-failure, and classical negation. Recursion is unrestricted. Existential rules
  (tuple-generating dependencies) permit value invention. Which non-monotonic semantics governs a
  rule set is **declared, not assumed** — it is the Model-semantics and Negation facets of the
  reasoning contract (see [The reasoning contract](#the-reasoning-contract)).
- **Logic programming, Prolog-grade.** Unification, goal-directed resolution through subsumptive
  demand transformation, query-as-program, builtins, and generative/relational computation. The
  engine runs **both** directions through one fixpoint core: forward materialization and
  classification *and* demand-restricted goal resolution. Graph query becomes a projection of goal
  resolution, not a separate paradigm. Procedural control — cut — is retired and rejected.
- **Contextual, temporal, modal, and probabilistic scope as first-class.** Every axiom or clause
  may carry standpoint, valid time, asserted time, provenance, confidence, modality, and
  disclosure/display state. These four quantitative axes are distinct and **not interchangeable**
  (see [Confidence, probability, weight, evidence](#confidence-probability-weight-and-evidence)).
  Temporal scope ties to the temporal query algebra.
- **Paraconsistency.** A contradiction does not explode the model. Inconsistency is localized to
  the contexts and scopes that disagree, witnessed explicitly, and reasoned around. The guarantee is
  made at the *entailment relation*, not merely at storage (see
  [Inconsistency across contexts](#inconsistency-across-contexts-and-context-indexed-entailment)).
- **Metalogic over triple terms.** RDF 1.2 triple terms make statements first-class objects, so
  axioms reason about axioms, rules about rules, and claims about claims. Axiom identity and axiom
  metadata are native, not encoded — but a triple term *names* a proposition; it does not assert it
  (see [Triple terms, reifiers, and assertion](#triple-terms-reifiers-and-assertion)).

These three concerns, which OWL tends to entangle, are kept distinct: **logical structure** (what
follows from the canonical model), **validation** (what data is ill-formed for a target purpose),
and **projection** (what weaker consumers can safely receive).

## Canonical Model

`logic:` resolves to `https://blackcatinformatics.ca/logic/`, declared once in the unified prefix
registry alongside `gmeow`, so it propagates to every serializer and the JSON-LD context.

The core authoring vocabulary includes:

- classes, properties, individuals, datatypes, and annotations;
- subclass, subproperty, equivalence, disjointness, inverse, transitive, symmetric, functional, and
  property-chain constructs;
- restrictions and quantification patterns projectable into OWL when possible;
- **rules** (Horn clauses and beyond): heads, bodies, negation, recursion, existentials, builtins;
- **RDF 1.2 triple-term forms** for axiom identity and axiom/rule metadata;
- **contextual axiom scope**: standpoint, valid time, asserted time, provenance, confidence,
  modality, disclosure;
- **closed-world constraints** for validation and contradiction detection;
- **loss metadata** for every generated compatibility projection.

The RDF 1.2 triple-term forms are not invented here. They generalize the authoring model already
proven for the assertion-level statement-metadata layer: `gmeow:StatementMetadata` is one reified
statement, the `(qSubject qPredicate qObject)` triad is the quoted triple, `gmeow:reifier` is the
`rdf:reifies` subject, and the `gmeow:annotation` list is every metadata triple hung off the
reifier. `logic:` lifts that same form from statements-about-facts to axioms-and-rules-about-anything.

### RDF 1.2 triple terms, precisely

`logic:` targets **normative RDF 1.2**, and uses the term *RDF 1.2 triple terms* consistently —
never "RDF-star," except when discussing lineage (the RDF-star community drafts that preceded RDF
1.2). The distinction is operational, not pedantic: the storage substrate **dropped RDF-star support
in favor of RDF 1.2**, and RDF 1.2 no longer admits triple terms in subject position. RDF 1.2 *Full*
conformance supports graphs and datasets whose triples contain triple
terms (in object position); *Basic* conformance excludes triple terms entirely. Symmetric or
generalized forms that place triple terms in subject, predicate, or graph-name position are
**non-standard extensions** that cause interoperability problems.

Rule:

> `logic:` MUST restrict canonical RDF output to normative **RDF 1.2 Full**. Any symmetric or
> generalized use of triple terms outside normative positions is an **internal IR extension** and
> MUST be downcast or rejected before emitting public RDF 1.2 artifacts.

This strengthens the projection doctrine: normative RDF 1.2 Full is the canonical *interchange*
surface; any more-general internal graph calculus is a **compiler concern**, not a standards
ambiguity exposed to consumers.

### Triple terms, reifiers, and assertion

The single most bug-prone confusion in a triple-term logic is treating `<<( x rdf:type T )>>` as
*both* a quoted object and an asserted fact. The rule that prevents it, stated early and normatively:

> A triple term denotes a **proposition-shaped term**. It does **not**, by itself, assert the
> embedded triple. Assertion, quotation, denial, refutation, provenance, confidence, and
> world-indexed truth are all mediated by explicit `logic:` predicates.

So a triple term gives **statement grouping** — the ability to name, quote, and annotate a
statement — and nothing more. Modal force, epistemic stance, and truth-in-a-world come only from
predicates over the reifier; they are never implicit in the quoting. This rule recurs wherever
triple terms appear (the foundation's modal lowering, the worlds layer), and it is the reason the
foundation needs explicit lowering rather than getting modality "for free."

## The reasoning contract

"Sound" is ambiguous until you answer "sound relative to which semantics?" Because `logic:` admits
monotonic rules, negation-as-failure, well-founded and stable-model semantics, paraconsistent
consequence, and procedural search all at once, **every reasoning request is governed by an explicit
`logic:ReasoningContract`** — a selection of values across orthogonal facets, defined in
[LOGIC-CONTRACT.md](LOGIC-CONTRACT.md). Soundness, completeness, and the decidability class are all
stated *relative to that contract* — a claim of "sound" with no contract in hand is meaningless.

A fixed, closed list of six indivisible "profiles" is not the model, and neither is a single
`Consequence` facet. **There is no one consequence facet.** What an entailment *means* is settled
jointly by several orthogonal facets of the contract
([LOGIC-CONTRACT.md](LOGIC-CONTRACT.md)). The "modes" that a single column would crowd together are
in fact values of *different* facets; the table below attributes each to the facet that actually
carries it.
They compose rather than exclude.

| Facet | Value | Formal meaning | Decidability character |
|---|---|---|---|
| Model semantics | least-model (positive Horn) | monotonic Horn entailment; the least model | terminating for the function-free fragment |
| Model semantics | least-model + stratified default negation | the unique perfect model under stratification | PTIME data complexity |
| Model semantics | well-founded | well-founded semantics; three-valued | total, polynomial |
| Model semantics | stable-model | answer-set / stable-model semantics; possibly several models | NP-hard |
| Negation operators | `{default}` / `{explicit}` / both | which "not" a rule may use; a **set**, composed *with* a model semantics, never one of its own | inherits the model semantics it composes with |
| Truth / inconsistency | a Belnap-family configuration (FDE, LP, K3, …) | four-valued / paraconsistent truth; contradiction does not explode | localized, witness-bearing |
| Uncertainty | `{probabilistic}` (and/or weighted, fuzzy) | a graded-belief **measure** carried alongside the model semantics | requires a declared dependency model |

Two attributions matter especially, because the old single-column table got them wrong:

- **Stratified negation-as-failure is not a model semantics of its own.** It is least-model semantics
  composed with the `Negation` facet value `{default}` under a stratification condition. Negation is a
  *set-valued* facet — a program may use explicit and default negation together — orthogonal to which
  model semantics selects the models.
- **Paraconsistency and probability are not consequence relations.** Paraconsistent truth is the
  `Truth / inconsistency` facet (a Belnap-family configuration — see
  [LOGIC-CONTRACT.md § Truth values](LOGIC-CONTRACT.md#truth-values-admissible-valuations-and-the-designated-value-policy));
  probabilistic inference is a *measure* on the `Uncertainty` facet, carried alongside whatever model
  semantics is in force, never in place of one.

The historical profile names survive only as **presets** — named bundles of facet values the compiler
expands before anything else runs (see [LOGIC-CONTRACT.md § Presets](LOGIC-CONTRACT.md)). The
historical `procedural` preset now selects least-model semantics with the closed set of checked
builtins under a bounded `Resource`; it does not license cut or a second search engine.

The governing rule on combinations is the contract's, restated here for the semantics:

> Not every point in the facet product has a defined semantics. **Unsupported combinations resolve to
> `unsupported` — they are never silently approximated by the nearest defined semantics.** A request
> for, say, probabilistic stable models, or paraconsistent counterfactual revision, either has a
> defined (possibly bounded) evaluation or it is reported `unsupported` in the typed result. Quiet
> substitution of a nearby consequence relation is the one outcome the contract exists to forbid.

A request with no resolvable contract is rejected before any reasoning begins.

### Query-scoped external relation inputs

Hybrid retrieval relations — lexical candidates, vector neighbours, graph neighbourhoods,
temporal/spatial candidates, and topology-derived candidates — may enter a query as explicitly
registered external relation providers. They are **derived inputs to one query execution**, not
asserted RDF facts and not additions to the ontology. A provider atom participates at its authored
position in the same demand-transformed fixpoint, joins, recursion, profile gate, and preservation
ledger as ordinary RDF EDB and IDB atoms. There is no second hybrid evaluator and no scratch-world
fallback semantics.

The relation descriptor is part of the request's meaning. It pins relation IRI and arity, positional
RDF 1.2 term schema, provider/index/model identities, bound/limit/order policy, annotation algebra
and dimension, deterministic budget, and preservation. The provider manifest is framed into the
query contract hash. Consequently, changing a model, artifact generation, budget, order, or schema
changes query identity rather than invisibly changing the result of identical query text.

The result boundary distinguishes three cases:

1. a complete successful batch with no rows is semantic absence for that requested relation prefix;
2. a provider that reports failure or cannot certify the prefix is a typed non-result; and
3. an RDF view that fails operationally is a separate typed source failure.

Neither non-result may be represented as case 1, and no partial batch crosses the boundary. Success
and failure receipts retain every attempted provider request and artifact generation; successful
answer lineage separately identifies only providers whose tuples contributed.

An external tuple's algebra element also has an explicit **dimension**. Similarity, ordinal rank,
distance, topological persistence, and epistemic confidence are not interchangeable. They may share
the same generic algebra interface only when the descriptor names the meaning and the selected
algebra identity matches; the engine never relabels a retrieval score as confidence.

### Cut is retired, not canonical

Cut is operational search control; it can change a program's answer set and makes explanations hard
to treat as logical proofs. It therefore stays out of the canonical declarative semantics:

> The canonical rule semantics is **cut-free**. The parser recognizes `logic:cut` only to emit a
> typed retirement diagnostic before execution; no preset licenses it, and an artifact containing
> cut is rejected rather than projected as a declarative rule (see [LOGIC-IR.md](LOGIC-IR.md)).

This preserves Prolog-grade *computation* (in its own profile, where it belongs) without letting
procedural pruning become part of the logic's truth theory or contaminate the faithful-by-
construction explanation contract.

### Confidence, probability, weight, and evidence

A confidence score, an evidential warrant, a calibrated probability, and a solver ranking weight are
**not interchangeable**, and a common failure mode of probabilistic knowledge graphs is treating
arbitrary confidence metadata as if it were a probability model. `logic:` keeps four distinct
predicates:

| Predicate | Meaning |
|---|---|
| `logic:probability` | probabilistic fact/event semantics; requires an independence or dependency model |
| `logic:confidence` | epistemic confidence assigned by a source or process |
| `logic:weight` | a solver/ranking weight, not necessarily probabilistic |
| `logic:evidenceStrength` | evidential support, provenance-derived (reuses the evidence-slice warrant axes) |

The governing rules:

> Probabilistic inference is available **only** when the contract's `Uncertainty` facet is
> `probabilistic`. A `logic:confidence` annotation MUST NOT be interpreted as a probability unless an
> explicit mapping to `logic:probability` is declared.

ProbLog-style inference is thus a *facet value*, not a default reading of every confidence number.

#### Marginals by weighted model counting

Probabilistic inference under `Uncertainty = probabilistic` computes **exact marginals by weighted
model counting**. A probabilistic fact is a Bernoulli variable; a **total choice** θ fixes a truth
value for every probabilistic variable. The probability of θ is:

- under **`logic:FullIndependence`**: `P(θ) = ∏_{f true in θ} p_f · ∏_{f false in θ} (1 − p_f)`;
- under a **`logic:DependencyModel`**: a declared explicit joint table over a correlated fact set
  replaces the product for those facts (each `logic:JointOutcome` carries its `logic:jointProbability`,
  and the outcomes must be exhaustive and sum to one); facts outside the correlated set stay
  independent and factorize as usual.

For each θ the least Herbrand model of `(Horn rules ∪ deterministic facts ∪ the facts θ makes true)`
is computed, and the **marginal of a query binding** is `Σ_{θ : binding ∈ model(θ)} P(θ)`. Inference
is exact by enumeration — `#P-hard` in general, which the contract's certified fragment records.

The further governing requirements (the named failure mode this prevents — treating un-modelled
metadata as a probability model):

> A declared `logic:probabilityModel` is **required**. A probabilistic query with probabilistic facts
> but **no** declared model returns `information = not-evaluated` (no probabilistic semantics was
> available — it is **not** the genuine `neither`) together with `evaluation = unsupported`, which
> marks the gap — it never silently assumes independence. A `logic:confidence` (or `logic:weight`, or
> `logic:evidenceStrength`) annotation is **never** read as a probability: a confidence-annotated fact
> is an asserted (deterministic) fact whose annotation is metadata, so its marginal is `1.0`, not the
> confidence value.

Under this facet, each answer binding additionally carries its computed `probability`; under any
other `Uncertainty` value no such field is produced.

## The reasoning result

Every reasoning surface — classification, goal resolution, validation, counterfactual construction —
returns the **same typed `logic:ReasoningResult`**. Uniformity is the point: an answer is never
interpretable apart from the contract it was produced under and the status it carries, so the result
makes both explicit. A bare "yes" or a bare set of bindings is never the whole result.

A `logic:ReasoningResult` carries a **compositional status**: a set of **five orthogonal fields**,
each ranging over its own values. They answer genuinely different questions, and several can hold at
once — a projection-loss-affected answer can *also* be budget-incomplete, an unsupported contract is
*also* a non-evaluation of the information. A single collapsed enum cannot express those co-occurring
conditions and silently forces one to mask another; the compositional shape is **Normative semantics**.

**`input` — was the request well-formed?**

- **valid** — the request and its sources parsed and type-checked; reasoning was attempted;
- **invalid** — the request or its sources were ill-formed and **no** reasoning was attempted (the
  other four fields are then vacuous).

**`evaluation` — what the engine was able to do.**

- **completed** — evaluation ran to its natural end under the contract;
- **budget-exhausted** — the resource budget was reached; the answers and witnesses found so far are
  returned with an honest incompleteness marker, and unprovable-within-budget is **not** falsity;
- **unsupported** — the requested facet combination (or a missing required model, such as an absent
  probabilistic dependency model) has no defined semantics; the contract's `unsupported` verdict
  surfaces here, never as a silent approximation.

**`completeness` — relative to what is the answer complete?**

- **complete-for-fragment** — the answer is complete relative to the certified fragment of the contract;
- **incomplete** — the search did not exhaust the answer space (e.g. under `budget-exhausted`, or
  outside any certified fragment);
- **unknown** — the engine cannot characterize its own completeness for this request.

**`preservation` — what did lowering do to the formulas?** This field is **not a single choice.** It
mirrors the structured, multidimensional preservation claim of
[LOGIC-CONFORMANCE.md § The loss ledger and preservation claims](LOGIC-CONFORMANCE.md#the-loss-ledger-and-preservation-claims)
and the per-lowering judgment of [LOGIC-IR.md](LOGIC-IR.md). It carries:

- a **set of preservation polarities** that may co-hold — `exact`, `under-approximation` (the evaluated
  theory is weaker; some consequences dropped), `over-approximation` (the evaluated theory is stronger;
  some spurious consequences possible), `inconsistency-preserving`, and `inconsistency-reflecting` —
  because a single lowering can be, for instance, *both* an under-approximation *and*
  inconsistency-reflecting; and
- the **set of unsupported constructs** the lowering could not carry at all.

`loss-affected` is **not** an alternative polarity. It is a *diagnostic* derived from the two sets
above — "some construct in the unsupported-set was relevant to this query" — and is usually a
*consequence* of an under-approximation, not a substitute for it. A result that passed through no
lowering carries the singleton polarity set `{exact}` with an empty unsupported-set; the consumer reads
the polarity set and the unsupported-set (or the full structured claim they reference) to see exactly
which formulas the target did not evaluate.

This is the contract the full-FOL formula evaluation honours. When a program's `logic:Formula` layer is
evaluated, the Horn-expressible fragment lowers exactly and runs in the chase, while any formula that
exceeds it (a disjunctive head, an existential needing a Skolem function, a sequence-marker or
non-binary predication) is carried as an unsupported construct: the result then holds the
`under-approximation` polarity (`sound-under`) — never a false `{exact}` — and the unsupported-set names
the residue by its closed `logic:FormulaShape` tag. Crucially, a non-evaluable formula contributes
**nothing** to the answer rather than being approximated by one disjunct or one witness: the consumer
sees a flagged residue, not a fabricated consequence.

**`information` — a four-valued (Belnap/FDE) information state about the queried proposition, plus two
explicit non-results.** It records what the *model's evidence* says — *when the evaluation was
conclusive and there were semantics to say anything at all*:

- **supported** — there is a proof and no counterproof;
- **opposed** — there is a counterproof and no proof;
- **both** — there is a proof *and* a counterproof (a witnessed contradiction within one context);
- **neither** — there is **neither** proof nor counterproof, established by a **conclusive** evaluation
  (a completed run, or one complete for a certified fragment). It is the open-world silence of a search
  that *finished*: a genuine four-valued verdict. The mere absence of a witness in an *unfinished*
  search is **not** `neither` — that is `undetermined`, below;
- **undetermined** — the evaluation did **not** reach a conclusive verdict, so the four-valued
  classification is not final. Any witnesses found so far are still reported — a proof found is
  *provisional* `supported`, a counterproof *provisional* `opposed`, both *provisional* `both` — but the
  bare *absence* of a witness within an incomplete search establishes nothing. Budget exhaustion before
  completion, and a partial-order tie in deterministic revision, both land here. It is also where a
  request that *has* a graded semantics but **no policy to discretize it** lands — a probabilistic
  marginal with no declared threshold: the marginal is reported and the discrete classification is left
  unassigned;
- **not-evaluated** — **no information semantics were available** to assess the proposition: an
  `unsupported` contract, or a missing *required* model (e.g. a probabilistic query with no declared
  dependency model — there is no probabilistic semantics to run at all). This is the field that prevents
  the most dangerous confusion in the result.

> The three non-positive states are **never** interchangeable. `neither` means *the engine looked,
> conclusively, and found no proof and no counterproof* — a real four-valued verdict. `undetermined`
> means *the engine has not (yet) reached a verdict* — the search did not finish, or no policy discretizes
> a graded answer — so its silence proves nothing. `not-evaluated` means *the engine could not look*,
> because the request had no defined semantics. Budget exhaustion yields `undetermined`; an unsupported
> contract or a missing probabilistic model yields `not-evaluated`; only a finished, conclusive search
> with no witnesses yields `neither`. Reporting an incomplete search's silence as `neither` would
> fabricate a conclusive verdict the engine never reached.

`information` is deliberately *not* classical true/false: `opposed` is not the negation of `supported`,
and `neither` is not falsity. Treating "no proof" as "false" is exactly the closed-world collapse the
open-world default rejects.

Because the five fields are independent, a result can carry, for example,
`input=valid · evaluation=budget-exhausted · completeness=incomplete · preservation={exact} ·
information=supported` (a proof was found — provisional, since the search did not finish) or
`input=valid · evaluation=unsupported · completeness=unknown · preservation={exact} ·
information=not-evaluated` (no semantics for the request, so no information was assessed) or
`input=valid · evaluation=budget-exhausted · completeness=incomplete · preservation={exact} ·
information=undetermined` (the search ran out of budget having found neither a proof nor a counterproof,
so no verdict was reached) — and the reader can tell those apart, which a single status word cannot.

### Contract-specific interpretation of `information`

The four-valued reading above is the *frame*; each consequence contract refines what `supported`,
`opposed`, and `neither` mean for it. The engine MUST apply the contract-appropriate rule, never a
generic one:

- **Stable-model (answer-set) contracts** distinguish **skeptical** from **credulous** support:
  skeptical-`supported` holds when the proposition is in *every* stable model, credulous-`supported`
  when it is in *some*. The result records which entailment regime the contract selected; the bare word
  `supported` is meaningless without it.
- **Well-founded contracts** distinguish the third truth value **undefined** from ordinary silence:
  `neither` here means *well-founded `undefined`* (the proposition is in neither the well-founded model
  nor its complement by the semantics' own assignment), which is a positive verdict of the
  three-valued model — not the absence of any verdict.
- **Probabilistic contracts** do **not** convert a probability into `supported`/`opposed` without an
  **explicit threshold policy**. Absent a declared policy mapping a marginal to a discrete information
  state, the discrete `information` field is `undetermined` — the discretization is *not applicable*, so
  the engine reports the marginal but takes no binary stance; this is emphatically **not** the
  conclusive `neither`. With no probabilistic model at all there is no probabilistic semantics to run,
  so the field is `not-evaluated`, per the rule above.
- **FDE / paraconsistent contracts** read `supported` as *evidence for the formula* and `opposed` as
  *evidence for its explicit negation* — the two are tracked separately, so `both` (evidence for each)
  and `neither` (evidence for neither) are first-class, not derived from one another.

Alongside the five status fields, the result carries the full apparatus needed to interpret and audit it:

- the **reasoning-contract identity** it was produced under;
- the **proof** and the **counterproof** (each a content-addressed derivation, or absent);
- the **context it holds in** — the world, standpoint, time, and path of the typed context algebra
  below (an answer is always *somewhere*, never nowhere);
- the **engine identity** that produced it;
- the **consumed budget** against the contract's `Resource` allowance;
- the **certified fragment** the completeness claim is relative to;
- the **projection-preservation class** of any lowering it passed through;
- the **contradiction witnesses** that justify an `information = both`;
- the **declared assumptions** the result depends on (closure choices, unique-name policy, entrenchment
  ordering, witness policy) so the result is never silently load-bearing on an unstated convention.

This is the shared currency between every reasoning surface and every downstream consumer: a single
typed object that says *what* was found, *how completely*, *under which contract*, *in which context*,
and *with what proof* — with no field standing in for another.

### The result row schema (ResultShape)

A `SELECT` returns a bag of variable bindings. Untyped, that bag carries no contract: nothing pins
which variables a query guarantees, what kind of term each binds, or how many rows the answer holds —
so two queries cannot be composed with any static guarantee that the producer's output fits the
consumer's input. The **`ResultShape`** is the schema-level type that closes this gap. It is the
**row-schema facet** of the reasoning result (`resultRowSchema`), and it is the Rust authority in
`crates/logic-compile/src/result_shape.rs` (re-exported as `gmeow_logic::result_shape`; this
section is its lossy projection, Principle 17).

A `ResultShape` is a set of typed **columns** plus a **row-set cardinality**:

- each **column** (`ResultColumn`) names one `SELECT` variable (`columnVariable`), declares its
  **term-kind** (`columnTermKind` — one of `iri`, `literal`, `blank-node`; **mandatory**, because a
  column with no declared kind is exactly the untyped bag this contract exists to remove), an optional
  **datatype** for a literal column (`columnDatatype`; absent means "any literal" — a *declared*
  loosening, never a half-typed column), and whether the variable is **bound in every row**
  (`columnBinding` — `required` or `optional`, the latter for a projected `OPTIONAL`);
- the **cardinality** (`shapeCardinality`) is one of `exact` (the declared example rows are the
  complete set), `contains` (they must all appear; extras are permitted), or `count` (only the row
  count is pinned, via `shapeRowCount`). These three modes subsume the test-DSL's
  `cqExactRows`/`cqExpectRowCount` tiers exactly — the schema is the single source, and the example
  rows become *example-instances* of it rather than a parallel mechanism.

Two operations make the contract enforceable, both **hard-fail and surfaced** (no silent
approximation):

1. **type conformance** — a result set's bindings are checked against the declared columns: a binding
   of the wrong term-kind or datatype, a missing `required` column, an undeclared extra column, or (in
   `count` mode) a wrong row count is a violation that stops the run with a named error.
2. **structural input→output compatibility**, data-free — a query may declare the shape it
   `expectsInputShape`, and the shape its producer `producesResultShape`. A producer *satisfies* a
   consumer's input shape iff it covers every `required` column with a compatible term-kind and (where
   the consumer pins one) datatype. This check runs **before execution**, so a composition that cannot
   type-check never runs — the query pipeline is checkable, not merely observable after the fact.

The declared shape is always the *contract*: it is authored, then bindings are validated against it. A
shape is **never** synthesised from the very bindings it would then check (that tautology always
passes and certifies nothing).

## Turing-Completeness, Decidability, and Termination

**Turing-completeness is a design goal, not a side effect.** `logic:` is meant to compute, not
merely classify — a general-purpose substrate in which any computable function is expressible as a
logic program. Builtins, arithmetic, unrestricted recursion, value-inventing existential rules, and
backward goal resolution are together Turing-complete, and that is the intent: the logic can express
its own transformations, validations, and solver-layer computations — including, metacircularly, the
rules that generate its own downcast projections.

Turing-completeness entails undecidability and the halting problem, by Church and Turing. That is a
theorem and the accepted shadow of a deliberately chosen capability, not a defect to be patched.
`logic:` manages it the way the project manages every hard constraint: by making termination a
**projection/profile property** and incompleteness **honest** rather than silent. Power at the
center, guarantees at the edges.

**Decidability is a contract `Resource` facet, not a canonical-layer promise.** The canonical layer
is maximally expressive and therefore only *semi-decidable*: consequences are enumerable, but the
*absence* of a proof need not be decidable in finite time. The decidable, tractable systems are
exactly the projections: OWL 2 EL (PTIME), OWL 2 DL (decidable, N2EXPTIME-complete), Datalog (PTIME
data complexity, terminating because function-free over a finite domain), and the chase-terminating
existential-rule fragments. "Decidability" joins "OWL-compatibility" as something a consumer **buys
by selecting a `Resource` facet** that names a certified-complete fragment, recorded in the
preservation judgment of any lowering it travels through.

**Contracts certify decidable fragments statically.** A model or slice may select a `Resource` facet
declaring that it lives in a decidable, terminating fragment — DL-safe rules, stratified negation,
weakly- or jointly-acyclic existential rules, guarded or sticky TGDs — and the compiler **statically
certifies membership**, flagging violations the same way it flags any structural anti-pattern.
Because termination is itself undecidable, certification uses *sufficient* acyclicity conditions, not
a complete test — a known, accepted tradeoff. Inside a certified fragment there is a hard termination
and complexity guarantee, and the result reports `completeness = complete-for-fragment`; outside it,
full expressivity and an explicit `completeness = incomplete` (typically alongside
`evaluation = budget-exhausted`).

**The canonical engine is sound-but-incomplete under an explicit budget.** Operationally the solver
runs under stratified semi-naive evaluation, subsumptive demand transformation over that fixpoint,
and a resource budget — the
`bounded` value of the contract's `Resource` facet, generalized. When the budget is exhausted it
returns `evaluation = budget-exhausted` with `completeness = incomplete`, never a false answer.
Soundness is total; completeness is relative to the budget and the certified fragment. Because
`logic:` is open-world, paraconsistent, and provenance-carrying, budget exhaustion is a normal state,
not a crash: the query returns the answers and witnesses found so far plus an explicit incompleteness
marker. Unprovable-within-budget is **not** false, and it is **not** the Belnap `neither` either: a
search that ran out of budget has not *established* the absence of a proof, so its information state is
`undetermined` (witnesses found so far are reported as provisional `supported`/`opposed`/`both`) — never
`opposed`, never the conclusive `neither`, and — since the engine *did* reason — never `not-evaluated`.

OWL 2 DL is decidable but N2EXPTIME-complete, the everyday face of "decidable but intractable."
`logic:` does not hide that cost behind a silent timeout; it makes the decidability class, the
fragment certification, and the budget boundary explicit, machine-readable, and tested.

## The `logic:` Foundation (UFO⁺)

A maximally expressive logic demands a foundational ontology authored *in that logic*, not imported
from a weaker one. "gUFO" is *gentle* UFO — a deliberately lightweight OWL 2 realization that drops
the modal distinctions and higher-order types of full UFO to stay inside OWL's decidable ceiling, so
it embodies the very restraint `logic:` rejects. The foundational theory — **UFO⁺** — is authored
canonically in the `logic:` namespace; gUFO becomes a generated down-projection (the gUFO/BFO/DOLCE
distinction is in
[LOGIC-FOUNDATION.md](LOGIC-FOUNDATION.md#foundation-projection-and-discipline)).

The foundational categories are `logic:` terms in the one namespace: `logic:Kind`, `logic:SubKind`,
`logic:Phase`, `logic:Role`, `logic:Category`, `logic:Mixin`, `logic:RoleMixin`, `logic:PhaseMixin`,
`logic:Relator`, `logic:Event`, `logic:Situation`, and relations such as `logic:rigidlyAppliesTo`,
`logic:suppliesIdentity`, and `logic:mediates`. They obey the same naming discipline as the rest of
GMEOW (Principle 9: no selector tokens) and the language-tag discipline (`@x-gmeow-english`).

The payoff: the OntoUML disciplines GMEOW enforces — exactly-one-stereotype, identity overlap
(**MixIden**), anti-rigidity (**MixRig** / **FreeRole**), and relator mediation (**RelComp**) — cannot
be expressed as OWL or gUFO axioms, because rigidity is modal and identity supply is second-order. In
the projection-down approach they survive only as **external structural checks** over the weaker
artifact. In `logic:` these become **actual axioms**. The discipline moves *from external check to
logic*; the equivalent checks survive only as projection-conformance tests over the gUFO downcast.

### Operational semantics: modality and identity supply

Lifting these disciplines into axioms is only real if the solver knows how to *interpret* them. By
the [triple-term/assertion rule](#triple-terms-reifiers-and-assertion), triple terms give statement
grouping, not modal or second-order semantics; so `logic:rigidlyAppliesTo` and
`logic:suppliesIdentity` are each given an explicit interpretation and **lowered** (compiled) onto
machinery a rule engine already executes. UFO needs only a small, bounded set of patterns, so the
lowering is tractable.

**Modality is Kripke semantics realized by GMEOW's existing contextual index.** Every axiom is
already relativized to standpoint, valid time, and modality; that index *is* a set of typed contexts
(see [the context algebra](#worlds-modality-and-counterfactuals--a-typed-context-algebra)). Predicates
are context-relativized — `holds(c, type(x, T))` rather than bare `type(x, T)` — and the modal
operators are the **standard translation** of modal logic into first-order logic over an explicit,
*typed* accessibility relation:

- **Rigidity.** `logic:rigidlyAppliesTo(T)` lowers to the integrity constraint *for every individual
  `x` and every pair of worlds `w`, `w'` in which `x` exists, if `x` is a `T` in `w` then `x` is a
  `T` in `w'`*. A `Kind` is rigid; a `Role`/`Phase` is anti-rigid — the same constraint *negated*.
  The solver needs no native ◻ operator: the compiler emits a universally quantified, world-indexed
  rule evaluated by closure/counting over the world set.
- **Boundedness.** Because the world set is the *finite, materialized* contextual index — not the
  unbounded space of all logically possible worlds — evaluation is bounded universal quantification,
  not full modal theorem proving.

**Second-order identity supply is HiLog/F-logic reification, not native second-order logic.** Types
are reified as first-order individuals (the OWL-punning move GMEOW already uses), so "quantifying
over types" becomes first-order quantification over those individuals: `logic:suppliesIdentity(K, x)`
is a first-order relation, and "every object instantiates exactly one ultimate sortal" is a
first-order **counting constraint** over `instantiates(x, K) ∧ Kind(K)`. This is the Flora-2/Ergo
lineage: syntactically second-order, semantically first-order, executable.

**The lowering is specified by the disciplines it replaces.** Over the gUFO downcast the lowered rules
must produce exactly the verdicts the external structural checks yield — the conformance suite is the
specification of the lowering — and they additionally decide cases (cross-world rigidity, type-level
identity) those checks cannot express.

**The lowering target — native and authoritative.** The type-level disciplines
derive `logic:violation` facts for stereotype cardinality, **MixIden**, **FreeRole**, **MixRig**, and
**RelComp** under a stratified-negation contract, with absence expressed via negation-as-failure and
"two distinct values" via an inequality guard. Cross-world rigidity is decided in the same evaluator by
a bounded closure over the finite materialized world set, emitting a rigidity-violation finding in the
failing context. The full-provenance findings flow into the shared `logic:ReasoningResult` every
downstream consumer reads. There is no secondary oracle and no fallback (the no-optionality
doctrine); correctness is proven end-to-end by the foundation conformance goldens.

### Anti-rigidity needs a witness policy

The anti-rigidity lowering says an anti-rigid `Role`/`Phase` requires a world of existence where the
instance *lacks* the type. That is formally correct but operationally hazardous: when only a finite
materialized world set is available, many legitimate Role/Phase instances would fail merely because no
counter-world has been materialized. A policy must be chosen:

| Policy | Meaning | Tradeoff |
|---|---|---|
| Witness-required | anti-rigid instantiation is invalid unless a counter-world exists | strict but heavy |
| Witness-obligation | the solver emits an obligation to construct or cite such a world | practical |
| Schema-only anti-rigidity | anti-rigidity constrains the type hierarchy, not each token instance | closest to a structural check |
| Context-dependent | finite-context reasoning uses obligation; generative reasoning may construct witnesses | most flexible |

> **Default: witness-obligation.** Anti-rigid instantiation emits a discharge obligation rather than
> failing; generative counterfactual reasoning may construct the witness world under budget (see
> [the context algebra](#worlds-modality-and-counterfactuals--a-typed-context-algebra)). Finite-context
> reasoning treats the obligation as satisfiable on demand; a slice may opt into the stricter
> witness-required or the lighter schema-only policy and declare which in its contract.

The three policy values are enforced by a dedicated anti-rigidity pass: `witness-obligation` (default,
emits a discharge obligation), `schema-only` (emits nothing at the instance level), and
`witness-required` (emits a witness-required violation absent a materialized counter-world). The policy
governs **only** the obligation/witness facet; the `logic:violation` and rigidity-violation findings
are computed by separate passes and are identical across all three policy values (the non-suppression
invariant).

## Worlds, Modality, and Counterfactuals — a typed context algebra

Possible worlds, hypotheticals, and counterfactuals are not a fringe concern for a reasoning ontology
built for AI work — they are the substance of slices GMEOW already ships: teleology (goal-worlds),
norms (ought-worlds), deception (belief vs asserted worlds), risk (feared futures), fiction
(representational worlds). The semantics make that apparatus explicit — and, crucially, **typed**.

### One generic `logic:World` is not enough

A single, undifferentiated `logic:World` with one generic accessibility relation is a category error.
"What I believe" is not "what is deontically ideal" is not "what would have happened" is not "what is
true in the fiction." Collapsing them lets an inference cross from one kind of context into an
unrelated kind — concluding that because a state is *believed possible* it is *permitted*, or that
because it holds in a *story* it holds *counterfactually*. The semantics therefore replaces the single
generalized world with a small algebra of **distinct typed contexts**, each with its **own typed
accessibility relation**.

### The typed contexts and their accessibility relations

| Context type | What it is | Typed accessibility relation |
|---|---|---|
| `logic:PossibleWorld` | an alethic possibility | **epistemically-possible** / alethically-accessible |
| `logic:EpistemicContext` | what an agent knows or believes | **doxastically-accessible** |
| `logic:Standpoint` | a named perspective truth is relative to | **sharpens** (a refinement poset) |
| `logic:Scenario` | a hypothesized situation under consideration | scenario-entertains |
| `logic:State` | one state of affairs along a history | (successor within a path) |
| `logic:History` / `logic:Path` | an ordered run of states | **temporally-succeeds** |
| `logic:ReferenceFrame` | a frame of measurement or canon | frame-relative-to |
| `logic:NarrativeFrame` | an in-universe representational canon | depicts / in-frame |

> **Realization mapping.** The conceptual `logic:`-prefixed context names in the table above are
> realized as declared terms distributed across slices per the Hybrid placement doctrine:
> `logic:PossibleWorld` and `logic:Path` are declared in the logic slice; `logic:Standpoint` is
> realized as the existing `gmeow:Standpoint`, `logic:Scenario` as `gmeow:Scenario`, and
> `logic:State` as `gmeow:State` in the standpoint slice; `logic:EpistemicContext` is realized as
> `gmeow:EpistemicContext` in the epistemics slice; `logic:ReferenceFrame` is realized as the
> existing `gmeow:ReferenceFrame` in the places slice; and `logic:NarrativeFrame` is realized as
> the existing `gmeow:NarrativeReferenceFrame` in the narrative extension. The `logic:` accessibility
> machinery (the five typed relations) remains central in this slice, referenced from domain slices
> in prose only.

Deontic and counterfactual reasoning are *uses* of these contexts rather than separate context types:
a deontic claim is truth in the **deontically-ideal** accessible contexts of an issuer; a
counterfactual claim is truth in the **counterfactually-closer** accessible contexts under a declared
closeness ordering. Each accessibility relation carries its own logical character (the **sharpens**
poset is transitive and reflexive; **temporally-succeeds** is a strict order; **doxastically-accessible**
need not be reflexive — an agent may believe a falsehood), and the modal operators are the standard
translation over the *appropriate* relation, never over a blurred union of all of them.

### The generic superproperty yields no cross-type inference

There is a generic `logic:accessibleFrom` superproperty of which every typed relation above is a
subproperty **in prose only** — none is asserted `rdfs:subPropertyOf` it, because such an edge would
re-enable exactly the cross-type entailment forbidden here. It exists for *uniform traversal and
provenance*, never for inference:

> The generic accessibility superproperty licenses **no cross-type entailment by itself.** An
> inference may follow a *named, typed* accessibility relation; it may **not** conclude anything by
> walking the bare superproperty across contexts of different types. Crossing from an
> `EpistemicContext` to a `PossibleWorld`, or from a `NarrativeFrame` to a `State`, requires an
> explicit bridge rule that names both types and states the consequence relation it carries.

This is what keeps "believed" from leaking into "true," "in the fiction" from leaking into "actual,"
and "ideal" from leaking into "is."

### An unindexed statement is unspecified, not universal

The most consequential rule of the algebra concerns the *absence* of an index:

> A statement asserted with **no context index** holds in an `gmeow:unspecifiedStandpoint`. It is
> **unspecified — not universal.** It is not implicitly true in every context, in any particular
> context, or at the top of the `sharpens` poset. Only an **explicit universal assertion** (a
> statement indexed to `gmeow:universalStandpoint`, or one that explicitly quantifies over all
> contexts of a stated type) propagates as universal.

Treating an unindexed statement as universally true is the world-semantics analogue of the closed-world
collapse: it manufactures a claim the author never made. The default is silence about *where* a
statement holds, and silence is the `gmeow:unspecifiedStandpoint`, which licenses no propagation. Universality
is a thing one says, not a thing one omits.

### Three strata of context reasoning

**Frame-indexed contexts (finite, decidable).** A finite set of materialized named contexts, truth by
`gmeow:accordingTo`, accessibility by the typed relation appropriate to the context type. Contested
facts coexist without collapse — a place is `conceivable`-ly contained in one polity per one standpoint
and `refuted` per another, both first-class. Deception lives here: a `gmeow:heldStandpoint` (an
`EpistemicContext`) and a `gmeow:projectedStandpoint` (an assertoric context) are two claims whose gap
*is* the deceptive act.

**Type-level / dispositional modality, the no-occurrence gate (decidable).** The risk doctrine: *the
counterfactual problem dissolves at the type level.* A disposition is real whether or not it manifests;
causal links relate event *types*, never event *tokens*; a goal is a *described* state realized by a
situation; a norm prescribes conduct *types*. The **no-occurrence gate** is an enforced invariant —
such reasoning entails zero event tokens. Counterfactual *force* with no generative machinery, fully
decidable.

**Generative counterfactual contexts (the frontier).** Genuine Lewis/Stalnaker counterfactuals: *if A
had been, C would follow*, where the A-context must be **constructed**. This is what an AI agent does
when it plans, models another mind (theory of mind = the held/projected divergence run forward), or
weighs a risk. The enabling reuses everything above: a counterfactual context is a **derived `logic:`
context** whose contents are *computed by a revision rule*, not asserted; the depiction seam generalizes
to a counterfactual seam; context construction is a logic program (the Turing-complete payoff); and
**closeness is declared data, not a fixed semantics** (Lewis comparative similarity = a generalized
closeness poset over contexts).

### Inconsistency across contexts, and context-indexed entailment

Typed contexts quarantine contradiction, which is why paraconsistency is load-bearing here. The
deceiver believes ¬P and asserts P; a character is a detective in-frame and fictional out-of-frame; a
place is claimed and disclaimed across standpoints. These are the data, not defects. But **partitioning
the data into named contexts alone does not by itself give paraconsistent *semantics*.** The guarantee
must be made at the inference relation:

> All entailment is **context-indexed** unless a rule explicitly quantifies across contexts. There is
> **no implicit union-of-contexts entailment mode.** A contradiction in context `C1` is not visible to
> context `C2` unless a cross-context rule names both, names their types, and carries its own
> paraconsistent consequence relation.

A contradiction *across* contexts is therefore never a contradiction *in* the model; only a
contradiction *within a single context* is a witness-bearing inconsistency, surfaced as
`information = both` in the reasoning result. `gmeow:refuted` encodes settled-false-in-a-context as
distinct from silence, and the three-axis orthogonality (`accordingTo` ⟂ `wasAttributedTo` ⟂
`confidence`) keeps "which context holds it," "who reported it," and "how sure we are" from collapsing.

This is why a context must never be described as simply "consistent." A context is **world-local in its
entailment scope** — it entails only what its own rules and the *named, typed* accessibility relations
reaching it license — and, under a paraconsistent consequence contract, it **may be internally
inconsistent**: a witnessed contradiction is *contained*, surfaced as `information = both` for the
queries that touch it, and reasoned around. It is not a global inconsistency that trivializes the model,
and it is not silently repaired into apparent consistency. "Consistent" is the wrong predicate; the
right ones are *world-local entailment scope* and *contained, witnessed inconsistency*.

### Deterministic revision: taming the AGM mutation explosion

Generative counterfactual reasoning computes AGM belief revision — minimally mutate a base context to
admit `A`, then chase. The hazard is real: revision is **not uniquely determined**. Retracting facts to
admit `A` in a tightly-linked graph can be done many ways (the multiple-maximal-consistent-subsets
problem, exponential). Stalnaker assumes a single closest context; Lewis admits ties, and resolving a
tie means quantifying over *all* closest contexts. Naive generative revision is a branching bomb: one
retracted fact (a failed mitigation) can break dozens of dependencies, each with several minimal repairs.

`logic:` does not defuse this by computing the permutations. It takes the design's own clause
literally — **closeness is declared data, not a fixed semantics** — and recognizes that the selection
among minimal revisions is exactly AGM's **epistemic entrenchment** (Gärdenfors–Makinson: entrenchment
orderings correspond to revision functions; a *total* order gives a unique, maxichoice revision). A
declared entrenchment ordering yields a single revised context, with no branching.

**Deterministic revision** is the default, and the only mode generative revision runs in unless a slice
opts out:

- **Ties are broken by declared type-level priority** — an entrenchment ordering for which `logic:`
  already owns the vocabulary: norm precedence (`gmeow:overrides`, `gmeow:AuthorityLevel`'s
  `absolute ≻ high ≻ medium ≻ conditional`, `gmeow:PrecedenceTenure`), risk grading
  (`gmeow:moreSevereThan`, `causalModality`'s `necessitates ≻ promotes ≻ enables`), source warrant
  (`gmeow:SourceTier` / `EvidenceClass`), and the `gmeow:sharpens` poset. The revision retracts the
  *least entrenched* facts first; a total order picks exactly one context.
- **A genuine tie is not enumerated.** If the order is partial and leaves two minimal revisions
  incomparable, the solver does **not** branch — the revision *ran* but selected no unique context, so
  it returns `information = undetermined` (the classification is not established because the selection
  was ambiguous; this is neither the conclusive Belnap `neither` nor `not-evaluated`) within budget.
- **Multi-context quantification is opt-in and budget-capped.** A slice that needs Lewis ties — `C` in
  *every* closest `A`-context (skeptical) or in *some* (credulous) — may request it as a non-default
  contract under a hard branch budget that degrades to an incomplete result on exhaustion. Never the
  default, never unbounded.

This resolves a tension with the no-privileged-fact discipline. The entrenchment ordering does **not**
globally privilege any fact; it is **local to one revision, declared for one counterfactual**. The
result is frame-relative — *"the closest `A`-context according to entrenchment `O`"* — itself a
standpoint-indexed claim. A different declared closeness yields a different counterfactual, and the two
**coexist** like any contested fact. Determinism per ordering; plurality across orderings.

### Decidability across the context strata

Frame-indexed and type-level reasoning are **certified-decidable** — the former because the context
set is finite and materialized, the latter because the no-occurrence gate keeps everything type-level.
Generative counterfactual reasoning is undecidable in general; it is enabled but governed — contexts
are constructed **lazily per query** (only the closest-`A`-context the goal needs), under the resource
budget, with certified fragments for the structured revision patterns the slices use (a lapsed
mitigation, a single counterfactual antecedent, a one-step belief update). The no-occurrence gate is
itself a reusable certified invariant: a contract may assert that a derivation introduces no token
occurrences, keeping type-level counterfactual reasoning inside the decidable region.

---

*Works cited here — AGM revision, Lewis/Stalnaker counterfactuals, Church/Turing, HiLog, F-logic,
the chase, well-founded and stable-model semantics — are listed in
[LOGIC-REFERENCES.md](LOGIC-REFERENCES.md).*
