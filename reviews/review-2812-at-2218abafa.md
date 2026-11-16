PASS at 2218abafa

# Delivery review: issue 2812 at 2218abafa

Reviewed 2026-11-15, after 06:08 -06:00 (the time of the coder's last log). Worktree
`/home/paudley/Active/gmeow-ontology/.worktrees/2812-native-engine-refuses-a-rule-head`, clean, HEAD
2218abafa, pushed according to `r3-push.log`. The verdict covers the whole change `417fd45d7..2218abafa`.

The coder left no report for this head, so everything below comes from the delta `90b07733d..2218abafa`
(3 files: README.md, `crates/gmeow-cli/src/commands/prove.rs`, `crates/gmeow-cli/tests/prove_cli.rs`),
from the code it calls, and from the `r3-*` logs. Earlier rounds: `review-2812-at-7670e16e1.md` and
`review-2812-at-90b07733d.md`.

## R3: a refused head in an authored record is now a named drop, `not-decided`, exit 3 (fixed)

- **Where it is decided.** Whether a head is refused is decided by one function,
  `reified_nary_head_groups`, through `validate_head_shape` or `PreparedChaseRule::new`. Each input
  channel turns that refusal into a drop once:
  - A formula is refused at lowering. The rule becomes residue (relational_core.rs:86-92), which prove
    reports as `UNSUPPORTED_CONSTRUCT` (prove.rs, the lowering-residue loop).
  - An `lx:ExistentialRule` record is refused when the source rule is prepared
    (`PreparedChaseRule::from_source`, joint/input.rs:541). prove now downcasts that `RefusedRuleHead`
    in `native_leg` (prove.rs:1011-1022). It adds a `DropRecord(ENGINE_NATIVE, "UNSUPPORTED_CONSTRUCT",
    refused.to_string())` and returns `NativeLeg::refused`. The `to_string()` text is
    `rule <…> with head relations [<…>]: <detail>`, so the drop names the rule and its head relations.
  - No other site classifies the refusal. The old evidence projection on `NATIVE_EXECUTION_FAILED` has
    been removed. It is now unreachable for this error, and this also closes the earlier O5.
- **A skipped rule cannot leave the verdict `consistent`.** `complete = admission complete &&
  drops.is_empty()` (prove.rs, after the legs). The `UNSUPPORTED_CONSTRUCT` drop makes it false.
  `native.clash()` is false for a `refused` leg. So the verdict is `inconsistent` only if the OWL
  tableau refutes, and otherwise `not-decided`. It can never be `consistent`.
- **The case the coordinator asked for.** Input: a record whose head is `logic:naryArg0(?x, ?x)` beside
  the single fact `ex:a ex:p ex:a`, with no other drops. The OWL view of that fact cannot clash. The
  native leg is refused, `drops = {UNSUPPORTED_CONSTRUCT}` and `complete = false`. The verdict is
  `not-decided`, exit 3 under `--gate`. This comes from reading the code; I did not run it.
- **The retargeted CLI test.** `an_authored_rule_record_the_engine_cannot_carry_is_a_named_drop`
  (prove_cli.rs:917-948) asserts all of the following:
  - verdict `not-decided`, exit 3;
  - native verdict `refused`;
  - `selection.complete` false;
  - the earlier `SCOPED_CLAIM_NOT_READ` drop is kept;
  - an `UNSUPPORTED_CONSTRUCT` drop with the exact text
    `rule <https://example.org/prove/r> with head relations [<https://blackcatinformatics.ca/logic/naryArg0>]`.

  It passed at 2218abafa (`r3-cli.log`). It would fail against 90b07733d, where this input gave
  `execution-failure` and exit 5.
- **The literal-subject input.** No test uses "head subject grounded to a literal" as an engine-failure
  input. A search of README.md, docs/, `crates/gmeow-cli`, `crates/cli-core` and the logic design set
  found no such test.

## R4: the README sentence is now true (fixed)

README.md:324-328 lists what a failure after both engines finished changes:
- verdict and decision set to `execution-failure` (`DecisionClass::ExecutionFailure.as_str()` is
  `"execution-failure"`);
- evaluation set to failed;
- completeness set to unknown;
- information state set to not evaluated;
- evidence grade and gate admission refused;
- the failure diagnostic appended.

The selection and engine blocks are kept. This matches `ProveReport::execution_failure`
(prove.rs:381-392) field for field, and the README claims no change to preservation, boundaries or
metrics, which are indeed left alone. No other text repeats "replaces only": I searched README.md,
docs/ and the CLI crates. `prose-check` on README.md:318-330 gives EXIT=0.

## What else is in the delta

Nothing beyond those two points:
- `NativeLeg::refused(claims_read)` factors out the `refused` leg that the existing `NativeRefusal`
  branch and the new branch share. Its behaviour is unchanged.
- The doc comment on `native_leg` now states the new branch.
- The `RefusedRuleHead` evidence projection was removed, as described above.

No logic-crate file changed.

## Acceptance coverage for the whole branch (417fd45d7..2218abafa)

| Box | Covered by |
|---|---|
| 1. A mixed head executes, with frontier, existential and constant subjects | chase.rs recognizer and firing; chase test `a_head_mixing_ordinary_atoms_and_a_reified_tuple_derives_both` (now with distinct argument values); nested tuples minted in dependency order |
| 2. A refusal names the rule identity and the head relation IRIs | `RefusedRuleHead` payload and Display text, on both the formula path and the record path; chase and lowering tests; both CLI drop tests |
| 3. A refused head is a named `UNSUPPORTED_CONSTRUCT` drop and `not-decided`, never an execution failure | formula path at lowering (`a_rule_head_the_engine_cannot_carry_is_a_named_drop`, `tuples_that_take_each_others_node_are_a_named_drop`); record path in prove (`an_authored_rule_record_the_engine_cannot_carry_is_a_named_drop`) |
| 4. After an engine failure, the report keeps the selection and earlier drops, marked incomplete | `ProveReport::engine_failure`, unit test `an_engine_failure_keeps_the_selection_and_the_drops_named_before_it` |
| 5. A CLI test runs the reproduction and gets a decided verdict | `a_consequent_mixing_a_unary_and_a_ternary_atom_is_executed`: `consistent`, exit 0, no drops |

All five boxes are covered.

Box 4 now has unit-level coverage only: the record input that drove a CLI failure test is no longer
an engine failure. As ruled in the previous round, that is acceptable. The only honest exit-5 class
reachable from input is a resource limit, and the issue does not require a CLI test for box 4.

## What the logs show (`/home/paudley/Active/gmeow-ontology/.stage/issue-2812/r3-*`)

Each log starts with the worktree path, the commit hash and the `git status --porcelain` block, and
ends with the tool's own `EXIT=`.

**Taken at 2218abafa on a clean tree** (written by `r3-batch.sh`, which finished with
`r3-batch.done` = BATCH-DONE):

| Log | Command | Result |
|---|---|---|
| `r3-nextest-logic.log` | `nextest-synthetic`, gmeow-logic, filter `/physical::chase\|nary::\|relational_core\|reason::tests\|provenance\|reason::claims\|reason::program::\|program_analysis\|lower::tests\|witness_tests\|native_semantics\|retention_tests/` | 434 run, 428 passed, 6 failed, EXIT=2 |
| `r3-nextest-logic-excluding-base-failures.log` | the same filter minus those six | 428 passed, EXIT=0 |
| `r3-nextest-compile.log` | gmeow-logic-compile filtered run | 379 passed, EXIT=0 |
| `r3-clippy.log` | `-p gmeow-logic-compile -p gmeow-logic --all-targets -D warnings` | EXIT=0 |
| `r3-clippy-cli.log` | gmeow-cli clippy | EXIT=101, the existing `large_unopt_stack_frames` error at `crates/gmeow-cli/src/lib.rs:125:17`, a file no commit on this branch touches |
| `r3-clippy-cli-diag.log` | gmeow-cli clippy, diag variant | EXIT=0 |
| `r3-hygiene.log` | `helpers-hygiene` | EXIT=0 |
| `r3-recipe.log` | `producer-recipe` | EXIT=0 |
| `r3-cli.log` | `cli-real.sh`: prove_cli and prove unit tests, against the existing bundle | 32 passed, EXIT=0; includes the retargeted test, both named-drop tests, the nested-tuple test, the reproduction test and the box-4 unit test |
| `r3-push.log` | `git push` | EXIT=0 |

The six failures in `r3-nextest-logic.log` are the same tests at the same panic sites as at
90b07733d:
- five `reason::tests` failures (mod.tests.rs:1792, 1900, 2043, 2317, 2437);
- the enactment fixture refusal (prepared_verify.rs:26).

All six are in `recovery/evidence/baseline-logic.fails`, and the five also fail at the base commit
417fd45d7. This delta touches no logic-crate file.

**Not taken at 2218abafa on a clean tree**, so they show nothing about this head:
- `r3-pre-fmt.log` (`make fmt`): at 90b07733d, dirty tree with the three delta files modified.
- `r3-it-cli.log` (`cli-real.sh`, 32 passed): at 90b07733d, dirty tree. An iteration run.
- `r3-commit.log`: at 90b07733d, three files staged. It is the commit itself, and its hooks exited 0.

**Not run at all:** `make check`, any producer, gate or reasoning pipeline run, a whole-workspace
test, nextest for the whole gmeow-cli package (only the prove selection ran), and any test of other
consumers of `RefusedRuleHead` outside prove.

## Findings

Required: none.

Optional:
- **O8: the retargeted test does not isolate the refused rule.** It also carries a scoped claim, so
  `complete = false` would hold without the refused rule. The test still protects the behaviour,
  because it asserts that the `UNSUPPORTED_CONSTRUCT` drop is present and `complete` is
  unconditionally `drops.is_empty()`. A version without the scoped claim would pin "the refused
  record rule alone prevents `consistent`" directly.
- **O9: records and formulas are declined at different granularity.** A refused formula drops only
  that rule, and the rest of the program still runs. A refused record declines the whole native
  program, so a native clash elsewhere in the same selection goes unreported and the verdict is
  `not-decided`. Both outcomes are sound and named. Dropping only the record rule would need a residue
  channel in source preparation in the logic crate.
- **O10: a refused head from an internal producer would be misclassified.** prove now treats any
  `RefusedRuleHead` as an unsupported construct. If an internal producer (DL bridges, `nary.rs`,
  domain rules) ever emitted a refused head, that engine bug would show as a drop instead of exit 5.
  Those producers build well-formed heads today, and the verdict would still be `not-decided`, never
  wrong.
- **Carried forward from earlier rounds; these existed before this change and are outside this
  issue:**
  - O1: a repeated n-ary atom in one head is refused.
  - O6: a malformed `lx:` record (`NativeCoverage`) still gives exit 5, where malformed input is
    arguably exit 4.
  - A head subject grounded to a literal still gives exit 5. It needs its own tracked defect.
  - O7: the five `reason::tests` failures on main are genuine assertion failures, not missing
    fixtures, and need an owner.

## Hygiene

- The only IRIs in the delta are `https://example.org/prove/r` and the `logic:naryArg0` namespace IRI.
- No `lillith`, `principia` or `soul`, and no issue or PR numbers or history, in the delta or the
  2218abafa commit message.
- No Python.

## What I did not verify

- I ran no tests, builds or probes. The test results above are the coder's logs.
- I did not run the isolated record-beside-consistent-facts case; it comes from reading the code.
- I did not check other commands that use the engine (the pipeline reason stage, `gmeow` commands
  other than prove). They still treat a refused record head as a hard error, which is outside this
  issue's prove contract.

VERDICT: PASS
