# Issue #1894: Ground financial amounts, rates, prices and computations in math:

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

Parent: #1717. Blocks: #1717. This is one complete cluster from the original quantitative-grounding contract; the parent remains open until every remaining cluster and its production acceptance are complete.

## Scope and acceptance

Ground monetary amounts, rates and prices through the canonical quantity/dimension substrate and currency reference frames without treating currency as an SI base dimension. Ground interest/return computations in actual math: operations. Reuse the shared observations MonetaryAmount owner instead of minting a finance-specific duplicate. Audit and retire superseded scalar paths under equivalence-before-deletion with explicit guards. Add real math artifact references and the matching manifest dependency. Preserve frame-relative amount semantics, exact numeric values and existing financial callers. Acceptance must exercise actual finance consumers, generated constraints, correct and malformed dimensions/frames and computations; source counts alone prove nothing. Full required gates and shipped/installed acceptance are mandatory.

Principles 4, 7, 9, 11, 17 and 19 govern this delivery. No requirement is waived by this decomposition. External repository implementation or model/service lifecycle actions require separate authorization.


## Comments (1)

### paudley — 2026-10-21T07:01:20Z

# Finance grounding #1894

This delivery implements the remaining finance child after the independently
landed shared exact scalar prerequisite PR #1901. Child #1894 and parent #1717
remain open until actual production qualification and all acceptance pass.
Principles 4, 7, 9, 11, 12, 17 and 19 govern the source design.

## Delivery contract

- Retype the existing observations MonetaryAmount IRI as the abstract math:Quantity
  owner. Preserve exact decimal values, explicit dimensionless dimension and scalar
  currency frame. Retire monetaryValue and its assertion carrier deliberately.
- Migrate actual finance and shared coverage callers, related schema value mapping,
  journal aggregation and holding count. Holding count is a framed asset quantity,
  preserving the asset identity and separate monetary cost basis.
- Add canonical simple-interest P*r*n and holding-period-return (C-O)/O ASTs.
  Methods reuse CallableSignature/CallParameter/CallResult; parameters, mathematical
  symbols and runtime quantities remain distinct. No compound-interest claim.
- Public installed `gmeow finance evaluate SOURCE --subject REQUEST --out RESULT`
  consumes authenticated embedded owners and complete generated projections.
  Explicit finance-exact-decimal-v1 admission validates observation context, exact
  quantities, currency compatibility, period basis, roles and recipe identity.
- Minimal shared prepared-math seam derives financial result roles in the same
  native execution. Original subclass-typed inputs require native Quantity typing
  from retained authenticated named subclass facts; no asserted type patch.
- Native arithmetic uses an explicit named operation-contract projection, not a
  whole-ontology verdict. Full actual generated owner constraints remain mandatory,
  including active required procedural products. Unsupported required anonymous or
  quoted operation fields fail closed. Full owner identity binds result identity.
- Exact division rejects zero/nonterminating quotients; generic math/RDF division
  behavior remains preserved. Results and observationResult are fresh derived rows,
  retaining normal source/rule/proof evidence rather than asserted metric writes.

## Acceptance and limits

Full original issue scope remains mandatory: amounts/rates/prices, shared owner,
frame semantics, scalar retirement guards, real artifacts and manifest dependency,
real consumers, generated constraints, malformed dimensions/frames/computations,
full required gates, and shipped/installed acceptance. Authenticated fixture tests
consume producer-selected source/projections; no test produces the corpus.

This source lane has run no builds, tests, formatting, lint or producer. Parent
owns sequential qualification and later base integration. Missing authenticated
assets are blockers to executing their consumer coverage, never permission to
create placeholders, skip admission, produce a corpus from tests, or claim success.
See validation.md for the coverage index. Root qualification has now executed13 focused native/standalone-math tests successfully; formatting and mandatory corpus purity passed. Strict prebuild Clippy is running. Actual produced/installed/full acceptance remains unrun under the campaign exception.


