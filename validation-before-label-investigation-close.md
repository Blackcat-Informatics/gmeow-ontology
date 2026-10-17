# Current acceptance

The first Criterion build (session 93082) terminated with a harness type error:
its proof subject is TermValue, not RdfTerm. No timing was produced. The assertion
now checks the exact IRI through as_iri without allocating another term; all
retained-membership and complete-family assertions remain mandatory. The original
failure and harness are preserved in class-labels-baseline.log and
class-labels-harness-initial.patch. The corrected harness is class-labels-harness.patch.
Repaired baseline session 45275 is live in class-labels-baseline-repaired.log;
production class analysis still has no optimization edit. This supersedes earlier
statements that 93082 was live, and it is not benchmark acceptance yet.

Pre-optimization native execution selection completed: all eight cases passed in
0.924s, including the new wide-label direct/nominal conflict control. It validates
minimal source leaves, local conclusions, disconnected subjects and retention of
nominal model obstructions. Log: class-labels-baseline-tests.log. This replaces
the earlier pending test session 51363; production class code is still unchanged.
Criterion baseline session 93082 remains separate and has not yet yielded timings.

Current-head CodeRabbit review explicitly covers 4b733c636 and reports no new
actionable risk. Documentation coverage passed at 82.50% (40 functions across
12 files). Raw evidence: pr1832-bot-review-current.json, updated 21:22:38Z.
Hosted lint passed; producer-build and rust-prebuild remain live on that head.
This supersedes pending documentation coverage below, not hosted qualification.

## Current review repair and active performance baseline

Documentation-only commit 4b733c636 is pushed, with every normal commit hook passed.
CodeRabbit's actual pre-merge documentation coverage error (42.42%) is addressed
with meaningful ownership/fixture contracts and obsolete snapshot comment removal;
current-head coverage is not yet verified. See admission-review-docs-report.md.
Prior functional tests retain their original scope because no runtime body changed.

The new class_memberships benchmark and wide-label native regression are local,
uncommitted work under class-labels-plan.md. Baseline compilation/execution is live
in session 93082, class-labels-baseline.log. The benchmark uses the existing Make
lane and bench profile with the unchanged dependency lock; no production class
lookup optimization has been applied before baseline evidence. Neither a current
baseline measurement nor a native/full-pipeline speedup is claimed yet.

The new native wide-label control and existing execution module are also running
against unchanged production code in session 51363, class-labels-baseline-tests.log.
This is a live validation attempt, not passing evidence. The benchmark session
93082 was re-polled and remains live. Both unfinished source files are preserved
as unstaged work, separate from the published documentation-only commit.

## Current admission-publication increment

The preceding source-law increments merged in PR 1831 as fb73513b8 under explicit
merge-now authorization. This worktree then fast-forwarded to that main and now
implements the next plan step: complete original-source admission observation in
the producer/bundle, with a transient provenance reverse index. This report is
not a prepared native base; original issue completion remains open. See
canon-admission-report.md for implementation, authority and evidence scope.

Baseline: all 68 existing TPTP controls passed (origin-index-baseline.log). Initial
implementation: 72/72 passed (canon-admission-tests.log). Final typed-diagnostic
and direct reader selection: 73/73 passed (canon-admission-repaired-tests.log).
The failed test-only import-path attempt remains in canon-admission-final-tests.log.
Strict producer-independent workspace/all-target Clippy passed with warnings
denied in canon-admission-clippy.log (3m05s), with the target's existing four
corpus-dependent consumer exclusions.
Normal signed commit hooks passed (canon-admission-commit.log); commit 41d204773
was pushed and PR 1832 is open. The current source worktree is clean. Hosted
production and current-head CodeRabbit review remain pending for this increment.
No whole-canon producer execution, full gate, installed native-base proof or
performance benchmark is claimed by these synthetic controls.

Post-merge PR 1831 generation jobs 117169103488/117169103559 reproduce the missing
recovery-leg error; fixture job 117169103468 reproduces stable/persistent admission
failure. Exact job logs are retained as postmerge-1831-*.log. The owning issues
1828 and 1829 received these results in comments 6223697525 and 6223698536; repair
PR 1830 remains live in CI run 38948027790. Issue 1768 has been reopened because its
full consumer-proof contract remains unmet despite the delivered distribution slice.

## Prior merged increment evidence

Authoritative plan: plan.md. Full issue remains incomplete.

| Criterion | Evidence / current state |
| --- | --- |
| Boolean/resource-enumeration source laws | Committed and pushed f9dbb7ae7. 62/62 selected TPTP/list/identity controls passed in fof-tests-repaired.log (60 compiler cases and 2 xtask controls). Includes shared owner provenance, nested restriction/nominal, blank identity, singleton arity, invalid empty/malformed lists and refusal controls. |
| Remaining selected class expressions | Not complete; datatypes and remaining constructors require certified semantics. Qualified object cardinality is implemented and pushed in 91298694b, with 74 focused compiler/list/restriction tests, strict producer-independent Clippy and normal hooks passed. |
| Partition and local-self constructors | Committed and pushed 0e5fb06bf. Corrected compiler/list/restriction selection passed 69/69, nextest 85045e6f-74cf-4848-bae9-69b4e3017cda. Strict producer-independent workspace/all-target Clippy passed in 51.19s. Normal hooks passed. class-constructors-report.md explains source wiring, full partition semantics, duplicate set members and the corrected RDF boolean test expectation. |
| Modal/context, Contract, distinct-body semantics | Contract declarations now have an explicit typed metatheory disposition in a9892dc72, with shared proof profile v2, selected-support checks and native cardinality guards. contract-metatheory-report.md records the exact 67-test, final 13-test and 9-test coverage, strict Clippy and normal hooks. This does not establish arbitrary contract execution, remaining context/modal or distinct-body semantics. |
| Selected probability model typing | Earlier merged synthetic controls, as recorded in issue comment. Not a new canon measurement. |
| Prepared canon in authenticated bundle | Not complete. Local producer fingerprinting conflicts with user host-probing ban; no execution or auth weakening authorized. |
| Installed blinded full-canon proof / negative grounding | Not run. Consumer currently compiles caller input. |
| Load/composition elapsed, memory, intermediate bytes and identities | Not established. |
| Full gate / hosted CI | No local full gate run for this issue. Hosted optimized producer, lint and rust-prebuild passed for ee21d2c32 in run 38938288117. Generation and fixture production then failed, with dependent validation skipped; issues 1828/1829 own the observed defects. Their repair is PR 1830, whose current source 1da90b0c1 has live run 38948027790: lint passed, producer and Rust prebuild in progress. These are pipeline-repair results, not full-canon acceptance for this branch. |
| Producer identity repair | PR 1827 merged through ghprsq as 0fd9462f0. Exact-source hosted optimized authentication passed before later pipeline failures. Its selected Stage evidence is archived at refs/ghprsq/pr/1827/stage; the temporary repair worktree was removed. Compiler-read and producer authentication were preserved. |
| Complete owner accounting | 1414e7498 retains explicit outside-selection rows and root/document evidence. Rejected owners and emitted IR with hard compiler errors report malformed selection. All 59 TPTP tests, strict producer-independent Clippy and normal hooks passed (owner-inventory-corrected-tests.log, owner-inventory-clippy.log, owner-inventory-commit.log). owner-inventory-update.md retains the precise scope and historical fixture failure. Unsupported execution semantics remain blocked. |
| Independent review | Unavailable: no delegation API. No independent verdict claimed. |
| Normal commit/push hooks | Class commit hooks passed, including source purity, formatting and issue-reference policy; class-commit-retry.log. Normal branch push succeeded. |

Initial make fmt refused the stale target symlink after worktree move. Relocated
the existing managed cache to the wrapper-required new worktree path, preserving
its contents; make fmt then completed successfully. No flags, profiles, caps or
host probes were added. Focused tests use the existing nextest-synthetic target,
which removes corpus credentials and does not produce fixtures.

The first commit attempt failed because stagectl open removed the prior local
/.stage/ exclusion. The issue-reference gate documents that untracked ignored
local tool records are outside authored product source and tracked files remain
scanned. Restored that local artifact classification in common Git info/exclude;
no hook, policy scanner, source exclusion, invocation or verification setting was
changed. All Stage evidence stays intact for selected ghprsq audit archival;
the failed attempt remains in class-commit.log. Normal retry passed every hook.

## Qualified object cardinality unit

All 74 selected compiler/list/restriction/FOF tests passed (qualified-cardinality-corrected-tests.log). Strict producer-independent workspace/all-target Clippy passed with warnings denied (qualified-cardinality-clippy.log); four corpus-dependent consumers remain outside that diagnostic target. Normal signed commit hooks passed (qualified-cardinality-commit.log). Qualified bounds preserve selected qualifier dependencies, conjunctive independent counts, native XSD integer values and denotational pairwise inequality. Zero-minimum laws retain dependency validation without an unused ground binding. Malformed/unsupported datatype bounds and unrepresentable counts fail explicitly. Existing unsupported-fixture controls now use datatype restrictions, rather than supported object bounds; their refusal and document-selection assertions remain intact. No full local gate, full canon certification or installed prepared-base acceptance is claimed.
