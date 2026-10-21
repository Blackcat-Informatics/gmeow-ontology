# Issue #1894: Ground financial amounts, rates, prices and computations in math:

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

Parent: #1717. Blocks: #1717. This is one complete cluster from the original quantitative-grounding contract; the parent remains open until every remaining cluster and its production acceptance are complete.

## Scope and acceptance

Ground monetary amounts, rates and prices through the canonical quantity/dimension substrate and currency reference frames without treating currency as an SI base dimension. Ground interest/return computations in actual math: operations. Reuse the shared observations MonetaryAmount owner instead of minting a finance-specific duplicate. Audit and retire superseded scalar paths under equivalence-before-deletion with explicit guards. Add real math artifact references and the matching manifest dependency. Preserve frame-relative amount semantics, exact numeric values and existing financial callers. Acceptance must exercise actual finance consumers, generated constraints, correct and malformed dimensions/frames and computations; source counts alone prove nothing. Full required gates and shipped/installed acceptance are mandatory.

Principles 4, 7, 9, 11, 17 and 19 govern this delivery. No requirement is waived by this decomposition. External repository implementation or model/service lifecycle actions require separate authorization.


## Comments (4)

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


### paudley — 2026-10-21T08:40:24Z

# Owned source-capability and helper repairs

Based on main after transcode PR1906. This reused finance issue lane
repairs introduced gate defects after the financial implementation landed.
Original financial corpus/generated/installed acceptance remains mandatory and
open. This repair is not evidence that those requirements pass.

## Concrete failure evidence

- Hosted finance run39565507443, helpers job119106691430, and transcode run39566072124,
  helpers job119108532651: isomorphic diagnostic constructors in finance/math and
  rdf-collection21 copies versus debt19. The parent reproduced both via helpers.
- Hosted transcode run39566072124, producer-build job119108532904: json_record!
  has no selected source-input capability owner in transcode/src/lib.rs.

## Coherent repairs

1. Records::list and owner import consume pinned purrdf_core::walk_rdf_list,
   the same strict owner used by DatasetView::rdf_list_strict. The callbacks
   retain visited cells while the owner determines traversal and membership.
   This is the actual pinned API; rdf_list_with in census guidance is stale.
   Strict malformed-list behavior and the finance named-record/member policy
   remain mandatory. Required selected anonymous/quoted edges still refuse.
2. Exact collection-census.log corrected the initial attribution: the second
   introduced copy is Records::admit_owner, not the synthetic owner builder.
   Owner import now expands list cells and member records through the shared
   strict walk instead of its generic first/rest closure traversal. Full cell
   records, positive proof premises, unsupported-field refusal and authenticated
   owner override checks remain mandatory. Effective merged lists are also
   strictly checked, including rdf:nil. The synthetic builder was modernized
   through purrdf_core::build_rdf_list but was not the remaining census finding.
   No token relocation, marker, ceiling or exception hides the owner traversal.
3. One error::profile_result constructor owns the diagnostic kind/format, with
   math and finance profile identities supplied as data by both real callers.
4. The JSON macro capability requires the actual selected external purrdf-lex
   package, declared dependency alias and matching full Git revision/lock object
   from the canonical substrate repository. Portable ownership proves the same
   source from unambiguous declarations and the lock. No local/path/floating/fake
   package, conflicting alias/target, replacement or malformed package grants it.
5. Only external-prelude ::alias::json_record! and ::alias::json_string_enum!
   receive that capability. Existing nested-input traversal, opaque/module-input
   refusal and extern-crate shadow controls remain active. Transcode uses the
   qualified path; no bare-name allowlist or suppression was added.

No ontology, ONNX fixture, manifest pin, gate flag, helper debt/ledger or ceiling
changes belong to this repair. Parent owns sequential qualification, integration
and delivery. This source lane runs no formatter, checks, builds or producer.

See validation.md for the exact unrun coverage and required qualification.


### paudley — 2026-10-21T10:03:48Z

# Intentional finance carrier retirement reconciliation

Refs #1894; Principles 4, 7 and 17.

Hosted production of PR1907 passed producer-build, helpers, lint and rust-prebuild, then failed stage-compile-logic on exactly one completeness discrepancy: retired gmeow:monetaryValue remained in the frozen ledger. Evidence: https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/39575061588/job/119149124349 . Current canonical Turtle inputs are unchanged since that run.

Remove only that retired property from the deliberately maintained ledger. Preserve the gate and all other entries. MonetaryAmount retains a decimal all-values restriction, closed required quantityValue path and maximum-one decimal constraint. math:quantityValue is not globally functional and must not gain a global carrier. Correct two obsolete references to a nonexistent pinning test to describe the actual compile_logic integrity owner.

Independent source review confirmed this scope and the stale references. Existing synthetic functional-carrier tests qualify missing/unexpected, duplicate, orphan and reintroduced marker detection. They cannot qualify equality against the full merged corpus. No local corpus production, sync or full gate under the reconfirmed campaign exception. Normal commit/push hooks remain mandatory. Hosted production rerun must establish full corpus equality; issue1894 remains open for all original acceptance.


### paudley — 2026-10-21T11:35:55Z

Hosted production now completes the repaired functional-carrier check and reaches mapping generation. Both generation and fixture-prefix stop at the same finance parameter: the FnO exporter rejects its required mathematical-symbol binding. Evidence: https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/39585407453/job/119190698964 .

The financial runtime consumes that binding to connect distinct callable parameters, mathematical symbols and runtime quantities. The repair will preserve the typed canonical symbol and method-recipe references beside the FnO interface, including the method type and scope-note metadata. Admission remains role-specific and rejects malformed targets, unpaired method/recipe declarations and unrelated unsupported fields. The catalog will not claim to contain an executable mathematical recipe.

Pure independently authored signatures will qualify preservation and failure paths. No local corpus production or full gate under the reconfirmed campaign exception; normal hooks remain mandatory. Actual financial generated/installed acceptance and the downstream ONNX parity check remain open. Refs #1894; Principles 4, 7, 17 and 19.


