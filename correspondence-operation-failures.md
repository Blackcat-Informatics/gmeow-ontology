# Native correspondence execution failures

The full logic issue/PR objective remains active and incomplete. This continuation
implements and checks the actual law execution path, not merely an error wrapper.
No commit, push, external comment, issue closure or merge has occurred. Preserve
the preexisting combined integration and sibling worktrees. The official PurRDF
Git-main pin remains unchanged.

## Behavior

Seed construction, query preparation/evaluation, recovery lowering, native carrier
materialization and canonical comparison failures now return the shared diagnostic
Result. Original native causes and context survive. Neither a false countermodel
nor a message-only Unknown stands in for an operation that could not execute.
Missing executable legs required by recovery evidence and execution-namespace
collisions are input errors. An empty independent case domain remains Unknown;
successfully evaluated missing/spurious assertions or endpoint differences remain
real violated obligations.

Prepared source/view roundtrips and recovery cases use one deterministic case
fold. Operational failures do not conceal a later concrete counterexample. A
decisive countermodel retains earlier LawCaseFailure records, including case
identity and the original structured diagnostic. Without a counterexample, any
failed case returns an error even if other cases succeeded. Single failures retain
their live native cause; multiple failures preserve every RecordedDiag in a typed
RDF JSON observed slot through shared diagnostic serialization. Normal successful
execution adds no diagnostic serialization or RDF serialization/reparse boundary.

The fallible comparison propagates through atomic and composed lens checks.
program_verdicts and the compile_program executor callback are fallible, so failed
execution cannot publish compiled law claims. Actual pipeline, conformance and
developer-feedback callers preserve those diagnostics. Synthetic callbacks and
controls now explicitly require successful operations; malformed-input controls
require a diagnostic rather than a fabricated violated law. Real countermodel
assertions remain in place.

## Verification

- raw/correspondence-errors-clippy-v1.log: terminal FAIL, six redundant closures
  and eight remaining test API errors. Fixed without weakening warning policy.
- raw/correspondence-errors-fmt-v4.log: PASS on corrected source.
- raw/correspondence-errors-clippy-v2.log: terminal PASS, 51.41s,
  producer-independent workspace/all targets, warnings denied.
- raw/correspondence-errors-lint-v1.log: terminal PASS, full repository source
  lint including corpus-purity and formatting seals. git diff --check passed.
- raw/correspondence-errors-runtime-v1.log: terminal PASS, all 71 selected tests.
  Nextest 35b409d7-2857-4505-8611-70403a8d7850; build 4m29s, run 0.197s.
  Covers native law/recovery, atomic/composed lenses, synthetic pipeline law and
  the production compiler callback. All three new failure-preservation controls
  passed. The two producer-backed SIOC cases are excluded; no test is authorized
  to build or invent corpus credentials. Final git diff --check passed.

New controls exercise refused comparison without a countermodel; an earlier
refused case followed by a real counterexample; refusal followed by success;
multiple failed cases surviving diagnostic serialization; and propagation of an
executor error through the production compiler callback without publishing claims.

## Full-goal boundary

This is not the full local gate, authenticated producer/corpus acceptance, browser
qualification, hosted CI, independent final review or ghprsq integration. Earlier
producer/browser evidence predates the Git pin. None of those requirements is
waived. Complete canonical source/proof admission, prepared-canon production and
embedding, request composition, native/external proof and blinded installed-binary
acceptance remain required, alongside the outstanding PR scope.

Live stagectl issue reads are retained in raw/issue1768-next-prover-current.txt
and raw/issue1770-next-proof-current.txt. Both remain OPEN. Their complete-canon
and prepared-base requirements remain real; the historical 327 blocker count is
not a measurement of the new product. Current prove.rs still lowers caller-only
files through PreparedLogicSource, so distribution evidence is insufficient.
No host/resource probe or resource-control wrapper was used.
