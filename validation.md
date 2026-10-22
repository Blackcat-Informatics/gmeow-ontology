# Source-medium registry acceptance

Plan: plan.md. Twelve explicit source paths (eleven reviewed frozen paths plus the necessary one-line existing-test import) reviewed in independent-review.md. Root owns serial checks and ordinary hooks; source author audit_branches ran no checks. Producer-independent Make graph retains its repository warning/assertion/profile contract and admits no corpus identity.

| Criterion | Current evidence/result | Scope / limits |
| --- | --- | --- |
| Format and source hygiene | PASS, fmt.log; staged diff check and unstaged diff exit0 | Twelve explicit source paths staged, no manifests/lock or unrelated edits |
| Complete registry and shared adapters | Final registry-tests-final.log PASS, terminal0; build2m16s,43executed/43passed,runtime.045s; initial43PASS and failedcompile refresh retained | Expected43: new8 +ledger13 +sweep8 +norm4 +source-contract6 +tinyregistry4; independently authored records/Turtle only |
| Structural helpers | Initial helpers.log FAIL, terminal2; repaired helpers-refresh.log PASS, terminal0 | Enforced isomorphic definition_result/closure_result introduced by extraction; exact shared recorded-outcome decode now has one owner;68jobs45enforced0variants881unchangedunenforced0distinct2025files, no exemption |
| Strict caller compilation | PASS, strict-prebuild.log, terminal0,2m03s | clippy-prebuild all targets, producer-independent graph excludes embedded cli/lsp/gmn-wasm/validate-wasm |
| Source dependency admission | PASS, source-recipe.log, terminal0 | producer-recipe source identity only, no producer execution |
| Fast lint | PASS, lint.log, terminal0 | Normal make lint |
| Independent source/control review | PASS, independent-review.md | Root reviewed all actual writers/readers/consumers and shared helper callers separately from author |
| Normal signed commit/push hooks | PASS, commit.log/push.log, terminal0 | No bypass permitted |
| Current feedback/integration | PASS clean candidate; feedback has no finding; integration.md/review-debt.md | Complete paginated REST/GraphQL and actual-base candidate before ghprsq |
| Actual source registry/medium acceptance | UNRUN | No authenticated selected corpus, live GTS, training/emission/measurement or installed runtime claim |
| Full1776 | OPEN | Generic cache/selector, arbitrary-value/digest and other owners plus full consumer/installed qualification remain open |

Focused command:

    make nextest-synthetic NEXTEST_FILTER='(package(gmeow-errors) & (test(json_diag_) | test(json_recorded_diag_))) | (package(gmeow-pipeline) & (test(json_source_medium_) | test(json_medium_baseline_) | test(json_norm_claims_) | test(json_source_contracts_) | test(=medium::registry::tests::reads_dictionaries_corpora_schemas_media_and_the_assignment) | test(=medium::registry::tests::an_unknown_rep_and_an_unassigned_rep_raise_different_failures) | test(=medium::registry::tests::a_dictionary_missing_an_exactly_one_field_is_rejected) | test(=medium::registry::tests::an_unrecognized_strategy_individual_is_rejected)))'

Existing selected controls were read and assessed as independently authored; no canonical source include, discovery, selected artifact loader or corpus construction is reachable. The new actual decoder check intentionally preserves typed-only restoration, including original absence of semantic RDF revalidation. Four tiny existing controls cover actual source registry admission/lookup/refusal; they do not qualify live corpus.

Current user exception: no local optimized producer/corpus/fixture production/sync/full gate and sequential merges ignoring CI. Normal signing/hooks mandatory. Host/toolchain and sibling processes untouched. Prior1947 postmerge formal/inline/threads0/0/0 with no pagination remainder and one complete processing-bot comment; no approval claim.

Development finding after initial43PASS: extracting singleton selection made definition_result and closure_result isomorphic; helpers correctly refused. Root moved that exact concrete recorded-outcome job to errors::json::read_recorded_result<T>, with fixed RecordedDiag error payload and original type/cardinality wording, and removed both obsolete decoder bodies. Original payload Record::required owns Ok/Err pointers. Native Record::new stores no expectation string; its type check cannot fail after read_variant has proved the same immutable object, so using type_expected there preserves observable refusals. No generic Result framework/parser/bridge, configuration change or exemption. Source author independently reviews this root-authored repair. Repeat helpers and43controls on the repaired source; retain original logs unmodified. Format/diff refresh alreadyPASS.

First runtime refresh registry-tests-refresh.log ended terminal2 during compilation, with E0433 and zero tests executed: removing the production BTreeSet import exposed one existing test that inherited it via super::*. Root added the explicit std::collections::BTreeSet import directly to that test module; no test body/assertion or production behavior changed. Final scope is12paths (original11 plus this one-line test import). Format/diff finalPASS; runtime final43PASS in registry-tests-final.log (2m16build/.045s runtime). Successful helper refresh remains applicable: only that standard-library import changed afterward, no helper body, declaration/path include, hash domain or file inventory changed. Strict compilation/lint/normal hooks still required.

Separate hosted milestone: hosted-pr1943-verdict.md and full raw logs show actual ONNX producer/fixture parity PASS before runner shutdown. Independent original-contract assessment issue1902-completion-review.md supports closure; forge1902 verified CLOSEDcompleted03:47:55Z. This is separate evidence from current registry controls. Parent1897/1433/1803 and full generation/corpus/downstream/installed acceptance remain open.

PublishedPR1948 source2e2406bf2a897b3ef5e992c9a4c5ae6ecd31cb73. Stagectl response parse failure after successful creation recovered via one open-head lookup (pr-create-recovery.json), no retry. All required final local checks and normal hooks passed; exact-head CI pending under current exception.

Final feedback refresh: formal/inline/threads0/0/0; one complete processing-bot comment read/retained, no substantive request or approval. Source/base unchanged; CI39721758089 helpers/lint SUCCESS, producer/prebuild/bot pending. All original local failures are preserved with final passing dispositions.
