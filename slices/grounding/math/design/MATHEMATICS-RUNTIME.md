<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Mathematics — Runtime, Ingestion, and the Solver Seam

> The **runtime charter** of the GMEOW Mathematics design set: how mathematical, probabilistic, and
> statistical objects get *into* the canonical form, how the reified graph stays tractable, and how
> heavy computation is handed off to external engines and returned as provenance-bearing
> observations. It makes precise the seam the manifesto ([`MATHEMATICS.md`](MATHEMATICS.md)) names in
> passing as "future solver profiles", and it answers the three engineering-friction questions a
> maximal, fully-reified design necessarily raises: ingestion, ABox density, and the execution
> handoff. General ingestion and solver-profile implementation is **design-only** here and gated
> (§ Acceptance). The bounded exact Clifford calculation and selected exact scalar profile
> described below are separate implementations with separate acceptance evidence; neither
> establishes general solver handoff. This charter fixes the posture for later Rust work.
>
> **Reading this charter.** The declarative present tense is normative: "X is" means a conforming
> realization implements X, established by the slice's gates and the projection loss ledger — not a
> claim that any implementation already realizes X except as those gates demonstrate.

## Purpose

The mathematics slice defines the *objects* of mathematics; `logic:` owns the *reasoning
semantics*; the `observations` spine holds *results*. Between those three sits a runtime seam with
three jobs: **lift** external artifacts into the canonical AST, **keep** the reified graph
tractable, and **hand off** computation that neither the ontology source nor a classifier can
perform. This charter is that seam's doctrine. It is the mathematical peer of
`slices/grounding/logic/design/LOGIC-RUNTIME.md`.

## Selected exact scalar execution

`gmeow math evaluate SOURCE --subject ROOT --bind SYMBOL=QUANTITY ... --out RESULT`
selects `math-exact-decimal-v1`. This consumer command accepts a user's Turtle
file and runs outside a repository. The standalone input in
[`../examples/exact-scalar.ttl`](../examples/exact-scalar.ttl) can be evaluated as:

```sh
gmeow math evaluate exact-scalar.ttl --subject urn:scalar:quotient \
  --bind urn:scalar:dividend=urn:scalar:seven \
  --bind urn:scalar:divisor=urn:scalar:two --out result.ttl
```

This profile admits named `math:ApplicationExpression` and `math:SymbolReference`
nodes, named `math:ArgumentSlot` cells with exactly the integer indexes 0 and 1,
and binary `math:Addition`, `math:Subtraction`, `math:Multiplication`, and
`math:Division`. Each referenced `math:MathematicalSymbol` must have exactly one
explicit command binding to an explicitly typed `math:Quantity` with exactly one
`math:hasDimension math:dimensionless` and one integer or decimal
`math:quantityValue`. Duplicate, missing and unused bindings fail. No dimension
is inferred from a missing declaration. Other dimensions, operators, blank nodes,
quoted objects and named source graphs are unsupported by this selected profile
and fail with an actionable diagnostic. Ordinary literal annotations are retained;
a numeric value or slot index cannot have language or direction metadata.

The command first admits the canonical AST through the existing structural
lowerer, then lowers the selected operation into the same native numeric Formula
execution used by the reasoning engine. PurRDF owns unbounded integer and decimal
arithmetic. The explicit `logic:rdfNumericExactDivide` relation selects its exact
no-rounding division policy; division by zero and a nonterminating decimal such
as 1/3 fail the entire operation. Float and double inputs fail rather than enter
IEEE arithmetic. Ordinary `logic:rdfNumericDivide` and symbolic application
semantics retain their existing behavior.

The result is a fresh content-addressed `math:Quantity`, its derived numeric
value and explicit dimensionless dimension, accompanied by the normal native
closure proofs and `logic:ReasoningResult` RDF projection. The profile, selected
root, bindings, source AST edges and complete bound literal identities determine
the operation identity; original quantity/value/dimension and expression/slot
facts remain positive proof premises. An already asserted result identity is
rejected. Outputs are published only after complete native result validation;
errors leave an existing output untouched. This is an arithmetic result, not an
asserted observation or a financial metric. No currency compatibility, monetary
result typing, unit conversion, general dimension algebra, compound interest or
general solver handoff is claimed by this profile. Domain callers must admit
their own role/frame/basis contracts before using it and must supply their own
explicit observation context.

## Ingestion is a projection, run backwards

Researchers write `y ~ x1 + x2`, LaTeX, PyTorch, or a Jupyter lineage trace — never dense GMEOW
Turtle by hand. This is not a threat to the design; it is the projection doctrine applied to
*input*. Just as MathML and RDF Data Cube are lossy lowerings *out* of the canonical form
([`MATHEMATICS-PROJECTIONS.md`](MATHEMATICS-PROJECTIONS.md)), R formulas, LaTeX, MathML, and
OpenMath are **ingest surfaces** lifted *into* it. GMEOW already makes exactly this move for
queries: RDFQuery is "a front-end that parses **into** `logic:`", not a stack bolted onto SPARQL
(`slices/grounding/logic/design/LOGIC-RDFQUERY.md`). The mathematics slice repeats it for formulas and
models.

Two authoring paths converge on **one** canonical AST:

- **Lift** — Rust parser-compilers (the anticipated `math-parse` crate) ingest a stringified R
  model, a LaTeX or MathML fragment, an OpenMath object, or a notebook lineage trace and unpack it
  into the hyper-factored `math:MathematicalExpression` AST, symbol references, argument slots, and
  — for statistical inputs — the `math:StatisticalModel` / `math:ModelFormula` / `math:DatasetMatrix`
  structures. The ingested string is retained as a `math:parseSource` on the resulting AST for
  audit, never as the identity.
- **Author** — hand-authored content is written through a DSL/API over the AST (author in the
  canon, never in a generated projection — the project's standing "author in the DSL, not the
  generated artifact" rule), so a human never assembles raw reified Turtle.

The invariant across both paths: **the canonical form is always the AST.** A display or source
string is canonical identity only when a formula was ingested *only* at that fidelity and is
explicitly marked as such (the manifesto's rendering rule). A lift that cannot resolve a symbol,
that produces a variable neither bound nor declared free, or that would silently flatten structure
to a string, **hard-fails** with a typed diagnostic rather than emitting a degraded AST — the same
no-silent-fallback posture the whole slice enforces.

> **Ingestion hard rules.**
>
> - No silent fallback from a structured expression to a string. A parse that cannot produce a
>   well-formed AST fails; it does not emit a string-valued placeholder.
> - Every lift records its `math:parseSource` and, where the source format is lossy relative to the
>   AST (or vice-versa), a preservation record in the loss ledger.
> - No optional parser backends. A required ingest format is present and correct or the build hard-
>   fails; there is no feature-gated "best effort" lifter.

## ABox density and content-addressed interning

Full reification is deliberate and it is dense: a three-variable OLS regression generates a large
fact set describing the formula AST, the model assumptions, the data matrix, and the provenance.
The design does not apologize for this — the density *is* the fidelity — but it does not pay for it
by duplication.

**Shared subexpressions are interned by content-addressed identity, not copied.** The `logic:` IR
already gives formulas an alpha-normalized content-key identity (`content_key`, append-only formula
collection); the mathematics AST reuses that discipline. Two occurrences of the same normalized
subexpression — a repeated `xᵢ` term, a shared covariance structure, a common distribution-family
individual — resolve to **one** interned node referenced N times, keyed by its normal form
(`math:normalForm`, [`MATHEMATICS-EXPRESSIONS.md`](MATHEMATICS-EXPRESSIONS.md)). Structural
memoization is therefore a property of the canonical model, not an optional runtime cache: identical
structure has identical identity, so the graph grows with *distinct* structure, not with textual
repetition.

The consequences for the runtime:

- Named constants, distribution families, parameter roles, and standard operators are **library
  individuals** interned once and shared across every analysis that references them, not re-minted
  per document.
- Query access to dense ASTs relies on the property-path machinery `logic:` already projects
  (`slices/grounding/logic/design/LOGIC-PATHS.md`) — bounded-depth, named parametric paths over the
  slot/operand structure — rather than hand-unrolled joins.
- Blob-scale inputs (a full data matrix, an MCMC sample) are held **by reference**, not inlined: the
  IR carries a blob reference and origin, never the payload bytes, per the project's blob-by-
  reference doctrine. `math:DatasetMatrix` names and frames the data; it does not embed it.

## The solver seam — profiles, engines, and results as observations

`logic:` decides how facts participate in inference, but it does not — and must not — compute a
matrix inverse, run an MCMC sampler, or discharge a proof obligation inside an OWL/SHACL framework.
Heavy computation is handed off across a **solver profile**: a well-defined serialization interface
that pipes a GMEOW AST or model object out to an external engine and maps the returned output back
into the canon.

The discipline mirrors the `logic:` oracle contract exactly:

- **The external engine is a vantage.** SymPy or a CAS for symbolic manipulation, SciPy/NumPy for
  numerical linear algebra, Stan/PyMC for Bayesian sampling, Coq/Lean/an SMT solver for proof
  checking — each is a named vantage, and its output is not a free-floating truth.
- **The result returns through the process/result/claim split.** The solver run is a
  `math:InferenceRun` (a `gmeow:Activity`); its output is a structured *result object* (a
  `math:Estimate`, posterior, or test statistic; a `math:FormalVerificationResult` for a discharged
  proof obligation); and the *held claim* is a `gmeow:Observation` with the engine as `gmeow:vantage`
  and `gmeow:wasGeneratedBy` the run. None is typed as another
  ([`MATHEMATICS-EXPRESSIONS.md`](MATHEMATICS-EXPRESSIONS.md),
  [`MATHEMATICS-STATISTICS.md`](MATHEMATICS-STATISTICS.md)). A number that comes back from SciPy is a
  provenance-heavy statistical result, never a bare scalar re-injected without its act.
- **Native ⊇ oracle.** Where GMEOW's own native routines and an external engine overlap, the
  external engine is an oracle held to the native-superset discipline `logic:` applies to its FOL
  oracle: the engine cross-checks the native path; it is not silently the authority. Divergence is
  recorded, not averaged away.
- **The handoff is serialization, not semantics.** The AST is serialized to the engine's input
  (a projection, with its loss recorded if lossy) and the engine's output is lifted back (an
  ingestion, § above). No solver profile lets an external engine's model *become* the canonical
  model; it stays a computation over a projection, with the canon retained.

> **Solver hard rules.**
>
> - No computation is performed inside the OWL/SHACL classification path; heavy computation crosses
>   a solver profile.
> - Every solver result re-enters as an `InferenceRun` / `FormalVerificationResult` observation with
>   the engine as vantage and recorded provenance.
> - No silently approximate model: a bounded or approximate solver path declares its loss.

## Rust-first implementation posture

General-purpose ingestion and solver-profile implementation remains outside this charter's realized
surface, but the posture is fixed so later work is not improvised. The anticipated crate map:

```text
crates/math-ast/          typed expression AST, symbol resolution, content-addressed normalization
crates/math-parse/        parser-compilers/lifters from MathML, LaTeX-like, R-formula, OpenMath-like inputs
crates/math-proj/         projection emitters and loss ledgers (MathML, RDF Data Cube, STATO, QUDT, …)
crates/stats-model/       typed probability/statistics model validation (not inference-heavy at first)
crates/gmeow-python/      PyO3 surface, if and where needed
```

The hard posture when work begins:

- Rust owns parsing, AST validation, content-addressed normalization, and projection-loss
  accounting.
- Python exposes ergonomic DTOs and CLI bindings only — a surface over Rust behavior, never a second
  semantics (adding any Python requires explicit authorization).
- No optional math-parser backends; no feature-gated lifters.
- No silent fallback from a structured expression to a string.
- No silently approximate probability or statistical model.
- Every lossy transformation — ingest, projection, or solver handoff — produces a machine-readable
  preservation/loss record.

### Realized exact Clifford calculation

One bounded computation is already native rather than an external handoff. The public
`gmeow_math::clifford` module implements diagonal orthonormal `Cl(p,q)` with at most 64 generators,
`u64` basis-blade masks, exact checked rational coefficients, deterministic sparse multivectors,
geometric and exterior products, left contraction, grade projection, the three standard
involutions, and exact positive-extension embedding/split/join.

The eighth math producer invokes that same kernel for `Cl(12,0)`, `Cl(6,6)`, `Cl(13,0)`, and
`Cl(7,6)`. It emits exact dimensions, all generator squares, all distinct-generator
anticommutation witnesses, pseudoscalar squares, and both `8192 = 4096 + 4096` module splits into a
dedicated carrier graph. The ontology is the semantic frame and the Rust kernel is the calculation
authority; neither hand-authors the other's derived results. No E8 representation is emitted.

## Acceptance

The general runtime seam remains design-only until the slice's implementation gates pass. The exact
Clifford kernel and the explicitly selected exact scalar profile above are bounded implementations,
with their own acceptance evidence; they do not establish general runtime acceptance. Consistent
with the manifesto's posture, a conforming general realization is accepted only when:

1. A lift from at least one real ingest format (e.g. an R formula or a MathML content fragment)
   produces a well-formed, gate-passing AST, with `math:parseSource` retained and any loss
   recorded.
2. Content-addressed interning is demonstrable: identical normalized subexpressions resolve to one
   interned node, and the fact count grows with distinct structure, not textual repetition.
3. At least one solver profile round-trips: an AST serialized out to an external engine, computed,
   and returned as an `InferenceRun` / `FormalVerificationResult` observation with the engine as
   vantage.
4. Every unsupported ingest, projection, or solver contract hard-fails with a typed diagnostic
   rather than degrading silently.
5. Python-visible APIs, where added, are wrappers around Rust behavior rather than a separate
   semantics, and no generated artifact is hand-edited.

## Competency questions

The runtime seam is accepted only when it can answer these structurally:

1. From which ingest source (and at what fidelity) was this AST lifted, and what did the lift lose?
2. Which interned subexpression fragments does this analysis share with others, keyed by normal
   form?
3. Which external engine (vantage) computed this estimate or discharged this proof obligation, and
   under what provenance did the result re-enter as an observation?
4. Which solver handoffs, ingests, or projections recorded a loss, and of what polarity?
