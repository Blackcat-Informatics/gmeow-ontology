PASS at 6026d22c3

# Delivery review: obligation declaration shapes, round 5

- Head: `6026d22c3066f268b17b2d0711f9acc27f5ed65b` (confirmed with `git rev-parse HEAD`; clean tree)
- Delta reviewed in full: `6c77441a7..6026d22c3` (one commit, eight files)
- Verdict scope: the whole branch diff against merge base `331c554f8` (seven commits, 33 files)
- Written: Sun 15 Nov 2026 02:16:11 AM CST
- Mode: read-only. The reviewer ran no build, test, make, cargo, producer, gate or reasoning
  command. Git was read-only (`diff`, `log`, `blame`, `merge-tree --write-tree`).

## Scope and reliance on earlier rounds

This round judges the delta adversarially and rechecks the round-4 findings R1, R2 and O1-O4. The
verdict covers the whole branch. For the parts this delta does not touch, it relies on the
earlier reports, whose REQUIRED findings were all resolved and are not re-derived here:

- Round 1 (`review-2745-obligations-at-fefb9bafd.md`) and round 2
  (`review-2745-obligations-at-116781ae8.md`): exclusion of each obligation declaration by IRI
  (`crates/logic-compile/src/projections/shapes.rs`, `ir/validation.rs`, `ir/validation/json.rs`
  and their tests, the snapshot `obligation-reifier-class-shape.snap`); the namespace guard on
  functional and inverse-functional class caps (`frontend.rs`, `foreign_domain_tests.rs`,
  `subsumption.rs`); the design text in LOGIC-VALIDATION.md, LOGIC-ENACTMENT.md and
  RECALL-CONTEXT.md; the pipeline validation-fixture test change.
- Round 4 (`review-2745-obligations-at-6c77441a7.md`): the case's declaration gaining an IRI and
  the re-blessed goldens, the excluded-list comment, the `conformance-bless` command's core
  behaviour (named cases only, every case run before any write, out-of-tree and oracle-backed cases
  refused), the `Cargo.lock` and `Cargo.toml` additions, crate layering, the external corpus
  descriptors and README, and the oracle-gold README.

## What was read

- The complete delta diff and its commit message (`prose-check --stdin`: exit 0; no issue or PR
  numbers added).
- `crates/gmeow-dev-cli/src/dev_conformance.rs` and `.tests.rs` at head; `lib.rs` (the variant and
  dispatch); `dev_producer.rs:60-140` (admission classification and `admit`);
  `dev_common.rs:106-121` (`project_root`).
- `crates/conformance/src/bless.rs` (doc, `Goldens`, every `init` use), `bless.tests.rs`,
  `error.rs` (whole), `profile.rs` (function list, `parse_shipped_rules`), `run.rs:236-262, 534-632`.
- `crates/pipeline/src/stages/conformance/execution.tests.rs:143-186` (corpus grading test);
  `crates/conformance/src/run.gating_tests.rs` (outline); `crates/conformance/tests/external_corpora_gates.rs`
  (outline).
- `Makefile:13, 1206-1210, 1310-1322`; `conformance/logic/runner/README.md`;
  `slices/grounding/lang/design/LANG-CONFORMANCE.md:187`.
- Repo-wide `git grep` for `GMEOW_CONFORMANCE_BLESS`, `write_expected`, `Goldens::` and `--init`.
- Evidence logs 80-86 under `.stage/issue-2745-obligations/`, and log 65 for comparison.
- `git merge-tree --write-tree origin/main HEAD` and the overlap of changed files with the local
  `origin/main` (`862638ad8`, not fetched by the reviewer).

## What was not read or verified

- No producer pass exists at this head. Nothing under `generated/`, the `gmeow.gts` bundle, the
  validation-fixture manifest or the recorded conformance stage artifacts was read.
- `make check`, `make clippy`, `make lint` and formatting were not run by anyone at this head per the
  logs; the commit hooks ran (no bypass observed) but their output was not read.
- The 35 commits on the local `origin/main` past the merge base were scanned only for overlap: one
  shared file (`crates/gmeow-dev-cli/src/lib.rs`, an unrelated five-line addition) and a clean
  textual merge. Their semantic effect on this branch was not reviewed, and the remote may have
  moved further.
- CI configuration was not read; the claim that the workspace test run in CI includes the corpus
  grading test rests on `make conformance`'s filter and the test's location.

## Evidence (implementer logs, all headed `6026d22c3` with a clean status)

- Log 80: focused nextest over logic-compile, the pipeline validation-fixture JSON tests,
  conformance (`bless|gating_tests|vendored`) and dev-cli (`dev_conformance|dev_producer|error::`):
  328 passed, 2601 skipped, `EXIT_STATUS=0`. The corpus-producer scanner and purity lint passed.
  Includes `dev_conformance::tests::named_cases_are_refreshed_and_a_refused_case_writes_nothing`.
- Log 81: clippy `-D warnings` on `gmeow-logic-compile` and `gmeow-conformance`, all targets:
  `EXIT_STATUS=0`.
- Log 82: clippy `-D warnings` on `gmeow-dev-cli`, all targets: `EXIT_STATUS=101` with one error,
  `large_unopt_stack_frames` at `crates/gmeow-dev-cli/src/lib.rs:150`, the
  `#[derive(Debug, Subcommand)]` on `Commands`. `git blame` attributes that line to `731e6f22f`
  (July), and the local `origin/main` carries the same line at 149. Log 65 reproduced the same error
  at line 149 with the branch's dev-cli changes reverted. The error is not introduced by this
  branch. Clippy reports every lint of the crate in one pass, so no other dev-cli lint is hidden.
- Log 83: `make producer-recipe` prints a digest, `EXIT_STATUS=0` (the recipe moves with the
  producer's source inventory, as established in round 2).
- Log 84: `make helpers-hygiene`, `EXIT_STATUS=0`.
- Log 85: `make maint-conformance-bless CASES=validation/reifier-shape` runs
  `cargo xtask producer run -- conformance-bless validation/reifier-shape`, finishes the `pipeline`
  profile, stages `dist/bin/gmeow-dev`, and reports `validation/reifier-shape: 0 expected file(s)
  differ`, `EXIT_STATUS=0`. The committed goldens are therefore a fixed point of the authenticated
  producer binary, not only of a dev build.
- Log 86: push `6c77441a7..6026d22c3`, `EXIT_STATUS=0`.

## Round-4 findings

- **R1 (REQUIRED): resolved.** `Makefile:1320` is `$(GMEOW_DEV) conformance-bless $(CASES)`. Log 85
  confirms the pipeline-profile producer runs it. No `gmeow-dev` Make target still compiles the CLI
  directly (`grep` for `cargo run.*gmeow-dev` and `-p gmeow-dev-cli` in the Makefile: none).
- **R2 (REQUIRED): resolved.** `crates/conformance/src/bless.rs:7-10` and
  `conformance/logic/runner/README.md:81-83` now say no test or gate blesses a committed case and the
  writer's tests bless synthetic cases in temporary directories. Both are true.
- **O1: addressed** with existing conformance kinds; see point 2 below.
- **O2: addressed.** See point 4.
- **O3: addressed.** The three `bless_cases` calls in `dev_conformance.tests.rs` carry the
  `gmeow-test-input: synthetic-only` marker, and the environment switch the test depended on is gone.
- **O4: addressed.** See point 5.

## The coordinator's points

1. **Make target statements.** The help text, the README sentence "`make maint-conformance-bless
   CASES=…` runs the same command", and the commit's "same authenticated producer runner as every
   other maintenance command" are true. `dev_producer.rs` keeps `ConformanceBless` in the group that
   needs no producer admission; the command still reads no producer output, so the README's "needs no
   producer output" holds even though the runner now builds the producer binary first. `project_root`
   resolves from the working directory, which `make` sets to the checkout, and log 85 found the case.
2. **Diagnostic kinds.** `Io` (`conformance.io`) for `read_dir` and `read` failures in
   `golden_bytes` is exactly its documented use. `CaseAnatomy` for a name that does not resolve to a
   case and `Cli` for oracle-backed and shipped-rules refusals are defensible but loose; neither is
   a wrong label on a consumer-visible path, because nothing keys on these codes for this command.
   See O5 and O6.
3. **`Goldens::{Existing, All}` and `--init`.** No reference to `GMEOW_CONFORMANCE_BLESS_INIT`
   remains anywhere in the tree. `write_expected` has three callers (`bless.tests.rs:49`,
   `dev_conformance.rs:122` and, through it, the dev-cli test) and all pass the new argument; no other
   crate or `xtask` calls it. The `init` code paths in `bless.rs:65-170` are unchanged, only their
   switch moved, so no behaviour is lost; the seed mode can no longer be changed by the caller's
   environment, which is a gain. The `All` branch was untested before this change and is still
   untested. A test is not required for the contract: the flag-to-enum mapping is a two-arm `if`, the
   flag is a clap-derived `bool`, and the seeding logic itself did not change. See O7.
4. **Shipped-rules refusal.** The condition (`profile.shipped_rules` is a non-empty array) matches
   exactly when `run_case` consults the library: `merge_shipped_rules` returns the program unchanged
   for an empty list (`run.rs:615-617`) and the library has no other use on either path that reaches
   it (`run.rs:248, 258`). A malformed or empty `shipped_rules` still fails, from the typed parser
   inside `run_case` (`profile.rs:289-321`). The test discriminates: without the check, the error
   would come from `run.rs:625-627` and would not contain the asserted "compiled rule library", and
   the test also asserts the earlier, valid case's golden is untouched. See O8 for the raw JSON read.
5. **Changed documentation.** Every changed sentence was checked against code:
   - The runner README status and wiring paragraphs: the producer's conformance stage executes the
     cases, and `corpus_native_conformance_goldens` (`execution.tests.rs:143-186`) grades the recorded
     stage artifacts with `compare::diff_case` without calling `run_case`. `make conformance`
     (`Makefile:1206-1207`) selects that test. The crate's own tests run only synthetic cases, and
     `external_corpora_gates.rs` executes no committed case, so "without re-executing the cases" holds.
   - The bless-rules paragraph and the `--init` sentence in the runner README, the `bless.rs` module
     and function docs, and `LANG-CONFORMANCE.md:187` are accurate.
6. **Logs 80-86.** Each is headed `6026d22c3` with an empty porcelain status, records the tool's
   own exit status, and says what the commit claims. Log 82's single error is in a line this branch
   did not introduce (see Evidence).

## Findings

REQUIRED: none.

### O5. OPTIONAL: the `Cli` kind's documentation names only two other commands

`crates/conformance/src/error.rs:170-174` documents `Cli` as an invalid argument surface of an
`ingest-external` or `conformance-report` invocation. It now also carries
`conformance-bless` refusals of a well-formed case it cannot process
(`dev_conformance.rs:93-112`). The doc is not false, but it no longer describes every use. Either
widen it to name the bless refusal or mint a kind for "case not blessable by this command".

### O6. OPTIONAL: an out-of-tree or unknown name is labelled as case anatomy

`dev_conformance.rs:140-148` raises `CaseAnatomy` for three conditions: the name does not resolve,
it resolves outside `conformance/logic/cases`, or the directory lacks `profile.json`. Only the last
is a malformed case as `error.rs:34-38` describes it. The first two are argument errors (`Cli`).
Splitting the closure would label each correctly.

### O7. OPTIONAL: seed mode has never been exercised

No test or recorded run uses `--init` or `Goldens::All`. One extra call in the existing dev-cli test
on a case with an empty `expected/` tree, asserting `Created` entries, would protect the documented
seeding behaviour.

### O8. OPTIONAL: the shipped-rules check re-reads raw JSON instead of the typed profile

`dev_conformance.rs:103-108` reads `case.profile.get("shipped_rules")` as untyped JSON. The public
`gmeow_conformance::profile::parse_profile` (`profile.rs:158`) returns the typed `Profile` with
`shipped_rules: Vec<String>`. Using it would follow the parse-at-the-boundary rule and refuse a
malformed profile before any case runs, instead of from inside `run_case`.

### Carried forward, unchanged

- N3 (OPTIONAL): the pipeline clippy errors and the dev-cli `large_unopt_stack_frames` error predate
  this branch.

## What only a producer pass can prove

- The three recorded MinCount violations are gone from the validation findings.
- The full set of ancestor shapes moved to the SPARQL target, with no new finding on the corpus or
  examples.
- The generated shapes and the `gmeow.gts` bundle agree, and the validation-fixture manifest is
  regenerated.
- The producer's conformance stage records `validation/reifier-shape` and the corpus grading test
  passes against the committed goldens. Log 85 shows the producer binary reproduces those goldens
  with the default library; for this case, which selects no shipped rules, the library is not
  consulted, so the stage's compiled-theory library cannot change the result. The recorded artifacts
  and the grading run are still unproven.
- WITNESS parity is unchanged.
- The branch merged with current `origin/main` passes `make check` (CI builds the merge).

## Verdict reasoning

Both round-4 REQUIRED findings are fixed, and the evidence shows the Make entry point now runs the
authenticated producer and reproduces the committed goldens. The delta introduces no false statement,
loses no behaviour, leaves no caller of the removed environment switch, and its new refusal is the
right condition with a discriminating test. The remaining items are optional quality improvements.
Combined with rounds 1, 2 and 4, no REQUIRED finding is open on the branch. The verdict is subject
to the producer pass and `make check` listed above, which this review cannot run.

VERDICT: PASS at 6026d22c3066f268b17b2d0711f9acc27f5ed65b
