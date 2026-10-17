# Current validation

## Current published head: e7be15f66

Remote branch subsequently advanced through merge a6c9c9986, bringing in current
main. The local checkout fast-forwarded to that published merge without source
edits. Run 38964236372 is terminal cancelled; replacement run 38964521632 is live
on a6c9c9986 with lint passed and builds/review pending. This task did not cancel
or restart either run. Earlier local focused source evidence remains scoped to
the e7be15f66 repair; current integration qualification is still pending.

This update supersedes the chronological hosted-status paragraphs below.
Run 38948027790 on 1da90b0c1 passed builds/lint but all three producer lanes
failed docs-render after native reasoning: valid input, unsupported evaluation,
16,575 unsupported observations. Generation (a) recorded reasoning at
4,707,532ms. See generation-a-final.log, generation-b-final.log and
fixture-prefix-final.log. This is failed hosted acceptance.

Issue 1833 repairs the documentation boundary with typed observations and removes
the old boolean/default verdict. All callers migrate together; native gate
grading and authenticated admission remain unchanged. All 43 selected synthetic
tests, strict producer-independent workspace/all-target Clippy and normal commit
hooks passed. See docs-observation-report.md and its referenced logs. The change
is pushed. New run 38964236372 and current-head CodeRabbit review are pending.
No local full gate or independent completion audit is claimed.

The obsolete 1792 and 1805 implementations already landed in PR 1802. Their
reconciliation on current main passed 21 native proof tests and 10 native RDF/GTS
tests. Audit records are preserved under reconciled-merged-work/ before removing
their obsolete worktrees and branches. Main helpers-hygiene failed; issue 1834
owns those structural failures. No ceiling was raised.

Native semantic completion, the measured 78-minute reasoning cost, full canonical
prepared-base acceptance and current-head hosted qualification remain unmet.

## Earlier fixture and recovery validation

Fixture contract: fixture-contract-tests.log reports all five fixture tests passed, including real DAG binding for every selected stage. The suite executes no stage and produces no corpus. It covers missing ancestors, receipt identity, candidate replay, source export parent binding and stable/persistent metadata admission. Rust and canonical TTL declarations agree. The goal-directed stage publishes only graph/goal-directed plus its own observation artifact, without retaining upstream handles or datasets.

Recovery source repair: imports/supersession-correspondences.ttl now supplies both executable path programs for the Writing System LossyLens refutation. The original formula and false mnemomorphic declaration remain intact; no recovery is asserted. A synthetic native regression checks missing programs remain an error, complete programs retain the violated section, honest loss passes declaration gates, and an overclaim fails. Combined execution is running; recovery-and-fixture-tests.log is not yet a passing claim.

Combined focused execution passed all 47 tests: 42 native correspondence-execution tests and five fixture tests. The new honest-loss regression passed alongside existing missing-leg, formula/body coupling, malformed recovery and branch-admission controls. See recovery-and-fixture-tests.log. This consumes only synthetic recovery graphs and DAG metadata; it does not produce the repository corpus.

Strict producer-independent workspace/all-target Clippy passed with warnings denied (clippy.log, 1m06s), using the target's existing four corpus-dependent consumer exclusions. Normal hooks passed for both the fixture and recovery commits; fixture-commit.log and recovery-commit.log retain the output. No local optimized producer/full gate is run because its fingerprint probes violate the user's host-probing prohibition. Exact-source explicit fixture production, generation and downstream consumers require hosted CI. No independent-agent API is available; no independent verdict is represented here.

Cache-key review: goal-directed consumes the complete compiled logic publication and a narrowed reasoning graph entity. scheduler.rs binds that entity to both graph and typed-handle commitments for live products and authenticated receipts. Therefore changes to the reasoning payload cannot reuse a proof observation under a graph-only key. Existing exact-delta admission and cache-size limits remain unchanged.

Publication: functional commits 693caea59 and ff3cfe1c6, then review repair 1da90b0c1, pushed normally; PR 1830 is open. stagectl pr-create created the PR but failed parsing the gh URL as JSON; resolved the existing PR through stagectl pr rather than retrying creation. Current CI run 38948027790 is live on 1da90b0c1: lint 117132294725 passed; producer-build 117132294625 and Rust prebuild 117132294301 are in progress. The prior run is not qualification for this new source. No manual cancellation or restart was issued. Hosted acceptance and updated review remain open; no merge-ready verdict is claimed.

Review repair: all 42 native correspondence execution tests passed with the new recoverable and independent path-body mutation controls. Strict producer-independent Clippy passed (review-clippy.log, 2m02s), followed by all normal hooks. Inline comment 4297931137 was answered with fix/evidence, and thread PRRT_kwDOSwdn1s6tU2Gd is resolved. Updated CodeRabbit review reports no actionable comments and passes documentation coverage at 93.33%; review-current-head.json retains the exact review. Hosted producer-build 117132294625 and rust-prebuild 117132294301 have now passed alongside lint. Both generation jobs and fixture-prefix are live; their acceptance and dependent gates remain open.
