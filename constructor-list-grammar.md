# Constructor-owned list grammar continuation

The full logic issue and PR goal remains active. This is a local implementation
checkpoint, not corpus qualification, a release, or a merge. The official PurRDF
Git-main pin remains `7734b013d19b68600ad24c4e8aa5a40955a8e89e` in root and fuzz.

## Behavior and ownership

The original list-membership feedback regression now passes without changing its
expectations: unbounded execution reports NonTerminatingExistential; budget 32
returns Exhausted, retains multiple witnesses, and never admits a termination
certificate. Grammar eligibility must be distinguished from extension completion
and from termination. Completed reads for negative/refutation consumers remain.

The new source_grammar module checks all reachable grammar writers for a selected
world before changing the exact local effect table. Only constructor-owned finite
oneOf lists qualify. Ordinary/property/external overlapping grammar writers,
open/cyclic/ambiguous lists, non-frontier allocation, erased field inputs, potential
grammar output, and known source aliases preserve the existing completion barrier.
The check inspects the complete selected source and nested quoted terms. Successful
capabilities are bound into both the schedule identity and native input contract.

Source admission still validates each actual selector and committed list, including
partial heads at a budget cut. A constructor capability is not deserializable;
serialized grammar-basis observations cannot authorize execution. Observations
distinguish pending writers, completed writers, and constructor grammar, and check
world/family ownership. A changed source cannot reuse the prior cached capability.

The prepared executor allocates tuple reifiers by relation/argument identity even
when a rule declares FrontierSkolem. Its constructor already rejects mixed tuple
and ordinary list heads; the certificate also explicitly requires the prepared
allocation to contain no tuple reifiers. This avoids relying only on a policy name.

No runtime dependency, resource cap, host probe, fallback, hook bypass, or weaker
original regression assertion was added. The source scan and commitment are
conservative; no corpus-wide capability or performance improvement is claimed.

## Evidence

- raw/constructor-list-clippy-v3.log: PASS, producer-independent workspace/all
  targets, warnings denied. Predates the final prepared-allocation guard/test.
- raw/constructor-list-runtime-v1.log: compilation FAIL, incorrect observation
  test API name; repaired to the existing validate method. No test verdict.
- raw/constructor-list-runtime-v2.log: PASS, 28/28 selected synthetic controls;
  nextest 5a2011a0-8ddf-46f3-9394-ba16985f138e, build 4m31s, run 0.204s.
  Includes the unchanged original feedback regression, conflicting-writer barrier,
  source-alias cache separation, finite multi-cell lists, external writer/world
  ownership, reduced frontier and nested-quote alias checks.
- raw/constructor-list-runtime-v3.log: compilation FAIL, two borrowed-string
  arguments in the added prepared-allocation test; corrected, no tests ran.
- raw/constructor-list-fmt-v6.log: PASS on the corrected final source.
- raw/constructor-list-clippy-v4.log: PASS, terminal exit 0, 54.29s;
  producer-independent workspace/all targets, warnings denied, on final source.
- raw/constructor-list-lint-v2.log: PASS, terminal exit 0, make lint including
  source-policy and corpus-purity seals, on final source. git diff --check passed.
- raw/constructor-list-runtime-v4.log: PASS, terminal exit 0, all 29 selected
  controls on final source. Nextest 24d24151-001a-424a-809e-7d613ecd4246;
  build 3m31s, execution 0.112s. Includes the new prepared-allocation control.

## Remaining work

The corpus producer/native canonical admission, complete prepared prover and
installed-binary acceptance, outstanding structured error propagation, browser
qualification, full make check/heavy hosted CI, review and ghprsq integration are
still unfinished. Prior producer/CI/browser evidence predates the PurRDF Git pin.
Do not treat the synthetic target or clippy-prebuild as the full gate.

The live open PR inventory in raw/open-prs-constructor-list.json contains 13 PRs:
8 project PRs and 5 dependency PRs. No new PR, commit, push, remote comment,
closure or merge occurred in this continuation. Main and sibling work are preserved.
raw/pr1802-constructor-list-current.json refreshes PR1802: OPEN/UNSTABLE at
b797a315e09cf6a118e0ca60975007a65f43a138. Generation (a) is cancelled;
generation (b), fixture-prefix and quality failed on the published source.
CodeRabbit's latest review remains COMMENTED on 2026-10-15. These hosted checks
do not qualify the local Git-pin migration or constructor grammar changes.

The next source error boundary already inspected is correspondence_exec.rs:
roundtrip execution/preparation failures currently fabricate violated outcomes,
and comparison failures retain only a string. Preserve operational causes and
real-counterexample precedence across cases when repairing that path; do not
return early on an unavailable case and conceal a later concrete refutation.
