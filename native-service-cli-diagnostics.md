# Shared native service and consumer diagnostics

The full logic issue/PR goal remains active. The preceding correspondence turn
was verified progress (71 controls passed). This turn changes native service and
consumer diagnostic propagation and starts the authenticated producer build.
No commit, push, issue closure, external post or merge has occurred.

## Source behavior

DlReasoner now returns the shared diagnostic Result from every failing operation.
The old DlServiceError category/detail wrapper is removed. The registered
logic.dl-service kind owns the no-model/malformed/execution disposition and an Arc
of the original EntailError, exposing the native Error source. Role hierarchy
semantic refusals remain malformed, while native storage/graph/cancellation
refusals remain execution errors. Certified unknown/budget answers are unchanged.
The logic diagnostic catalog now also registers its previously omitted native
coverage and source-admission kinds; all 27 declared kinds are cataloged.

The installed reasoning input reader is one native module shared by the existing
commands. It returns only Diag; ReasoningInputFailure is evidence inside that
substrate, not another function error type. Input read, parse, syntax and base
failures retain original causes, operation, path and available input identities.
An unresolved base no longer silently falls back to parsing without one. The
existing PreservedDiagnostic wrapper retains the original native Diag and source.

Reasoning reports preserve public operation codes while projecting the complete
native diagnostic. No-model outcomes retain their existing negative verdict,
gate disposition and exit behavior. Malformed/execution failures remain separate;
entailment errors retain their native cause. The second-input failure path retains
the already-read premise identity. A missing diagnostic disposition is an explicit
hard execution-contract failure, never a guessed logical result.

GMN Turtle serialization and UTF-8 failures now use the shared diagnostic report
with their original native/UTF-8 source. Existing refusal of named-graph loss is
unchanged. No resource cap, weaker parser, new runtime dependency, host probe,
priority wrapper, compiler-profile relaxation or hook bypass was introduced.

## Verification and active operations

- raw/cli-service-diag-clippy-v1.log: terminal FAIL for one err().expect() test
  style finding. Repaired to expect_err, with no lint suppression.
- raw/cli-service-diag-fmt-v2.log: PASS.
- raw/cli-service-diag-clippy-v2.log: terminal PASS, 27.51s;
  producer-independent workspace/all targets with warnings denied.
- raw/cli-service-diag-lint-v1.log: terminal PASS, repository lint including
  source/corpus-purity seals. git diff --check passed.
- raw/cli-service-diag-runtime-v1.log: terminal PASS, all eight native service/
  catalog controls; nextest 5a3c1996-0ea6-42ad-aaf3-04119c416b63, build 4m13s,
  run 0.075s. Includes native no-model causes, a real malformed role hierarchy
  after input destruction, and native cancellation disposition. Session 87568 ended.
- raw/purrdf-main-producer-build-v1.log: terminal FAIL before compilation,
  unsupported implicit-name Git Cargo package ID ending in #3.0.1. Session 54090
  ended. A source repair now normalizes IDs from Cargo's own name/version metadata,
  retaining exact lock identity and rejecting contradictions.
- raw/git-package-identity-clippy-v1.log: terminal PASS, 4.73s, the same strict
  producer-independent workspace/all-target inventory on the controller repair.
- raw/git-package-identity-lint-v1.log: terminal PASS, repository lint.
- raw/git-package-identity-runtime-v1.log: terminal PASS, both new controls;
  nextest 8c870d74-6835-4c2c-bddb-4bc3964bd56c, build 3.10s, run 0.059s.
  Confirms metadata normalization without a name guess, contradictory metadata
  refusal, resolved lock revision binding and ambiguous lock refusal. These inert
  controls do not execute producer fingerprinting. Session 20963 ended.

Source inspection after that failed producer invocation discovered that the
existing authentication routine enumerates environment variables and reads CPU
identity (including /proc/cpuinfo). Invoking that routine conflicted with the
user's no-host-probing instruction. The user was told candidly. Do not rerun the
producer build/recipe/verification or another path into that fingerprinting
without explicit authorization. Do not remove or weaken authentication to bypass
the conflict. The parser repair and inert synthetic tests do not require it.
An explicit asynchronous authorization question is now pending. No answer has
arrived as of this checkpoint; elapsed time is not authorization.

The clippy target excludes gmeow-cli and three other corpus-dependent consumers.
It does not compile or execute the changed consumer CLI. Existing installed
consistency/no-model and malformed-input controls were strengthened for the new
structured fields but remain UNRUN on this source. Authentic producer/bundle
materialization must precede consumer compilation and acceptance.

While awaiting that decision, the old infallible fixture builder usage in
crates/gmeow-cli/src/commands/world_routing_tests.rs was migrated to explicit
success construction, retaining every claim-layer/provenance assertion. The actual
hybrid-query loader now handles WorldStore::new's shared diagnostic at its CLI exit
boundary and preserves parse/placement/load causes instead of flattening them.
raw/cli-world-fallible-fmt-v1.log and raw/cli-world-fallible-lint-v1.log passed;
final git diff --check passed. These consumer changes remain uncompiled/unexecuted. Settle consumer
compilation before claiming the Git pin qualified; do not weaken the inventory.

## Remaining full-goal work

The live diagnostic issue body is recorded in
raw/issue1762-cli-services-current.txt. Shared propagation and these checks alone
do not close its required crate-check, shipping/report acceptance or merge gates.
After successful, authorized producer authentication, make crate-check can run
through that exact producer. Complete canonical source/proof admission, prepared-canon producer
and embedding, request composition, authenticated corpus, browser and blinded
consumer acceptance, the full local/hosted gates, review and ghprsq integration
remain required. No corpus/CI acceptance or pipeline speedup is claimed here.
