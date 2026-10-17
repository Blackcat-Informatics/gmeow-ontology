# Qualification index

The implementation executes native node-kind checks and equality filters through the existing integrity gate. It preserves the canonical ontology, failure markers, and structural-query sources. Both dependency manifests and lockfiles select the merged native equality API.

The earlier focused run passed all 49 selected tests, and strict Clippy passed for both affected packages. Those results preceded the final scope review and do not qualify the subsequent compiler changes.

Final self-review found two incorrect quantifier transformations. Both regression tests failed on the earlier implementation. The compiler now preserves the supported existential fragment and refuses unsupported quantifier alternation or variable capture. The first correction passed all 51 selected tests in `scope-tests.log`. The final correction, including three additional capture cases, passed all 54 selected tests in `final-scope-tests.log`. The other 3,320 tests were intentionally outside the selected scope; this is not a full workspace pass.

The final explicit tiny reason and conjecture witness producer passed after the scope correction; see `final-reason-witness-refresh.log`. Both diffs were inspected and their semantic records are unchanged. Both read-only consumers passed in `final-witness-consumers.log`. This producer does not build the repository corpus.

Strict affected all-target Clippy passed with warnings treated as errors for both logic crates and xtask; see `final-scope-clippy.log` and `provenance-boundary-clippy.log`. All 13 provenance controls passed in `provenance-boundary-tests.log`, including the staged/tracked audit boundary, nested lookalikes, and the real repository. All normal commit hooks passed in `commit-with-complete-shell-check.log`, and the branch was pushed normally. No hook was bypassed or disabled. The first failed attempts and their concrete repairs remain recorded.

PR 1844 is published. Its review capture contains no submitted reviews, inline comments or review threads. The bot remains in progress and is not approval. Hosted lint and helpers passed; build and broader production acceptance remain pending. The user's instruction permits integration with pending hosted checks. No independent review is claimed; available reviews are self-reviews. The actual merge and archive remain to be performed through ghprsq.

Hosted generation and fixture-prefix acceptance remain outstanding. The issue must stay open until those production checks pass. Focused fixtures do not establish production-corpus qualification. The full logic backlog remains active.

The native proof-vector repair passed its hosted package and conformance consumers. The borrowed equality API also passed its actual hosted native and WASM comparison tests, and its issue is closed. The separate imported-closure proof replay defect remains open and recurred in the latest native workflow. These results do not establish that the entire native workflow passed.
