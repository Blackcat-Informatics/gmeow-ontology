# Current validation

The latest bot comment (6224649194, updated 21:29:11Z) explicitly rate-limits the
new review: the organization has reached its spending cap and the next included
review is available in 17 minutes. No billing or review-limit setting was changed.
Its risk-coverage marker still names c74f5e804, so outer CodeRabbit SUCCESS is not
a current-head review of 425eb65de/c3123db1a. The displayed 65.52% coverage remains
the old review's failed check. New-head review/coverage stays pending until an
included review can run. Evidence: bot-review-after-docs.json. Other authorized
implementation and qualification work continues; this is not a global impasse.

Hosted job 117205051895 on current c3123db1a passed "Fingerprint the resolved
optimized producer recipe", the exact prerequisite that previously rejected
impl_diag_kind. The source-input ownership repair is therefore exercised by
the real hosted recipe. The optimized binary build is still live and later
verification/upload steps are pending; this is not a full producer/gate pass.
Raw step evidence: current-producer-job.json. Hosted lint also passed.

Documentation-only commit c3123db1a is pushed normally and every required hook
passed. Updated review coverage is pending. Existing functional and source-owner
checks retain their recorded scope; hosted current-head qualification remains open.

## Helper documentation precheck repair

CodeRabbit's c74f5e804 review has no source findings but fails documentation
coverage at 65.52%. Documentation-only commit c3123db1a addresses the affected
regression contracts and passed every normal hook. Push is in progress; new-head
review coverage and hosted qualification remain required. See review-docs-report.md.

## Producer ownership repair in progress

Commit 425eb65de is pushed normally. All required hooks passed. The source worktree
contains only this committed repair; new-head hosted acceptance and review remain
required. No successful full producer receipt is claimed for the previous failed job.

The four macro-ownership controls passed in 0.073s execution. Refreshed
helpers-hygiene passed and strict producer-independent workspace/all-target
Clippy passed with warnings denied in 4m50s. Commit 425eb65de passed every normal
hook; push is in progress. See macro-ownership-report.md. The full hosted producer
and authenticated consumer inventory remain unqualified on the repaired head.

Hosted producer-build job 117194390514 failed at producer-recipe before compiling
the producer: impl_diag_kind! had no selected source-input capability owner.
The exact log is producer-build-failure.log. This is a real prerequisite failure
introduced by the new shared macro, not successful hosted qualification.

The source-input registry now recognizes that syntax-only diagnostic macro beside
define_diag_kind. Recursive scanning of its arguments is unchanged. A new synthetic
inventory control requires included bytes to change the inventory and still rejects
unknown nested capabilities for both diagnostic macros. It runs through the real
InputInventory API without invoking a compiler/probe or any repository producer.
Four selected macro-ownership controls are live in session 8054; see
macro-ownership-tests.log. New commit, push and hosted requalification are pending.

Publication succeeded: c74f5e804 pushed normally; PR 1835 is open against main.
stagectl pr-create created the PR but failed parsing the returned URL as JSON;
stagectl pr resolved the existing PR, without attempting duplicate creation.
The source tree is clean and tracks its pushed origin branch. Producer-build,
lint and rust-prebuild are live; CodeRabbit is pending and CLA passed. Hosted
checks and review are required before merge readiness.

Commit c74f5e804 completed normally with all required commit hooks passed.
The commit contains only the eleven explicit source/debt files. Publication is
in progress; hosted qualification and independent review remain pending.

Strict producer-independent workspace/all-target Clippy completed successfully
with warnings denied in 1m18s. Normal commit hooks are running (session 67649,
commit.log). No source changes remain planned for this repair.

## Settled focused checks

make helpers-hygiene completed with exit 0: all native/Python scanner regressions,
the real 68-job ledger, banned-dependency checks and hash-domain self-tests passed.
No scanner implementation or exemption rows changed. See helpers-hygiene.log.

The freeze-boundary rerun completed with all 156 selected tests passing in 0.137s
execution (workspace compilation precedes execution). This covers the original
typed cause, code/grade registration, diagnostic catalogs and render snapshots,
affect producer/recovery/idempotence, sink language identity, duplicate collapse,
emission-order determinism and invalid-IRI refusal at freeze. See
focused-tests-freeze-boundary.log. The earlier third run executed the stale
pre-correction sink binary and failed exactly that case (155 passed, one failed);
it is retained as failing evidence in focused-tests-final.log.

Strict producer-independent workspace/all-target Clippy is live as session 14581,
log clippy.log. CLI-specific controls still require authenticated hosted inventory.
The historical pending sessions below are terminal and superseded by these results.

## Implementation and historical checks

Scope: shared helper ownership and lower-only debt repair on issue 1834's isolated
branch, based on origin/main b25d5c510. Source edits are not yet committed/pushed.

Implemented: one RDF sink and finite-f64 decimal lexical formatter used through
the existing math/lift dependency chain; explicit affect language literals;
shared diagnostic registration for generated and existing evidence payloads;
PurRDF field framing and registered Domain for reasoning program identity; four
diagnostic-catalog sets use FixedState. No dependency or scanner rule changed.

The debt tool removed seven obsolete body groups and the retired WorldStore
member from the existing default cohort. An explicit JSON comparison established
that every retained group is a subset of an existing group and every copy count
is non-increasing. Fixed-hasher, frame and domain counts equal their existing
ceilings 702, 52 and 45. No new group was accepted or count raised.

The first focused build failed on a new test's nonexistent dataset.len() API;
it was corrected to quad_count(). The second failed on a re-export name colliding
with math's distinct Rational formatter; the finite-f64 formatter now has an
explicit f64_decimal_lexical name. These logs are failing evidence, not passes.
The third run is live as session 59864, log focused-tests-final.log.

Source inspection of pinned PurRDF 7734b01 established invalid IRI admission is
deferred to freeze. The new sink test was corrected to assert serialize failure,
not intern failure, while the third run compiled other crates. It may still
execute the earlier test binary and therefore does not qualify the final sink
test until an affected rerun passes.

make helpers-hygiene is live as session 64271; helper-census self-tests passed,
Python scanner self-tests are pending. Strict Clippy, normal commit hooks,
publication and current-head hosted qualification have not run yet.

The development target excludes four corpus-dependent consumers, including the
CLI. Added CLI program-identity/failure-chain controls require the hosted
authenticated consumer inventory; they are not locally executed evidence.
No local optimized producer/full gate is authorized to probe the host. No
independent-agent API is available; no independent audit is claimed.
