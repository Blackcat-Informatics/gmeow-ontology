PASS at e8f625ffa

# Delivery review: issue 2726, PR 2821, head e8f625ffa

- Head: e8f625ffab2abccc1628f4b1585b7d9609dc09c5. Confirmed in the worktree: the working tree is clean and HEAD matches origin/paudley/2726-evaluate-a-reified-claim.
- Delta reviewed: `git diff 9f445d310..e8f625ffa`. The previous round reviewed the whole branch against origin/main at 9f445d310 (`review-2726-at-9f445d310.md`). This verdict covers the whole branch diff with this delta applied.
- Method: read-only. A host quiet window is in force, so I ran no build, test, lint or locked command; only git, grep and file reads. I read the coder's logs. I did not reproduce them.

## Scope of the delta

The delta touches 3 files (+18/-8) and nothing else:
- `crates/slicetest/src/dsl.tests.rs` (+14/-4);
- `slices/core/epistemics/i18n/fr.po` (2 msgstr lines);
- `slices/core/epistemics/i18n/zh.po` (2 msgstr lines).

No msgid, msgctxt, source TTL, ledger or golden changed.

## Findings

### Required

None.

### Optional

None from this delta. One phrase in the new French `putForwardByClaimFunctionality|avoidWhen` msgstr, "À éviter lorsque une seconde valeur…", misses the elision to "lorsqu'une". The branch did not introduce it: the identical sentence already appears 17 times in origin/main's fr.po for the sibling functionality records. Changing it for this one entry alone would make it differ from the other 17, which the glossary-consistency lint treats as an error. So no change is warranted in this branch.

## R1 (previous round): resolved

- `adequacyUnderStandardFunctionality|useWhen`: the msgctxt/msgid/msgstr block in both fr.po and zh.po is now byte-identical to origin/main (checked by `diff` against `git show origin/main:…`). The msgstr is back to "nomme exactement une valeur" / "恰好命名一个值 时使用", which matches its unchanged English "names exactly one value".
- `putForwardByClaimFunctionality|useWhen`: the msgstr is now "nomme au plus une valeur" (fr.po:530) and "至多命名一个值时使用" (zh.po:530). Both match the msgid "names at most one value", which matches module.ttl byte for byte (checked last round with `grep -F`; neither the msgid nor module.ttl changed in this delta).
- Effect on the reviewed-coverage golden: none. Both msgstr values are still non-empty, not fuzzy, French or Han respectively, and contain no English prose, so the integrity guard keeps them. The key set is unchanged.

## R2 (previous round): resolved

`crates/slicetest/src/dsl.tests.rs`:
- `parses_competency_exemplars` now asserts `spec.competency.len() == 3`. That matches the three `gmeow:CompetencyQuestion` cells in competency.ttl (lines 20, 85 and 389).
- `parses_conformance_exemplars` now asserts `len() == 4`. That matches the four `gmeow:ExampleConformance` cells in example-conformance.ttl (lines 26, 32, 46 and 52).
- The conforming cell is selected by IRI (`ecJustificationConforms`) and the violating cell by IRI (`ecDoxasticStateMissingAgent`). Each selection is followed by `assert_eq!(…outcome, Outcome::…)`. The file, violation-code and none checks that follow are unchanged. Selecting by IRI removes the dependence on SPARQL solution order. `Outcome` derives `Debug` and `PartialEq` (`dsl.rs:198`), so `assert_eq!` compiles on it.
- rustfmt: the only formatting changes split two `assert_eq!` calls across lines. The arguments and messages are unchanged, so behaviour is unchanged.
- Logs:
  - `.stage/issue-2726/r4-nextest.log`: `head=e8f625ffab2a…`, empty `git status --porcelain`, `make nextest-synthetic NEXTEST_FILTER=package(gmeow-slicetest) & test(/parses_/)`. Result: "4 tests run: 4 passed, 51 skipped", covering `parses_competency_exemplars`, `parses_conformance_exemplars`, `parses_structural_exemplars` and `parses_cq_result_shape_into_the_canonical_type`. `EXIT=0`.
  - `.stage/issue-2726/r4-clippy.log`: same head, empty status, `cargo clippy -p gmeow-slicetest --all-targets -- -D warnings`. Result: Finished, `EXIT=0`.
  - Tested scope: the four `parses_` tests in gmeow-slicetest, and clippy on that one crate. Neither run covers anything else.

## The 17 catalog entries the branch adds, msgid against msgstr

The same 17 contexts were added to both catalogs (`comm` of origin/main and head msgctxt sets). I read each msgstr against its msgid.

| Context | fr | zh |
|---|---|---|
| putForwardByClaim label | faithful ("avancé par la revendication") | faithful ("由主张提出") |
| putForwardByClaim definition | faithful: covers every clause (reifier stays a reifier; functional precondition; shared and content-addressed reifiers never linked; separate reifier per act; Principle 9; not inverse functional) | faithful: same coverage |
| putForwardByClaim example | identical code, correct | identical code, correct |
| putForwardByClaim scopeNote | faithful | faithful |
| putForwardByClaim useWhen | faithful | faithful |
| putForwardByClaim avoidWhen | faithful (all three avoid clauses) | faithful |
| putForwardByClaim howToUse | faithful (named reifier, one token, agent and frame, traversal back) | faithful |
| putForwardByClaimFunctionality label | faithful | faithful |
| putForwardByClaimFunctionality definition | faithful ("au plus un", inverse unconstrained, whole-act judgment) | faithful ("至多", 反方向不加约束) |
| putForwardByClaimFunctionality example | identical code, correct | identical code, correct |
| putForwardByClaimFunctionality scopeNote | faithful | faithful |
| putForwardByClaimFunctionality useWhen | faithful ("au plus une"): fixed by R1 | faithful ("至多"): fixed by R1 |
| putForwardByClaimFunctionality avoidWhen | faithful (minor elision; see Optional) | faithful |
| putForwardByClaimFunctionality howToUse | faithful | faithful |
| ClaimEvaluation howToUse | faithful, including the statedRationale and putForwardByClaim sentences | faithful |
| ClaimEvaluation avoidWhen | faithful, including "never suppresses, ranks or replaces… no ground for displayable false" | faithful |
| evaluates avoidWhen | faithful, including the reifier sentence | faithful |

No translation says something its source does not, and none drops a clause.

## Ledger table (re-checked against this delta)

The delta changes two msgstr values and one test file. Only the two slicetest rows change status: they are now satisfied.

| Pin | Satisfied? |
|---|---|
| functional-carrier ledger | yes (unchanged) |
| reviewed-coverage golden, epistemics keys | yes (key set unchanged; stale on main for other slices, not this branch's) |
| slicetest epistemics competency count | **yes** (3) |
| slicetest epistemics conformance count | **yes** (4) |
| slicetest structural lower bound (>= 13) | yes |
| epistemics class-universe membership | yes |
| `conformance_epistemics` rosters and Principle-10 sweep | yes |
| projection-vocabulary ceilings | yes |
| axis floors (Gmn1, DocMaturity, Translation, others) | yes (Gmn1 inferred from code; Translation unaffected: same reviewed entry count) |
| term-content-authority.json | not a registry |
| docs count snapshots (model, landing, llms header, competency index, coverage ratchet) | no, but stale on main already; not caused by this branch (previous round's O1) |
| richest-surface and term-card goldens | yes (inferred) |
| Python models package | yes (inferred) |
| SSSOM lint golden | yes |
| oracle gold, DL-EL crosscheck ledger, foundation goldens, conformance cases, GMN pack | not affected |

## Not verified

- Nothing was executed in this round. The slicetest pass and the clippy pass come from the coder's logs, which I read but did not reproduce.
- The pins marked inferred in the table are as they were last round. Neither the producer, nor the i18n lint, nor the reviewed-coverage test has run on this head.
- Whether the five docs count snapshots fail on main is inferred from slice counts.
- `make check` has not run on this head.

VERDICT: PASS
