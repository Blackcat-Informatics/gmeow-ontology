# Current closure acceptance

No implementation changes or new PR are required: the inspected repair and benchmark are already in current main through PR 1802. The original ghprsq base/head/result/stage audit refs exist. Current source matches main for the affected paths. The merged PR has no inline review comment on the native source-support, case-split or benchmark paths. This local assessment is self-review, not independent approval.

| Requirement | Evidence | Verdict |
| --- | --- | --- |
| Stop rebuilding the full proof index per restriction/obstruction | NativeProofLookup owns a lazy index for an immutable batch; class-result validation creates it once outside both loops; nested proof validation accepts the same lookup | Source verified |
| Growing admission uses exact original proof positions | NativeFamilyInput::support_with_sources uses the existing by_id index and checks the selected slot's actual proof identity | Source verified |
| Preserve missing/wrong-ID/world/contract/growth/replacement and class/branch behavior | Existing Make synthetic native-evidence and class-execution selection; full output in closure-tests.log | Running |
| Exercise real class operation at 1,250 / 2,500 / 5,000 restrictions | Existing Make class-evidence benchmark; requires executed class outcome, actual engagement, retained ledger and every original descriptor proof; full output in closure-benchmark.log | Running |
| Preserve normal build and semantic contracts | Current lock and existing normal test/bench profiles; no flags, dependency, source, fixture, admission or verdict changes | Verified |
| Merge the repair through the mandated path | Existing merged PR 1802 and ghprsq audit refs, not a new duplicate PR | Verified |

The historical same-lock benchmark series in the issue remains historical evidence. Current measurements will be reported as current absolute measurements, not a transplanted speedup ratio. Full corpus/class capability and production-pipeline acceptance are separate unmet contracts on their existing owning issues; this report claims no full make check or hosted production pass.

Automation note: stagectl supplies the issue/PR evidence and comment publication. It has no issue-close operation; if this contract qualifies, the ordinary issue-close command will record completion of the already merged repair. It will not be used for any branch or PR merge.

## Current benchmark result

make bench-class-evidence exited zero. All three ten-sample synthetic actual-operation cases completed using the existing normal bench profile and current dependency lock. The harness requires executed and engaged class analysis, a retained ledger and every original descriptor proof.

| Descriptors | Current point estimate | Current confidence interval |
| --- | --- | --- |
| 1,250 | 697.26 ms | 520.31 to 914.08 ms |
| 2,500 | 720.60 ms | 695.59 to 746.05 ms |
| 5,000 | 2.2536 s | 1.7750 to 2.7988 s |

The complete output is closure-benchmark.log. These current absolute results do not reproduce the old same-lock baseline and therefore are not presented as a new speedup ratio. No benchmark configuration was adjusted in response to Criterion's sampling estimates. Focused correctness controls remain running; this is not yet a closure verdict.

## Final acceptance

Both Make commands terminated successfully with exit zero. The synthetic native/class selection passed all 21 tests in 0.215s, with both corpus-purity checks passing. No runtime source was edited during compilation or measurement. The original performance and exact-evidence contract is met; closure-report.md is the final public report. Earlier Running cells above are superseded by these terminal results. The sibling rule selection also passed all 15 controls in 0.144s on the same main runtime source.
