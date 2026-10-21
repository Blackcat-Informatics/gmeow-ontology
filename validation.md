# Shared exact-scalar prerequisite validation

Status: source implementation; qualification in progress under root ownership.
Root's first focused synthetic run exited 2 at the logic build ownership guard:
inline test implementation must be extracted from math_evaluation.rs. The test
module was moved verbatim to math_evaluation/tests.rs; the guard is unchanged.
No test verdict was reached. Root owns the preserved raw failure log. Root's
initial make fmt passed; formatting must repeat after this source repair.
The delegated lane invoked no build, lint, producer or test.

Root's second focused run completed EXIT0: all 21 selected native/transport tests
executed and passed, with 7m32s workspace build time. The fixed-subject
quantityValue input/output flow was admitted without a termination workaround.
After that verdict, review found a mixed-concrete-AST-type dispatch ambiguity:
structural admission chose ApplicationExpression before SymbolReference while
the selected evaluator did the reverse. The selected profile now rejects
conflicting concrete forms and keeps MathematicalExpression base annotations.
New controls and changed profile behavior require a focused rerun; the earlier
21 passes do not verify that repair. Root's third run completed EXIT0: all 22
selected tests passed after the AST repair and integration of current main.

Subsequent source review confirmed a false-refusal path: native subclass rules
can legitimately derive an extra result type, violating the old total-row-count
completion test. Completion now requires Quantity type, exactly one dimensionless
dimension and one exact numeric value, allowing and preserving other native
consequences and their proof. A tiny subclass regression was added. Expression
lowering was split into coherent symbol/application/slot helpers without new
semantics. This changed profile coverage and the added regression need rerunning;
unmodified numeric and transport coverage retain the 22-test run's pass. Formatting
must repeat. CLI and full corpus acceptance remain unrun as documented below.

| Criterion | Source control | Status |
| --- | --- | --- |
| Four operations and unbounded exact values | math_evaluation_executes_four_operations_with_native_source_proofs | PASS in 22-test run; changed profile needs rerun |
| Native proof retains slots and quantity inputs | Same test, normal ResultProjection assertions | PASS in 22-test run; changed profile needs rerun |
| Binding/AST/value/literal identity and RDF order stability | math_evaluation_identity_retains_bindings_ast_values_and_literal_metadata | PASS in 22-test run; changed profile needs rerun |
| Missing/extra bindings, float/string/dimension/slot/cycle failures | math_evaluation_rejects_malformed_or_unsupported_selected_operations | PASS in 22-test run; changed profile needs rerun |
| Fresh identity, no asserted result reuse | math_evaluation_never_reuses_an_asserted_result_identity | PASS in 22-test run; changed profile needs rerun |
| Mixed concrete AST forms fail; shared base types remain valid | math_evaluation_rejects_conflicting_concrete_ast_forms_but_keeps_base_types | PASS in 22-test run; helper extraction needs rerun |
| Additional valid native consequences retain complete output and proof | math_evaluation_keeps_valid_native_subclass_consequences_and_their_proof | UNRUN: added after 22-test pass |
| Exact policy plan identity; default divide unchanged; no IEEE exact divide | exact_division_selection_changes_plan_identity_without_changing_default_division | PASS in 22-test run; unchanged |
| Typed policy survives RDF transport and changes content/projection keys | logic-compile::relational_core::numeric::tests::exact_division_relation_retains_policy_through_native_rdf_transport | PASS in 22-test run; unchanged |
| Actual public command outside checkout; Quantity and native proof | crates/gmeow-cli/tests/math_evaluation_cli.rs | UNRUN: mandatory codebook absent |
| Nonterminating/zero/duplicate-binding failure preserves prior output | Same CLI integration file | UNRUN: mandatory codebook absent |
| Canonical coat and fr/zh catalogs | logic/module.ttl and i18n/{fr,zh}.po | Source reviewed; authored generation/catalog acceptance UNRUN |
| Formatting, strict Clippy and applicable native inventory | Root selects supported Make targets sequentially | PASS: final fmt and strict prebuild Clippy; full suite UNRUN |

Focused native filter: `(package(gmeow-logic) & (test(math_evaluation) | test(physical::numeric::tests))) | (package(gmeow-logic-compile) & test(relational_core::numeric::tests))`.
CLI tests consume the binary selected
by the consumer runner and never produce a corpus themselves.

Root inspected main read-only: generated bundle present, mandatory
generated/projections/lang/gmn-codebook.cbor and authenticated fixture selector
absent. No placeholders, copied assets, guard weakening or local production are
authorized. Full gate, authenticated projection and fresh installed acceptance
remain unverified. Child 1894 and parent 1717 remain open.

## Final integrated evaluator qualification

Base integration preserved11 tracked and4 new owned source files byte-for-byte through a source-only stash, fast-forwarded to trust main, restored without conflicts, verified every source and dropped only that owned stash. Stage records/logs were excluded. Base delta is trust ontology/consumer registration and does not alter numeric/transport implementation.

`make fmt` passed after the final result-completeness/helper repair. `make nextest-synthetic NEXTEST_FILTER='package(gmeow-logic) & test(math_evaluation)'` terminated EXIT0: six tests executed and passed, native-qualification-4.log;6m09 build,0.325s execution. This covers four operations and unbounded values, native source proof, stable identity, malformed/unsupported cases, asserted result refusal, conflicting AST forms with valid base annotations, and extra valid subclass consequences/proof. Tests are independent tiny inputs; no corpus production or fixture admission.

The17 direct numeric/transport tests in the prior integrated22-test passing run remain applicable: numeric operator inventory, upstream policy adapter, scheduling, transport, errors and their direct tests were unchanged by the subsequent evaluator helper/completeness repair. These are reused results, not17 newly executed tests. The current evaluator execution covers the changed caller of those primitives. No whole-suite pass is claimed.

Strict producer-independent `make clippy-prebuild` completed EXIT0 on the final frozen source; clippy-qualification.log retains the raw result. All-target checking ran with warnings denied. It excludes the four generated-bundle consumers and cannot replace whole-workspace Clippy or installed CLI acceptance. CLI tests remain authored and UNRUN. Actual corpus/generated/installed/full gates remain UNRUN under the campaign exception. Normal signing/hooks remain required.

No source changed after the final evaluator pass and strict Clippy. Native review findings are resolved; final normal commit/push hooks and refreshed forge feedback/integration assessment remain required before landing.

Final publication preparation: normal source commit and normal push completed EXIT0. All15 source files were explicitly staged; Stage metadata remained untracked. No hooks or signing were bypassed. Refreshed review/check/base assessment and ghprsq archive gate remain required.

Final forge refresh: helpers and lint passed; optimized producer and rust-prebuild are in progress, bot review pending. Complete feedback inventories remain0 formal reviews/0 inline comments/0 threads and1 review-progress issue comment. No approval inferred. Refreshed base/head candidate is clean; no source changed after final native/Clippy/hook checks.
