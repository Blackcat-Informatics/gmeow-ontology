# Issue #1717: grounding: ground the six quantitative slices in math: (finance, risk, quality, evidence/trust, model-serving, diagnostics)

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, cross-cutting, complexity-hard

## Body

Split out of #1439, whose first checklist item — the `gmeow:Quantity` ≡ `math:Quantity` reconciliation — is complete and verified. This carries the remaining six clusters so the reconciliation can close without dragging an open sweep behind it.

Child of #1433.

## Why

The `.goals` MAXIMAL GROUNDING mandate says the three grounding slices ground all other slices. Measured against `main` @ c2b03aba6, `math:` references in each slice's `module.ttl`:

| slice | `math:` refs | note |
| --- | --- | --- |
| affect | **38** | the precedent — nearest-prototype on math geometry |
| diagnostics | 3 | already grounded via `gmeow:gradeOrdinal` — see struck item below |
| quality | 1 | effectively ungrounded |
| finance | **0** | |
| risk | **0** | |
| evidence | **0** | |
| trust | **0** | |
| model-serving | **0** | |

Numbers in those slices are ungrounded scalars.

## Work (one PR per cluster is fine)

- [ ] **finance** — monetary amounts, rates and prices carry `math:Quantity` dimensions; interest/return computations reference `math:` operations.
- [ ] **risk** — risk scores/likelihoods ground in the probability layer (`Prior`/`Posterior` where Bayesian; never bare floats).
- [ ] **quality** — quality metrics as dimensioned quantities; aggregate scores with declared estimators.
- [ ] **evidence / trust** — strength/confidence values ride the probability-vs-confidence never-conflate rule (P19): confidences stay `logic:confidence`, probabilities require a model.
- [ ] **model-serving** — tensors/metrics ground in the linear-algebra layer (`TensorComputationGraph`, `math:LossFunction`).
- [x] ~~**diagnostics** — extend the 3-ref toehold to graded-severity quantities.~~ **Struck: already grounded.**
  `gmeow:gradeOrdinal` embeds each truth-order chain into (ℤ, ≤) — severityInfo 0, Note 1,
  Warning 2, Error 3 — with the one-way bridge `math:integers math:formalizes gmeow:gradeOrdinal`
  declared in the math slice, and the term states it is "the Principle-19 MATH grounding of the
  truth ordering" and "NOT a parallel gmeow:Grade subtree". Severity is ORDINAL, not dimensioned;
  pushing it toward `math:Quantity` would conflate an order-embedding with a magnitude. The low
  reference count measures how compactly it was done, not absence.

## Dependencies

Blocked by the probability and statistics layers **only where a cluster needs them** — risk and evidence. finance, quality, model-serving and diagnostics are not blocked and can start now.

## Acceptance

- No inherently quantitative slice sits at `math:` 0.
- Each grounded slice records `math:` in its manifest `gmeow:sliceDependsOn` set.

## The pattern to follow

`affect` (38 refs) is the worked precedent. `observations` is the smaller one, and is now complete end to end: it declares `slices/math` in `gmeow:sliceDependsOn`, consumes `math:Quantity`, and its structural tests *assert the legacy `gmeow:` quantity vocabulary is absent* rather than merely not using it — see `slices/core/observations/tests/structural.ttl:136`. Grounding a slice is not finished until the retirement is guarded that way.

## Comments (2)

### paudley — 2026-09-05T03:44:35Z

## Two scope corrections found while surveying the clusters

### The `diagnostics` checkbox looks misconceived — recommend dropping it

I inherited "extend the existing toehold to graded-severity quantities" from #1439 without examining what the toehold is. It is not a toehold; it is a complete and deliberate grounding.

`gmeow:gradeOrdinal` embeds each diagnostics truth-order chain into (ℤ, ≤):

```
gmeow:severityInfo    gmeow:gradeOrdinal 0 ;
gmeow:severityNote    gmeow:gradeOrdinal 1 ;
gmeow:severityWarning gmeow:gradeOrdinal 2 ;
gmeow:severityError   gmeow:gradeOrdinal 3 ;
```

with the one-way bridge declared on the math side —

```
slices/grounding/math/module.ttl:1885
math:integers math:formalizes gmeow:gradeOrdinal .
```

— and the slice's own definition is explicit that this is *"the Principle-19 MATH grounding of the truth ordering"* and *"NOT a parallel `gmeow:Grade` subtree — it annotates the EXISTING chain individuals."*

Severity is **ordinal, not dimensioned**. Pushing it toward `math:Quantity` would conflate an order-embedding with a dimensioned magnitude — the opposite of what P19 asks for, and a regression against a grounding that already states its own rationale. The low `math:` reference count is a measure of how compactly it was done, not of absence.

Recommend striking this checkbox and recording diagnostics as **already grounded**.

### `quality` is the best first cluster — its commitment is already written, just not axiomatised

`slices/core/quality/module.ttl:121` already says:

> the quality result is the `observationResult` (typically a `math:Quantity`)

That is the intended semantics stated in prose, in `skos:definition`, where nothing can enforce it. Turning it into an EL-safe `logic:` axiom in `module.ttl` — and recording `math:` in the manifest's `gmeow:sliceDependsOn` — is exactly the grounding this issue asks for, with the modelling decision already made and documented. No new ontology design is required, which is not true of finance (currency-as-dimension) or model-serving.

Suggested order for the four unblocked clusters: **quality → diagnostics (drop) → finance → model-serving**, leaving risk and evidence/trust behind the probability and statistics layers.

### paudley — 2026-09-05T17:06:44Z

## Correction: `quality` is *not* the easy first cluster — I was wrong

In my previous comment I wrote:

> `quality` is the best first cluster — its commitment is already written, just not axiomatised … Turning it into an EL-safe `logic:` axiom in `module.ttl` — and recording `math:` in the manifest's `gmeow:sliceDependsOn` — is exactly the grounding this issue asks for, **with the modelling decision already made and documented. No new ontology design is required.**

That is wrong, and I proved it wrong by trying it (PR #1719). Recording the failure so the next person does not repeat it.

### 1. Prose is not evidence — the dependency gate says so

Declaring the manifest edge alone reds the gate:

```
error slice-ownership.stale-dependency: .../slices/quality declares gmeow:sliceDependsOn
.../slices/math but no artifact references it
```

The analyser requires an **artifact term reference**. The slice's only `math:` mentions are inside `skos:definition` / `gmeow:howToUse` **string literals**, and a string literal creates no closure obligation and no reasoning link. Correct behaviour: it forces the dependency graph and the axiom graph to agree. `examples/` does not count either.

So the manifest edge cannot be added first and backfilled — the axiom has to come first.

### 2. The obvious axiom is false, and the word "typically" is why

The commitment I called "already made" reads (`slices/core/quality/module.ttl:121`):

> the quality result is the `observationResult` (**typically** a `math:Quantity`)

I read "typically" as hedging. It is not — it is load-bearing. The axiom it suggests,

```turtle
gmeow:QualityAssessment rdfs:subClassOf [
    a logic:Restriction ;
    logic:onProperty gmeow:observationResult ;
    logic:someValuesFrom math:Quantity ] .
```

is **false**. An ISO 19157 assessment may yield a *categorical conformance verdict* rather than a magnitude — classification correctness, or conformance to a topological rule set, both of which this slice already seeds as dimensions. `gmeow:observationResult` carries `rdfs:range logic:Individual` precisely to admit both, and its own definition enumerates "a concrete entity, coordinate set, instant, abstract quality value, `math:Quantity`, or another particular".

### 3. The magnitude commitment lives on the dimensions, where EL cannot reach it

What *is* true is per-dimension: positional and temporal accuracy are inherently metric; logical/topological consistency is not. But `gmeow:QualityDimension` is an **`AbstractIndividualType` — an open value vocabulary of individuals**, and Principle 9 explicitly forbids per-dimension subclasses ("New dimensions are added by minting fresh individuals, not by creating new classes"). There is no class to hang an EL restriction on.

### 4. What the cluster actually needs (a real modelling decision)

Grounding `quality` requires choosing one of:

- **(a) Subkind split** — introduce a magnitude-valued subkind of `QualityAssessment` (categorical verdicts staying on the base kind) and put the `math:Quantity` restriction there. Cheapest in EL; costs a new class and a classification rule for which assessments land in it.
- **(b) Dimension-typed results** — give `QualityDimension` individuals a declared result type (a `math:Dimension` reference for metric dimensions, absent for categorical ones), grounding via the *dimension* rather than the assessment. Keeps Principle 9 intact; the enforcement is then a `logic:Constraint` + `logic:Formula` (cross-node, procedural), not an EL axiom, per the LOGIC-VALIDATION split.
- **(c) Declare it out of scope** — accept that a quality *verdict* is not inherently dimensioned and that the grounding belongs to the `observations` stack it already reuses, which **is** grounded. Then `quality` legitimately sits at `math:` low and the acceptance criterion "no inherently quantitative slice sits at `math:` 0" needs its premise re-examined for this slice.

My read is **(b)** is the honest one — it puts the commitment exactly where it varies — but it is a design decision, not a transcription.

### Consequence for this issue

- The suggested order **quality → finance → model-serving** should be dropped. `quality` is *harder* than I claimed because its grounding is conditional on a dimension, not universal on a class.
- The acceptance criterion *"Each grounded slice records `math:` in its manifest `gmeow:sliceDependsOn` set"* is sound but is an **output** of the work, never a step in it — the gate will reject it until a real term reference exists.
- PR #1719 is reduced to what survives: the two prose literals now state the magnitude/categorical split explicitly instead of hedging with "typically", so the next author meets this constraint at the point of authoring rather than rediscovering it from a red gate.

