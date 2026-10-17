# Requirement candidates — issue #1803

Lines from the issue body and comments that matched a lexical rule, copied
verbatim with the rule that fired. This is a CANDIDATE list: rules both
over-match (a "should" in passing) and under-match (a requirement stated
without a keyword). The issue-analyzer keeps or drops each row, adds what
the rules missed, and accounts for every silent comment. Code fences are
skipped.

| ID | Source | Line | Rules | Text |
|---|---|---|---|---|
| B1 | body | 17 | modal | - **CLAUDE.md** currently says "the native `logic:` reasoner is the single reasoning authority". It should read: the authority on `logic:` **semantics**, executed on purrdf engines. |
| B2 | body | 21 | list-under-requirement-heading | 1. **Proofs bound to source premises,** so gmeow ledgers can cite purrdf derivations. purrdf already has `DlProof`, `ChaseProof` and `ServiceProof`. |
| B3 | body | 22 | list-under-requirement-heading | 2. **World-scoped input:** named graphs as worlds. |
| B4 | body | 23 | list-under-requirement-heading | 3. **Budgets and stop signals** (`StopSignal` exists). |
| B5 | body | 24 | list-under-requirement-heading | 4. **Shared-store or delta seams,** so production does not copy a ~690k-fact store per call. These may need adding upstream. |
| B6 | body | 25 | list-under-requirement-heading | 5. **purrdf release cadence** that keeps up with boundary moves (gmeow pins `purrdf = "3"`). |
| C4.1 | comment 4 (paudley, 2026-10-16T02:00:42Z) | 5 | modal | The official crates.io Git index was refreshed through the GitHub API and still lists 3.0.1 as the latest stable non-yanked `purrdf`. The previously captured ancestry places the three closed upstream boundary fixes after that release. A version bump to 3.0.1 alone therefore does not establish those capabilities or the required production-world decisions. |
| C6.1 | comment 6 (paudley, 2026-10-16T23:52:09Z) | 7 | modal | Verification exercises the real NativeValues membership/equality path for malformed fractional/exponent/empty-magnitude decimal syntax, a valid scale-80 exact decimal, and the actual datatype-enumeration consumer: a malformed decimal cannot satisfy the rational half enumeration, while valid decimal half is included and quarter excluded. Run the surrounding synthetic native datatype/refutation/program controls and strict producer-independent Clippy, normal commit hooks, then publish the coherent fix in the existing proof branch and PR. Hosted corpus and full-canon qualification remain required; this does not close the whole engine-ownership issue. |

## Comments that matched no rule

- comment 1 (paudley, 2026-10-06T02:25:16Z)
- comment 2 (paudley, 2026-10-06T03:31:22Z)
- comment 3 (paudley, 2026-10-16T00:13:54Z)
- comment 5 (paudley, 2026-10-16T22:50:27Z)
- comment 7 (paudley, 2026-10-16T23:59:25Z)
