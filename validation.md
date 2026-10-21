# Native cost/soak JSON acceptance index

| Behavior / check | Actual result | Scope / evidence |
| --- | --- | --- |
| Initial native family |13 authored tests PASS,1110 outside selection skipped | native-tests-before-review-fix.log; before the independent admission finding was repaired |
| Final shared codec/admission/report controls |15 executed,15 PASS,1110 outside selection skipped | make nextest-synthetic NEXTEST_FILTER='package(gmeow-pipeline) and test(json_bench_cost_)'; native-tests.log |
| Existing report regressions |2 executed,2 PASS,1123 outside selection skipped | Existing contextual grounding and incoherent rule-parallel refusal tests; existing-report-tests.log |
| Strict prebuild Clippy |make clippy-prebuild exit0,27.88s | All producer-independent workspace targets, -D warnings; clippy.log |
| Helper ownership |make helpers-hygiene exit0 | helpers.log; no policy/debt changes |
| Producer source admission |make producer-recipe exit0 after review repair | producer-recipe.log; source identity resolution only, no optimized build or corpus production |
| Formatting / whitespace |make fmt exit0; git diff --check exit0 | fmt.log |
| Independent review |review-cost-json.md: original finding, producer proof and final repair disposition | No remaining blocking finding; reviewer ran no checks |

The fifteen new controls use independently authored tiny complete JSON artifacts,
plain records and temporary files through both actual report functions. They
pin complete literal ledger/soak outputs, all five fragment profiles, native
required/optional/duplicate behavior, u64/u32/i128 extrema and signed-zero
distinctions, exact four-slot tuple order/arity/native pointers, missing selected
evidence, legitimate absent proof height, schema/count/identity/nonvacuity/scope
refusals, sorting, malformed/trailing/UTF8 input, gap and ruleparallel refusals.
The literal soak digest was independently computed by b3sum on four authored
bytes hex431f440a, not recomputed through the renderer under test.

The two unchanged existing tests read committed cost-baseline JSON metadata,
mutate temporary JSON and assert report refusals. They execute no engine, corpus
loader or producer. They preserve contextual grounding and ruleparallel behavior;
they do not establish actual produced report byte parity or benchmark acceptance.

Independent review found an inherited gap where zero comparison evidence or
empty identities could still produce a HELD claim. Source proof establishes
that successful producer fragments contribute1/1/2/4/5 comparisons and ledger
keys exactly equal case corpora. Root repaired the touched shared boundary to
require nonempty engine/ledger/finding identities, positive ledger comparison
counts and exact corpus scope. Two controls were added; the original13
expectations were unchanged. No raw case/comparison count equality, digest format
or recomputation, or globally mandatory optional panel was introduced.

All13 private records now use native FromJson/Record; both actual readers share
parse_cost_artifact. Unsigned fields use strict native as_u64 with checkedu32.
Signed i128 admission intentionally retains -0: selected Serde1.0.151
de.rs356–379/418–441 scans and parses integer text, matching native as_i128.
Native items_with/Within supplies tuple pointers. Known duplicates refuse,
unknown fields remain ignored and ledger-map duplicates remain last-wins.
Positional record arrays are deliberately refused and documented. Missing
selected measurements no longer become0/false; absent proof height becomes an
em dash. Existing gap refusal, four-worker validation, report order/arithmetic
and exact corpus+US+digest+LF soak framing remain.

User retained no local optimized producer, corpus/fixture construction, sync or
full gate and sequential merges without waiting for CI. Actual committed-stage
byte parity, produced/installed/authenticated corpus, full consumer-inclusive
Clippy and full1776 acceptance remain unrun. The supported synthetic/prebuild
graph excludes gmeow-cli, gmeow-lsp, gmeow-gmn-wasm and gmeow-validate-wasm.
No benchmark/engine execution, baseline refresh, manifest/lock/build-profile/
fixture/authentication/generated edit or performance claim. Normal commit/push
hooks remain required; actual results will be recorded before merge.

Normal commit and push exited0 with required hooks; actual results are in commit.log/push.log. PR1921 created once, then recovered by forge lookup after stagectl returned its known JSON-parser error; no duplicate creation retry.
