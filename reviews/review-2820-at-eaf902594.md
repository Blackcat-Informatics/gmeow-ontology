PASS at eaf902594

# Delivery review: issue 2820, delta a32dae828..eaf902594

## Coverage of this verdict

The verdict covers the whole branch. It rests on:

- the PASS at f7b072bbb;
- the delta reviews at fe1dc4bf8 and a32dae828 (both BLOCKED). R1 and R2 of a32dae828 are the
  findings this delta answers;
- this review of the two new commits.

The merge b34163b70 brings no new content into the branch tree (item 1), so no code entered
outside the wording commit.

## Scope and state

- Worktree: /home/paudley/Active/gmeow-ontology/.worktrees/2820-cache-size-bound-makes-prove-refuse-a.
  - `git rev-parse HEAD` = eaf902594506159140da48636b398a695a312740, as the caller stated.
  - `git status --porcelain` is empty.
  - HEAD equals its remote branch.
- `origin/main` = bc4560473, an ancestor of HEAD. I did not fetch.
- Requirement: `stagectl issue 2820`, read verbatim. It has 5 acceptance boxes and 0 comments.
- Commands I ran: read-only git, `stagectl issue`, grep, sed and stat, and `prose-check --stdin`
  on the delta's added lines and on the commit message (both EXIT=0).
- Commands I did not run: make, cargo, tests or the producer.

## What I read

- `git show eaf902594`, in full. `git show --stat` for the merge, and its parents.
- The previous review: review-2820-at-a32dae828.md, in full.
- The coder's table: .stage/issue-2820/wording-truth-table.md, in full.
- crates/logic/src/physical/seminaive/joint/input/admission.rs:
  - 280-509: `certify`, the `refused` closure, both bail-outs, and `certify_settled`;
  - 785-940: both notes, `EvidenceGap`, `is_bound` and `Evidence`;
  - 1094-1102: `Settled`.
- admission_tests.rs:350-440.
- crates/logic/src/physical/effects/value_flow.rs:
  - 1655-1664: the `InterningCut` doc;
  - 3490-3547: `with_source_constants`.
- crates/logic/src/physical/seminaive/joint/input.rs:1280-1325, where an evidence gap goes to
  `bounds` or `violations`.
- crates/logic/src/physical/seminaive/joint.rs:1170-1190.
- crates/logic/src/physical/chase.rs:
  - 1355-1385: the `Uncertified` docs;
  - 1940-2030: the `Uncertified` constructions, including the context-note prepend at 1973-1987;
  - 2085-2110;
  - 2705-2776: `violations`, `bounds`, `capability_gap_rows` and the `to_finding` doc.
- crates/logic/src/reason/program.rs:720-770, where `NativeRefusal` is built.
- crates/logic/src/error.rs:104-122.
- crates/logic/src/reason/ledger.rs:140-175, plus that file's branch diff against origin/main.
- crates/logic/src/materialize.rs:685-705 and 1395-1412: the only callers of
  `capability_gap_rows`. Both are refusal paths.
- crates/gmeow-cli/src/commands/prove.rs:
  - 885-901;
  - 1050-1110: the `NativeRefusal` drops;
  - 1150-1200: the `CHASE_UNCERTIFIED` drops on a budgeted run.
- crates/gmeow-cli/tests/prove_cli.rs:955-1010 and 1060-1070.
- README.md:
  - 286-300 and 312-322;
  - `git diff origin/main...HEAD -- README.md`.
- Evidence: n-all.sh in full. For n1 to n4: the headers, the tails, the FAIL and panic lines, the
  summary and the relevant PASS lines. commit.log in full. The stat times of the logs and of the
  eight files.

## What I did not read

- The bodies of the RJA and MSA rungs and of `certify_ladder`. The delta does not touch them.
- The non-overlapping files main brought: their content, past their names.
- The rest of n1-nextest.log beyond the lines listed above.

## 1. The merge b34163b70

- **Clean merge.** `git merge-tree --write-tree a32dae828 origin/main` writes
  c7b0a26768e7fedab6a0d81c1a396621c9521a7c. That is the tree of b34163b70, so there are no hand
  edits.
- **The merge changes nothing.** That tree is also the tree of a32dae828:
  - `git diff --stat a32dae828 b34163b70` is empty;
  - main moved from e32f90c96 to bc4560473, the squash of the 2812 branch this one was stacked on;
  - `git diff e063cd8df bc4560473 -- crates/logic crates/gmeow-cli README.md` is empty, so the
    squash carries exactly the content the branch already held.
- **The branch's own change.** `git diff origin/main...HEAD` now shows 21 files, +1401/-242, and
  no 2812 content:
  - README.md;
  - crates/gmeow-cli: src/commands/prove.rs, tests/prove_cli.rs;
  - crates/logic-compile/src/relational_core.rs;
  - crates/logic: error.rs, native_semantics.rs, program_analysis.rs and relational_core.rs;
  - crates/logic/src/physical: chase.rs, chase.tests.rs, chase/execution_cbor.rs,
    effects/value_flow.rs, effects/value_flow/tests.rs;
  - crates/logic/src/physical/seminaive: joint.rs, joint/input.rs, joint/input/admission.rs,
    joint/input/admission_tests.rs;
  - crates/logic/src/reason: ledger.rs, mod.rs, program.rs, program/minimum_tests.rs.
- **Interaction with main.** Main's change touched 7 of the branch's files: prove.rs,
  prove_cli.rs, error.rs, chase.rs, chase.tests.rs, relational_core.rs and README.md. That content
  was already in the branch through the stack, and the fe1dc4bf8 and a32dae828 reviews covered it.
  The merge adds no new interaction.
  - The nearest point of contact: main's `RefusedRuleHead` drop sits directly after the
    `NativeRefusal` drops at prove.rs:1100-1110.
  - It returns separately and emits no `NATIVE_ANALYSIS_BOUND`.
- **The branch is no longer stacked.**

## 2. Does the wording commit change behavior?

No. It changes 8 files, +40/-28:

- doc comments in prove.rs, error.rs, chase.rs, ledger.rs and admission.rs;
- the README parenthetical;
- the prove_cli.rs doc comment and one assertion message;
- the `interning_note` text and its doc;
- two test pins of that text, and one test doc comment in admission_tests.rs.

The only executable change is inside `interning_note` (admission.rs:816-832). The function now
builds its progress clause with a branch on `cut.rows_interned == 0`. It takes the same inputs and
returns a `String`, and no caller reads the text. Which results carry the note is decided at
admission.rs:329-336, which is unchanged. No control flow, type or condition outside the note
text changed.

## 3. Truth table

### Cases and code facts

The cases:

1. A bound is present, the program is refused, and the program really cycles.
2. A bound is present, the program is refused, and the program terminates; it is refused only
   through coarsening.
3. A bound is present on a budgeted run, and nothing was refused.
4. No bound is present, and the program is refused.

What the code shows:

- A bound note reaches a result only through `refused` (admission.rs:339-346), through the
  evidence-gap prepend (input.rs:1303-1311), or through the ladder's `bounds`
  (chase.rs:2022-2025). Each inserts it into an `Uncertified`.
- Every `Uncertified` from `certify_statements` carries non-empty violations. The first is the
  context line "joint statement value-flow abstraction of N native producer(s) has no joint
  termination certificate" (chase.rs:1985-1987), then cycles and weak-acyclicity edges.
- When a bound is present, those violations come from a coarser analysis:
  - the unenriched second pass;
  - the first pass, on a bail-out;
  - the template, on an evidence-gap bound;
  - the lower rungs, on an MSA bound.
- With a budget, an `Uncertified` program runs budgeted and is not refused (chase.rs:756-757). Its
  bounds go out as `NATIVE_ANALYSIS_BOUND` beside `CHASE_UNCERTIFIED` (prove.rs:1177-1193).
  `NativeRefusal` and the ledger rows arise only on refusal (program.rs:748-765; materialize.rs:698,
  1407).

### The table

T means true. "no claim" means the sentence says nothing about that case.

| Sentence | 1 | 2 | 3 | 4 |
|---|---|---|---|---|
| prove.rs:895-896 "drop code naming an analysis bound that coarsened or stopped part of the termination analysis, so a finer analysis might have decided" | T | T | T | no claim (no drop) |
| prove.rs:896-897 "It does not say the bound caused a refusal." | T | T | T | no claim |
| prove.rs:897-898 "sits beside … `NATIVE_REFUSAL_REASON` or, on a budgeted run, beside `CHASE_UNCERTIFIED`, where nothing may have been refused" | T | T | T, but hedged: on a budgeted run nothing was refused (O1) | no claim |
| prove.rs:898-900 "With a bound present those reasons are the cycles the coarser analysis found, which a finer one might have ruled out, so neither the bound nor the reasons alone say whether the program cycles" | T (the modal is epistemic) | T | T | no claim |
| README.md:295 "(a finer analysis might have decided; with a bound present, the rules then named are the cycles the coarser analysis found, not proof that the program cycles)" | T | T | T. The cell attaches the code to a declined program and does not mention a budgeted run: an omission, not a falsehood | no literal claim. The contrast implies a no-bound refusal is proof; see O2 |
| error.rs:113-114 "`bounds` are the analysis bounds that coarsened or stopped part of the analysis, so a finer one might have decided" | T | T | no claim (no `NativeRefusal` on a budgeted run) | no claim |
| error.rs:115-117 "A bound is not itself a gap in the program: it says the analysis was coarser, so with a bound present the gaps are the cycles the coarser analysis found, not proof that the program is at fault." | T | T | no claim | no literal claim (same contrast as O2) |
| chase.rs:1378-1382 "It does not say whether the bound caused the refusal; with a bound present, `violations` are what the coarser analysis found and may be artifacts of it." | T | T | T ("the refusal" is loose on a budgeted run) | no claim |
| chase.rs:1372-1373, 1378 and 2733 "bounds that kept a finer rung / certificate from being decided" (unchanged) | T | T | T | no claim |
| ledger.rs:148-151 "per reason (a rule on a termination cycle, or an analysis bound that coarsened or stopped part of the analysis, so a finer one might have decided) of a refused … program" | T for bounds | T for bounds | no claim (no rows) | T for cycle rows; the list omits the context and evidence notes (O3) |
| ledger.rs:153-155 "an existential edge lies in a cycle of the analysis, which an analysis bound may have coarsened or stopped" | T | T | no claim | T (hedged by "may") |
| admission.rs:794-797 `interning_note` doc: "the second pass used the coarser flow without the supplied rows … it does not say the bound caused the refusal it is attached to" | T, but "without the supplied rows" is loose (O4) | T | "the refusal" is loose: the note sits on an uncertified admission that ran budgeted | no claim |
| admission.rs:799-801 "When it is zero the note says no row was begun; otherwise … the last begun row may be only in part, since the cut can fall inside a row or on its boundary" | T | T | T | no claim |
| prove_cli.rs:974-977 "This small program stays under every analysis bound, so none is expected; a cycle in a program that crossed a bound would also carry the bound drop beside the reasons the coarser analysis found." | T, by reading: a genuine cycle refuses the first pass, so the notes are attached | — | — | T, by reading. Not runtime-shown on this host |
| prove_cli.rs:1004 "no analysis bound is expected for this small program" | — | — | — | T (closes the earlier O3) |
| README.md:319 "an analysis bound of the termination proof is a `not-decided` drop, never this exit" (unchanged) | T | T | T; a clash still makes `inconsistent` | T (vacuous) |

### Comparison with the coder's table

The coder's table agrees with mine on every true or false judgment. The differences:

- The coder's "true (no claim)" for README and error.rs in case 4 is literally correct. However,
  "with a bound present … not proof" invites the reading that a no-bound refusal *is* proof
  (O2).
- The coder marks the `interning_note` doc and chase.rs:1380 "true" in case 3. "The refusal it is
  attached to" presupposes a refusal that a budgeted run does not have. That is loose, not false.
- The coder's table does not note that each "those reasons are the cycles" also covers the
  leading context line, which is not a cycle. I judge this harmless.

### Under-claim check

No sentence under-claims to the point of being useless.

- The README tells a reader of a bounded refusal two true and useful things: a finer analysis
  might decide, and the named rules are the coarser analysis's findings.
- The code supports no stronger conclusion for case 2. The coarse pass's cycles are not shown
  anywhere to be a superset of a finer pass's cycles, so "a finer pass could only remove rules"
  is not a claim I could make from the code.
- The one place the text says less than the code allows is prove.rs:898, "where nothing may have
  been refused". On a budgeted run nothing is refused (O1). This is a source comment, not
  user-facing.

## 4. The interning note

Code: value_flow.rs:3499-3522 and admission.rs:816-832.

- **Zero rows.** `rows_interned == 0` only on the early return at 3501-3506, where
  `cells == flow_cells` and no supplied row was touched. "The interner stopped before beginning any
  of the M rows supplied" is exact. "Of which N were already held by the flow", with N = cells, is
  exact. When M = 0, it reads "any of the 0 rows supplied": vacuously true but awkward (O5).
- **Row boundary.** The cut can fire on the object term of row `read`. The row is then fully
  observed and `rows_interned = read + 1`. "k … begun, the last possibly only in part" is exact,
  because "possibly" covers a complete last row.
- **Mid-row.** The cut can fire on the subject or predicate term. Row `read` is then partial and
  rows 0 to read-1 are complete. The note is exact.
- **The count M.** M = `row_count + possible` = `source_rows.len() + possible.len()`. That is the
  length of the chained iterator (admission.rs:311-317), so M is exact.
- **Test coverage.** The k > 0 text is no longer pinned by any test: both pins now take the zero
  branch, and `interning_cut_measures_the_cells_and_the_rows_interned` checks only the numbers
  (O6).

## 5. Other descriptions in the repository

What I searched, with `git grep -n -I -i` over the tree (wasm and generated/ excluded):

- `ANALYSIS_BOUND|native-analysis-bound|analysis bound|analysis-bound`;
- `interner stopped|rows supplied|enrichment was abandoned|coarser analysis|program's own|program itself|caused (the|a) refusal|not proof that|artifacts of it|the bound is the cause|bound stopped the proof`,
  in crates/logic, crates/gmeow-cli, crates/logic-compile, README.md and docs/.

What I found:

- Every hit on the drop code or the notes is one of the sentences in the table above, or one of
  these:
  - admission.rs:309, 559, 842, 878, 887, 922, 938, 996, 1097 and 1241;
  - chase.rs:1559, 2000, 2463 and 2773;
  - prove_cli.rs:1067;
  - the test names in input/tests.rs and minimum_tests.rs.
- Each describes a specific bound or note accurately as written.
- The other hits are unrelated wording in cost.rs, explain.rs, goal_directed.rs, identity.rs and
  docs/PROJECTION-DEBT-TRIAGE.md.
- No snapshot, docs/ page or help string names the drop code or the note text.
- Nothing still expects the old "with 0 of the M rows supplied begun" text.

## 6. Evidence

All logs are in /home/paudley/Active/gmeow-ontology/.stage/issue-2820/.

n-all.sh writes the following to each log, with no pipe:

- `pwd`;
- `git rev-parse HEAD`;
- a porcelain block;
- `CMD:`;
- the command, under `flock`;
- the tool's own `EXIT=$?`.

| Log | HEAD in log | Command | Result |
|---|---|---|---|
| n1-nextest.log | b34163b70 | `make nextest-synthetic NEXTEST_FILTER='package(gmeow-logic) & (test(/physical::seminaive/) \| test(/value_flow/) \| test(/chase/) \| test(/reason::program/))'` | 382 run, 379 passed, 3 failed; EXIT=2 |
| n2-clippy.log | b34163b70 | `cargo clippy --locked --manifest-path …/Cargo.toml -p gmeow-logic-compile -p gmeow-logic --all-targets -- -D warnings` | EXIT=0 |
| n3-recipe.log | b34163b70 | `make --no-print-directory producer-recipe` | faa350fc…; EXIT=0 |
| n4-lint.log | b34163b70 | `make lint` | every hook passed, including `cargo fmt --check` and the issue/PR reference lint; EXIT=0 |
| commit.log | — | the commit hooks' output | every applicable hook passed |

**The three n1 failures are the expected ones:**

1. input/tests.rs:680, `Arc::ptr_eq(&left.effects, &right.effects)`.
2. dl.tests.rs:1444, `closure.iter().any(…)`.
3. operator_rules/test_support.rs:16, "operator controls require the exact producer selector:
   NotPresent". This is the missing-fixture refusal.

The touched tests pass:

- `the_interning_note_states_cells_and_inputs_not_a_row_count_as_cells`;
- `a_refusal_counts_the_selector_rows_supplied_to_the_interner`;
- `interning_cut_measures_the_cells_and_the_rows_interned`;
- the three analysis-bound and cacheable tests in minimum_tests.

**Wrapper noise.** No log contains a line that begins `/home/paudley/stage/scripts/cargo: line`
(0 matches in each of the five).

**Command and exit lines.**

- n1 to n4 record both.
- commit.log records neither its command nor an exit line; it is the hooks' output only.
- The commit exists, so the hooks passed. The hooks' own status is therefore shown by the commit
  object, not by the log.

**Do the logs stand for eaf902594?**

- Each porcelain block lists exactly the 8 files that `git show --stat eaf902594` lists, all
  ` M`.
- The eight files' mtimes are 19:47:04-19:49:17. n-all.sh is from 19:51:30, the logs end
  20:00:15-20:09:57, and the commit is from 20:11:10 (all -0600).
- No file was rewritten after the evidence run began. So the tested content is, by inference from
  stat times, the committed content of eaf902594.
- eaf902594's parent is the logs' HEAD, and the commit hooks ran over the same content.
- This is sufficient for the logic crates, lint and recipe at eaf902594. It does not cover
  gmeow-cli (see "Not shown").

## Findings

### REQUIRED

None.

R1 and R2 from a32dae828 are resolved:

- **R1.** The four sentences (prove.rs:898-900, README.md:295, error.rs:115-117,
  chase.rs:1380-1382) no longer say that the reasons show the program is at fault. Each now says
  that, with a bound present, the reasons are the coarser analysis's findings. prove.rs also
  names the `CHASE_UNCERTIFIED` placement.
- **R2.** ledger.rs:148-155 no longer says every bound stopped the analysis.

### OPTIONAL

- **O1. prove.rs:898 "where nothing may have been refused".** With a budget, an uncertified
  program always runs budgeted (chase.rs:756-757), so nothing was refused. Write "where nothing was
  refused".
- **O2. The contrast in README.md:295 and error.rs:115-117 invites a false inference about
  case 4.** "With a bound present … not proof that the program cycles" suggests that a no-bound
  refusal is such proof. The code does not support that:
  - the certificate ladder is sufficient-only;
  - the analysis is a value-flow abstraction even with no bound (chase.rs:1973-1987);
  - with `EvidenceGap::Unobserved` the template decides with no bound present (input.rs:1307-1311).

  I do not block on this. The literal text is scoped and true, and the issue's own acceptance
  language calls a no-bound refusal's rules "the offending cycle". A wording that is true without
  the contrast: "the rules named are the cycles the analysis found; with a bound present that
  analysis was coarser, so a finer one might find none".
- **O3. ledger.rs:148-151 and 158-160 list the reason kinds as rule-on-cycle or bound.**
  - Every joint refusal's first violation is the context line "… has no joint termination
    certificate" (chase.rs:1985-1987).
  - An `Unobserved` gap prepends an evidence note (input.rs:1310).
  - Both become ledger rows.

  Add "or a context or evidence note", as chase.rs:1371-1372 already says.
- **O4. admission.rs:795 "used the coarser flow without the supplied rows".** The second pass still
  reads those rows for its bindings (347-351); what it lacks is their interned constants. This is
  carried from the last review.
- **O5. The zero-row note when M = 0** reads "any of the 0 rows supplied". It is true but awkward.
  Say "no rows were supplied" when `supplied == 0`, or leave it.
- **O6. The k > 0 progress text has no test pin.** Both pins now take the zero branch. If the note
  text is to stay pinned, one assertion with `rows_interned: 4` would cover the other branch.
  Otherwise drop the mirror pins (the earlier O5).
- **O7. "The refusal it is attached to" and "the refusal"** (admission.rs:797, chase.rs:1380)
  presuppose a refusal. On a budgeted run the same note sits on an uncertified admission that ran.
  Write "the result it is attached to".

## Not shown

- **gmeow-cli at this head.** No compile, no clippy and no CLI tests ran, because this host cannot
  build the crate (the known missing-fixture constraint). By reading, these could be wrong there:
  1. prove.rs:895-900 changed only doc text. A doc comment could still break the build only
     through a lint such as a broken intra-doc link. It names no `[...]` links, so the risk is
     low.
  2. `a_real_existential_cycle_is_a_named_refusal_with_its_authored_formula` would fail if the
     self-feeding program, with the current bundle content, crosses any analysis bound: the
     interning cut (flow plus supplied rows past 512 cells), the enrichment-rows bound, or a
     closure or MSA bound. Its new doc then would be false as well as its assertion.
  3. `a_program_padded_past_the_cacheable_bound_is_decided_rather_than_refused`
     (prove_cli.rs:1060-1070) would fail if the padding or bundle growth adds any bound drop or
     leaves the program uncertified.
- **A runtime witness of case 2 through the interning cut.** That is, a terminating program
  refused only because its coarse second pass found cycles. The wording's truth in case 2 rests
  on the code and on admission.rs:319-322, not on a run.
- **Byte-equality of the tested working tree and eaf902594.** It is inferred from file mtimes
  preceding the evidence run, not from a recorded diff hash.
- **Open state.**
  - Pushed: origin's branch is at eaf902594. Not merged.
  - `make check` has not been run on this head.
  - The branch is no longer stacked.
