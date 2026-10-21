# Issue #1894: Ground financial amounts, rates, prices and computations in math:

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

Parent: #1717. Blocks: #1717. This is one complete cluster from the original quantitative-grounding contract; the parent remains open until every remaining cluster and its production acceptance are complete.

## Scope and acceptance

Ground monetary amounts, rates and prices through the canonical quantity/dimension substrate and currency reference frames without treating currency as an SI base dimension. Ground interest/return computations in actual math: operations. Reuse the shared observations MonetaryAmount owner instead of minting a finance-specific duplicate. Audit and retire superseded scalar paths under equivalence-before-deletion with explicit guards. Add real math artifact references and the matching manifest dependency. Preserve frame-relative amount semantics, exact numeric values and existing financial callers. Acceptance must exercise actual finance consumers, generated constraints, correct and malformed dimensions/frames and computations; source counts alone prove nothing. Full required gates and shipped/installed acceptance are mandatory.

Principles 4, 7, 9, 11, 17 and 19 govern this delivery. No requirement is waived by this decomposition. External repository implementation or model/service lifecycle actions require separate authorization.


## Comments (10)

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


### paudley — 2026-10-21T12:07:19Z

Further source tracing found the next mandatory finance admission gap: all five input parameters and both output slots lack authored labels, definitions and owner references. Mapping generation reads the source module records directly; its later generated annotation completion cannot supply metadata required before FnO admission.

This repair adds the three required properties to those seven existing identities, with paired French and Chinese translations for the fourteen English literals. Computational edges, recipe topology, signatures and identities remain unchanged. Pure independently authored FnO controls will verify missing input/output metadata refusal and preservation; tests will not compile the canonical finance module or produce a corpus.

Actual hosted mapping rerun and full financial generated/installed acceptance remain open under the retained no-local-corpus/full-gate exception. No translation floor is changed without measurement. Refs #1894; Principles 1, 4, 6, 7, 17 and 19.


### paudley — 2026-10-21T14:57:35Z

Hosted [Pages generation after PR1917](https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/39600502877/job/119228681872) now reaches the catalog-family registry and fails at stage-mappings: the schema.org HowTo exemption counts eight grounding rows above its ceiling of seven.

The added row is the valid math:quantityValue to schema:value bridge introduced by the finance work. The existing HowTo family uses the entire schema.org prefix, mixing that quantity row with its seven procedure rows. The source investigation will preserve all mappings and the ceiling; no new headroom or weakened check is authorized.

A narrow value guard also exposes two existing DSL text-value templates: identifierValue and exifTagValue. Their values are not numeric quantities, and no canonical target law currently drives those template atoms. The repair needs a real owned derivation for those outputs, preserving their guards, literal values and loss semantics; marking them as grounded is insufficient. The separate checkout remains source-untouched while that compiler/ownership prerequisite is resolved.

Refs #1894 and #1717. Principles 4, 17 and 19 govern the shared source and domain ownership. Actual hosted mappings completion, downstream ONNX and full finance acceptance remain open. No local corpus, producer, fixture rebuild, sync or full gate is running under the retained campaign exception.


### paudley — 2026-10-21T16:00:23Z

The owner-set prerequisite now supports exact nonempty grounding-owner sets while refusing expansion/replacement or a changed counter on every existing guard. Current declarations and ceilings stay unchanged. Native/node correspondence validation remains required; raw assertions and incomplete cells remain residue. Installed human/TSV views include every sorted owner.

58unique focused controls passed, including standalone/text/relocation paths and canonical required-owner derivation. Strict producer-independent Clippy, helpers, source admission and formatting passed; independent review covered the installed CLI caller repair. The embedded CLI itself, native full i18n/metrics/corpus/installed acceptance remain unrun. The frozen reviewed-coverage map still includes retired carrier contexts and needs an explicit qualified hosted producer refresh after the registry repair; it will not be hand-edited.

The schema.org8-versus7 failure remains unresolved. The next whole unit must supply real Lang grounding, selected-cell evidence and mandatory catalog/runtime admission through all consumers before partitioning the family. This issue and its parent stay open. Principles 4, 7, 17 and 19.


### paudley — 2026-10-21T20:07:58Z

The current hosted mapping failure is being repaired without raising the old procedure ceiling or deleting the valid Math value bridge. The delivery introduces an explicitly owned Lang literal-value bridge, selected-template references with retained source evidence, mandatory runtime checks and a disjoint registry family for shared value bridges. Repository, bundle and standalone scoring consumers share that admission boundary; base measurement keeps its own base catalog.

All 28 new authored-data controls now pass, including literal preservation, ownership/tamper refusal, registry partition, exact-base inputs, file/directory scoring parity and explicit symlink refusal. Strict prebuild also passed. The earlier failing standalone refusal exposed a real omitted-input path and was repaired with its expectation intact.

Affected existing regressions are still compiling. Final helper/source/lint checks and a fresh hosted native translation-coverage artifact remain pending before PR publication. No local corpus, fixture or full-gate production has run under the recorded exception. Full quantitative, installed, corpus, metric and floor acceptance remains open; this progress does not close the parent. Principles 4, 7, 17 and 19.


### paudley — 2026-10-21T21:00:15Z

The owned literal-projection repair is committed and pushed with normal signing and hooks. It preserves the valid Math bridge, all seven procedure rows, the old ceiling and the default-zero guard. Selected correspondence evidence now reaches typed programs, whole-operation runtime admission, scoring and exact-base measurement; mapping-DSL translations follow their actual ontology owner.

Qualification now covers 290 distinct independently authored controls: 100 affected/native cases passed after four narrow regression repairs, with 190 unchanged passing cases reused after source review. The final 28 controls passed again, including the immutable-tree base fixture with no commit/signing/configuration overrides. Strict prebuild, helpers, source admission, lint and independent source review passed. These checks exclude embedded consumers and do not establish production corpus acceptance.

The exact-source [hosted native translation-golden refresh](https://github.com/Blackcat-Informatics/gmeow-ontology/actions/runs/39675042447) is running. Its complete artifact still requires review before copying any golden bytes and opening the PR. No local corpus/fixture producer, sync or full gate ran under the campaign exception. Full quantitative, installed, corpus, metric and floor acceptance remains open. Principles 4, 7, 17 and 19.


### paudley — 2026-10-21T21:58:28Z

PR #1934 is merged. It lands the literal correspondence/template, required catalog and same-base counter closure, separate schema:value guards preserving the procedure ceiling, and complete affected i18n owner routing. The exact native hosted golden was reviewed before copying. Focused controls, strict affected compilation, source/helper/lint checks, independent review and normal hooks passed. The signed Stage archive was verified and the completed checkout cleaned. Hosted producer qualification remains in progress; full corpus, installed, metrics/floors and the parent contract remain open. Principles 4, 7, 17 and 19.

