# Qualification index

The correction is committed and pushed normally. PR 1846 is open, and the tracked source tree is clean.

| Requirement | Evidence | Current result |
| --- | --- | --- |
| Real co-occurrence regression | `reproducer.log` | Reproduced: authored source reaches violation lowering and reports UnsupportedAntecedent |
| Missing diagnostic registration | `reproducer.log` | Reproduced: the independent catalog check finds zero entries instead of one |
| Scope-preserving implementation | Current source diff | Positive atoms, conjunctions and existentials share lexical renaming with positive-head lowering; whole-law names are reserved; unsupported guard operators remain residue |
| Diagnostic correction | Current source diff | Both kinds are included in the catalog and eager registration in matching order |
| Strict affected all-target Clippy | `clippy.log` | Passed with warnings treated as errors |
| Expanded compiler and actual gate checks | `focused-tests.log` | Passed: all 84 selected tests, including co-occurrence execution, lexical scope, existing head lowering and both diagnostic registration checks |
| Final tiny witness refresh | `witness-refresh.log`, `witness-review.md` | Passed; seven reason records and 127 conjecture records retain their semantic content |
| Read-only witness consumers | `witness-consumers.log` | Both passed against exact committed witness bytes |
| Normal hooks and publication | `commit.log`, `push.log`, PR 1846 | Passed; no hook bypass or disabled verification |
| Complete review and ghprsq integration | Pending | Required before completion |

Both initial regressions failed against unchanged production code. The existing assertions were not weakened. The expanded suite retains the earlier native kind, equality, typed refusal and chase controls, and includes the existing positive-head scope tests because their variable walker was shared rather than copied.

No canonical ontology, structural query, native dependency pin, runtime dependency, machine probe, resource cap, full local corpus producer or full gate was changed or invoked. The scanner processor-count suggestion was explicitly declined and its thread resolved under the user's instruction. The two valid source findings remain open until the correction lands. Production generation and fixture-prefix acceptance stay attached to the original integrity issue.

Final qualification: the child commit adds 16 documentation lines to three test files. Production source, test execution and witness bytes are unchanged, so the 84-test and strict Clippy evidence remains applicable. Both exact witness consumers passed again on this final tree; all normal commit hooks passed, and both commits are pushed normally. A fresh fetch found no additional base changes; the final integration candidate is clean. CodeRabbit completed the production review with no actionable code findings and minimal assessed merge risk. The linked-issue checklist had stale witness status; completed results and the documentation update were posted for refresh. Hosted CI is pending. No full production acceptance or independent approval is claimed. The original integrity issue and the full logic goal remain open.

Final external checklist disposition: CodeRabbit inspected the final documentation commit and updated PR description. Its reply reports no remaining concern in either the linked-issue or docstring check. It explicitly relied on the reported local results and did not rerun tests. Its reply confirms all 16 added lines are test documentation only. No updated coverage percentage or independent execution is claimed. The full production review already found no actionable code findings. Latest hosted producer-build, helpers, lint and rust-prebuild are still running.
