# Current acceptance

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
