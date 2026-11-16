BLOCKED at bcb94a541

# Delta review of issue 2727 at bcb94a541 (range cd0a85620..bcb94a541)

## Head and tree

- `git rev-parse HEAD` printed `bcb94a541898aed099d5070731a4ab8da1044e70`. `git status --porcelain` printed
  nothing, so the tree is clean.
- The delta is one commit, `bcb94a541`, whose parent is `cd0a85620`. It touches 5 files:
  - `conformance_standpoint.rs`: 2 comments added;
  - `docs/standpoints.md`: 1 bullet;
  - `slices/core/lifecycle/i18n/fr.po`: 1 msgstr;
  - the two ranker fixture headers.
- The branch merge base is still `e32f90c96`. `origin/main` has since moved to `bc4560473`, but
  none of the files main changed in that range overlaps a branch file. This is not a finding: the
  branch has to integrate main before landing anyway.
- I changed nothing in the worktree.

## What I read, and what I did not

**Read:**
- **The issue.** `stagectl issue 2727`: the body and all 4 comments, including the maintainer's
  decision and the later rulings.
- **The previous review.** `.stage/reviews/review-2727-at-cd0a85620.md`, all of it.
- **The delta.** `git diff cd0a85620..HEAD`, every hunk, and the commit message.
- **`docs/standpoints.md`.** Lines 1-215, the whole file.
- **`slices/core/standpoint/module.ttl`.**
  - Lines 56-85 (`StandpointTenure`).
  - Lines 800-835 (the ontology header `rdfs:comment`).
  - Lines 940-991 (`WithdrawnStandpointSuppressionConstraint`).
  - The lines 199, 1156, 1190 and 1286, compared with `origin/main`.
- **`slices/core/standpoint/docs.md`.** Lines 140-205, plus a grep for tenure, withdrawal,
  retraction and suppression across the file.
- **`docs/claim-lifecycle.md`.** A grep of the same terms; lines 1-110 reached through the hits.
- **`CONSTITUTION.md:190-225`.**
- **`crates/validate/tests/conformance_cases/`.**
  - `ontology_conformance.rs:35-80`.
  - `conformance_standpoint.rs:50-85`.
  - `conformance_events.rs:260-335`.
  - `conformance_epistemics.rs:1-40` and `540-600`.
- **`slices/core/gender/module.ttl:420-445`**, at head and on `origin/main`.
- **`slices/core/names/module.ttl`.** The `SupersededNameSuppressionConstraint` block.
- **Fixtures.**
  - `tests/fixtures/coverage/events.ttl:60-100`.
  - `tests/fixtures/coverage/genealogy-contested.ttl:50-66`.
  - The headers of `standpoint-withdrawn-warning.ttl`, `standpoint-ended-tenure-conforms.ttl` and
    `participation-wellformed.ttl` (and that last file's diff against main).
- **Other text.**
  - `slices/core/events/docs.md:15-60`.
  - `docs/reasoning.md:94-104,134-142`.
  - `README.md:788-796,855-861`.
  - `dsl/mappings/projections/sosa.ttl:85-107` and `schema-org.ttl:685-700`.
- **Statement lowering.** `crates/logic/src/operator.rs:330-375`, and every caller of
  `lower_reifiers` and `cli_world_dataset` (grep).
- **Evidence.** All six `.stage/issue-2727/r5-*.log`, in full.
- **The skill.** `rdf-work/SKILL.md:30-111`.

**Searches for any remaining sentence that states a removed or narrowed rule.** I ran `git grep` over
the whole tree, excluding `*.po`, `generated/`, `imports/`, `*.json`, `*.wasm` and `*.snap`. The
patterns were:
- every line containing `displayable` (951 lines, counted per file);
- `displayable`/`suppress` together with supersession, withdrawal, retraction, correction,
  closed/ended, `validUntil`, dispute, tenure, evaluation or revision;
- withdrawn/closed/ended tenure or standpoint near suppression;
- `supersededBy` near suppression;
- correction near suppression;
- `validUntil` near suppression;
- succession or retraction near liveness;
- "superseded/withdrawn/... are suppressed";
- the names of the removed constraints (`SupersededObservationSuppression`,
  `CorrectedEventSuppression`, `SupersededFacetSuppressionConstraint`);
- `#` comment lines in `queries/**/*.rq` and `slices/**/*.rq`;
- `//` comments in `crates/**/*.rs`.

That search covered `docs/`, `README.md`, `CONSTITUTION.md`, `governance/`, every slice `module.ttl`,
`docs.md`, design note, test and example, `queries/`, `dsl/`, the test fixtures and the Rust
comments.

**Commands I ran:**
- `rapper -i turtle -c` on both ranker fixtures: 25 and 23 triples, both exit 0.
- `prose-check` on `docs/standpoints.md`: exit 0.
- `prose-check` on the two fixtures and `conformance_standpoint.rs`: exit 0, but I cannot tell
  whether it examines `.ttl` and `.rs` content.
- A diff filter showing that every changed line under `tests/` and `crates/` in the delta is a
  `#` or `//` comment.
- `git grep` comparisons against `origin/main` and `e32f90c96`.

**Not read:**
- The remainder of each module and catalog outside the ranges above.
- The coder's `msgid-check` script itself.
- `rust-work/SKILL.md`.
- The purrdf SHACL engine.
- Translations other than the entries this branch added or changed.

**A note on /tmp.** One `git grep -o` over three revisions exceeded the call timeout. The harness then
moved it to the background, and it wrote its output file under `/tmp/claude-1000/...`. That file
came from the tool, not from me, and the search found no match. I wrote nothing except this review.

## Status of R1-R3

### R1: still open

Fixed in `docs/standpoints.md`. Two other sentences in the branch's scope still state the removed rule.

**`docs/standpoints.md`: consistent.** I read the whole file.
- Lines 62-65 say a tenure whose holder withdrew the position (a retraction by the holder of the
  held claim) is suppressed. A tenure that merely closed, or whose claim another party retracted,
  takes no flag.
- Lines 202-204 now read: "A claim its holder withdrew, or a `StandpointTenure` whose held claim its
  holder retracted, sets `gmeow:displayable false` during ordinary revision; a tenure that merely
  closed takes no flag."
- Line 61 ("recognition granted in 2008 and withdrawn in 2030 is an opened-then-closed tenure,
  retained and never deleted") and lines 186-187 are compatible with both.
- All of this agrees with:
  - `module.ttl:72` ("A closed tenure is not by itself a withdrawn one");
  - `module.ttl:74`;
  - the constraint at `module.ttl:949-991`.
- One residual ambiguity in the word "held claim" is listed as O3.

**Elsewhere: see the REQUIRED findings.**
- **D1:** `slices/core/standpoint/module.ttl:822`.
- **D2:** `tests/fixtures/coverage/events.ttl:79-80`.

**The three places the coder left.**

**(a) `CONSTITUTION.md:221-222`.** Not a finding against this branch: only the maintainer amends the
constitution. It now contradicts the branch:
- The constitution reads: "[`docs/standpoints.md`](./docs/standpoints.md) (a withdrawn standpoint /
  closed `gmeow:StandpointTenure` is suppressed, not deleted)."
- The branch's `docs/standpoints.md:63-64` reads: "a tenure that merely closed, or whose claim
  another party retracted, takes no flag". Lines 203-204 say "a tenure that merely closed takes no
  flag".
- `module.ttl:72` reads: "A closed tenure is not by itself a withdrawn one: the interval can end for
  any reason, and only a holder retracting its own held claim is a withdrawal."
- So the slash equates "withdrawn" with "closed" and says both are suppressed. That is the equation
  the branch removed, and the parenthesis now misdescribes the very document it cites.
- `CONSTITUTION.md:204` lists "a withdrawn standpoint" as a supersession trigger. That remains
  consistent under the narrowed meaning of "withdrawn".

**(b) `tests/fixtures/coverage/events.ttl:79-80`.** This is finding D2.

**(c) `ontology_conformance.rs:51`.**
- The comment says the suppression warning "migrated to gmeow:SupersededFacetSuppressionConstraint".
  No constraint by that name exists, at this head or on `origin/main`: `git grep` finds the string
  only in this comment, on both revisions.
- The actual constraint is `gmeow:SupersededGenderIdentitySuppressionConstraint`
  (`slices/core/gender/module.ttl:425`), and it carries
  `logic:formalizes gmeow:SupersededFacetSuppressionShape`. It has the same name and the same
  `formalizes` target on `origin/main` (gender `module.ttl:425,431`).
- The branch did not remove or rename anything here. It changed that constraint's definition and
  added the type-guarded selector, and it left this comment line untouched (the comment is
  byte-identical on main).
- So the comment is false about the name, and it was already false before this branch. That makes
  it O1, not a blocker.

### R2: resolved

- **The corrected entry.** `lifecycle/i18n/fr.po`, msgctxt `…/supersededBy|skos:definition`, now
  reads "…et le seul masquage que suscite la tranche lifecycle vise un nom remplacé ou une étiquette
  d'identité remplacée, que son titulaire masque à tout lecteur." That is hiding, as intended.
- **The msgid.** The delta changes only the `msgstr` line. The `msgid` line is unchanged, and no
  module literal changed in the delta, so the byte-equality established at cd0a85620 still holds.
- **Every fr entry the branch added or changed** (`git diff origin/main...HEAD -- '*fr.po'`, every
  `suppr*` occurrence):
  - "masquer"/"masqué(e)" is used for display suppression.
  - "supprimer"/"supprimé" and the bare noun "suppression" are used for deletion. Examples:
    "jamais une suppression" (never a deletion); "Ne pas déduire … une suppression" (English
    "deletion"); "signal de suppression" (English "deletion signal").
  - Where "suppression"/"supprimé" refers to display it is qualified: "SUPPRESSION de l'affichage",
    "supprimé de l'affichage … plutôt qu'effacé". Both are correct.
  - The R2 inversion does not occur anywhere else.
- **Every zh entry the branch added or changed:**
  - 隐藏 and 抑制 (显示抑制, 抑制显示, 对显示的抑制) are used for display suppression.
  - 删除 is used for deletion, including 删除信号 for "deletion signal".
  - "既不取代也不抑制被纠正的声明" correctly means "neither supersedes nor suppresses".
  - No misuse.

### R3: resolved for what this branch must do

**The fixture headers.** `standpoint-ranker-other-party-succession.ttl:1-11` and
`standpoint-ranker-own-succession-conforms.ttl:1-9`.
- They say each case "exercises that query over data already written in the lowered form". They
  also say: "Data written as RDF 1.2 reified statements does not reach these constraints on the
  validation paths: no validation path produces the lowered edges from RDF 1.2 input."
- This is true by the code reading:
  - `lower_reifiers` has exactly one production caller, `cli_world_dataset`
    (`operator.rs:513,566`).
  - Its only caller is `derive` (`operator.rs:351-356`), the operator's rule derivation, which is
    not a validation path.
  - The SHACL paths flatten without lowering (previous review, R3).
- The scenario sentences match the hand evaluation in the previous review. The other-party file
  reports the reversed pair and the missing closure. The own-succession file reports nothing for
  these constraints.
- No issue or PR numbers and no process wording.

**The two Rust comments** (`conformance_standpoint.rs:63-65` and `:71-72`).
- They are true and carry no issue or PR numbers.
- The first ends "…does not reach these constraints on the validation paths yet." The trailing
  "yet" hints at a future change. It is not an issue reference, but see O2.

**The ranker constraint definitions.** The phrase "through their RDF 1.2 reified bodies" occurs
nowhere at head or on `origin/main`. The actual sentences:
- `module.ttl:1156` (`moreEntrenchedThanAttributionConstraint`): "every gmeow:moreEntrenchedThan
  edge asserted from a gmeow:Standpoint must have an RDF 1.2 reifier of the same statement that
  carries both a gmeow:accordingTo value … and a gmeow:validFrom value …" and "A
  universal-to-existential formula over the lowered reifier components."
- `module.ttl:1190` (`moreEntrenchedThanRankerOrderConstraint`): "Both edges are matched through
  their RDF 1.2 reifiers, which must name the same ranker with gmeow:accordingTo, …"
- `module.ttl:1286` (`moreEntrenchedThanRankerClosureConstraint`): "All three edges are matched
  through their RDF 1.2 reifiers, which must name the same ranker with gmeow:accordingTo, …" and "A
  universal-to-existential formula over the lowered reifier components."

**Did this branch write them?** No.
- Each of these sentences is byte-identical on `origin/main` and `e32f90c96` (compared by `grep -F`
  on the full definitions).
- The branch reworded only the exemption and liveness sentences of 1190 and 1286. Its new words
  ("a gmeow:succeedsClaim or gmeow:correctsClaim statement whose reifier names the same ranker")
  speak of a reifier without claiming that the RDF 1.2 path reaches it. They are true of the lowered
  reifier node too.
- So this is a note for the separate defect, not a finding.

**Are they false as things stand?** Yes, as a description of validation behaviour.
- On every SHACL validation path, an RDF 1.2 reifier is never lowered, so nothing is "matched
  through" it.
- The attribution constraint would report a correctly attributed RDF 1.2 edge as a Violation.
- The order and closure constraints never match real rankings.

## REQUIRED findings

**D1. The standpoint slice's own header still states the removed rule:
`slices/core/standpoint/module.ttl:822`.**
- **Where.** The shipped `rdfs:comment` of `<…/slices/standpoint>`, DOCTRINE bullet: "Suppression
  for ordinary revision (Principle 10): a withdrawn claim or a closed tenure sets gmeow:displayable
  false; it is retained, not deleted."
- **Why it blocks.**
  - This is the sentence R1 corrected in `docs/standpoints.md:202-203`, now found in the slice's own
    ontology header. That header ships in `gmeow.gts`.
  - It contradicts the same file's `StandpointTenure` definition (`:72`), its `avoidWhen` (`:74`)
    and the narrowed `WithdrawnStandpointSuppressionConstraint` (`:959`).
  - It is unchanged from main, but it sits in a file this branch edits. The branch's purpose is to
    remove exactly this equation, and its commit message says the guide no longer says it.
- **Smallest correction.** Reword the bullet to: "Suppression for ordinary revision (Principle 10):
  a claim its holder withdrew, or a tenure whose held claim its holder retracted, sets
  gmeow:displayable false; a tenure that merely closed takes no flag; nothing is deleted."
  - The comment has no fr or zh catalog entry today (grep: 0 hits in both catalogs). AGENTS.md § 8
    requires entries for a changed literal, and other slices catalog their header `rdfs:comment`
    (events, names). So the change needs a fr and a zh entry whose msgid equals the new comment byte
    for byte.

**D2. The events coverage fixture describes correction by suppression:
`tests/fixtures/coverage/events.ttl:79-80`.**
- **Where.** "A superseded participation claim — a mistaken attendee, corrected by SUPPRESSION
  (gmeow:displayable false), never deletion (Principle 10). validUntil closes it."
- **Why it blocks.**
  - The maintainer's rulings separate the two acts: "correction and suppression are separate acts";
    "No data may set `gmeow:displayable false` because a claim is disputed, corrected or evaluated".
  - Issue comment 3 lists the events slice's "corrections use `gmeow:displayable false`" as scope of
    this issue. The branch's own `docs/claim-lifecycle.md` heads a section "Correction is not
    suppression".
  - The branch rewrote this fixture's exact twin (`participation-wellformed.ttl`, previously "A
    superseded (corrected) participation, suppressed") for this reason, but left this one.
  - The rationale also ties the flag to the closing `validUntil`, which the ruling says is not a
    reason.
- **What still holds.** The data and the test
  `schema_role_projection_suppresses_withdrawn_participation` (`conformance_events.rs:319-336`) are
  fine if the record is read as its recorder's own withdrawal.
- **Smallest correction.** Comment only; the data is unchanged: "A participation its recorder
  withdrew as mistaken — the recorder's own retraction, so it is suppressed (gmeow:displayable
  false) and retained, never deleted (Principle 10)."

## OPTIONAL findings

- **O1. The constraint name at `ontology_conformance.rs:51` is false.** Write
  `gmeow:SupersededGenderIdentitySuppressionConstraint` (it formalizes
  `gmeow:SupersededFacetSuppressionShape`). This predates the branch, but the file is touched.
- **O2. Drop "yet" at `conformance_standpoint.rs:65`.** The fixture headers state the same fact
  without it, and a forward hint is close to the banned "will be fixed" wording.
- **O3. "Held claim" names two different properties.**
  - `StandpointTenure` `howToUse` (`module.ttl:79`) binds "the held claim with gmeow:tenurePosition".
    The standpoint `docs.md:203` likewise says "`tenurePosition` the standpoint-indexed claim it
    held".
  - The narrowed constraint (`module.ttl:959`), `docs.md:167` and the fixture headers mean
    `gmeow:standpointClaim` by "held claim".
  - So a reader of `docs/standpoints.md:62-63` and `:203` who retracts the `tenurePosition` statement
    draws no warning.
  - Fix: name the property once in `docs/standpoints.md`, or say "the generated claim
    (`gmeow:standpointClaim`)".
- **O4. Similar wording in genealogy: `tests/fixtures/coverage/genealogy-contested.ttl:57`** ("a
  mistaken attribution, corrected by suppression"). The data shows the parish withdrawing its own
  claim (`accordingTo ex:standpoint-parish`, `refuted`), so only the word "corrected" is wrong.
  Suggested: "withdrawn by its own standpoint, so suppressed".
- **O5. Projection loss notes say superseded observations are suppressed.**
  `dsl/mappings/projections/sosa.ttl:106` and `schema-org.ttl:699` say "Non-displayable/superseded
  observations are suppressed". Both projections suppress only on `gmeow:displayable false`
  (`suppressWhen`). Now that no constraint prompts the flag on an ended observation, an unflagged
  superseded observation is projected. Suggested: "observations flagged gmeow:displayable false are
  suppressed".
- **O6. A stale path in `docs/standpoints.md:8`.** It names `ontology/modules/standpoint.ttl`, which
  does not exist. The slice lives at `slices/core/standpoint/module.ttl`. This predates the branch.
- **O7. The evidence does not name the head.**
  - Five of the six `r5-*.log` files record `cd0a85620` plus the dirty list of the five files, not
    `bcb94a541`: `r5-cargo`, `r5-lint`, `r5-msgid`, `r5-prose` and `r5-rapper`. Only `r5-commit.log`
    names `bcb94a541` (git's commit line), after the hooks pass.
  - All six end with an `EXIT=0` line; `r5-rapper` has one per file.
  - `r5-cargo.log` shows a bare `cargo check`, with no `--manifest-path` and no `--tests`. A plain
    `cargo check` does not compile integration tests, so it says nothing about
    `conformance_standpoint.rs`. The delta there is comments only, so nothing is at risk.
  - `r5-prose` covered only `docs/standpoints.md`.

## Notes for the maintainer

- **The constitution sentence.** `CONSTITUTION.md:221-222`, "(a withdrawn standpoint / closed
  `gmeow:StandpointTenure` is suppressed, not deleted)", now contradicts `docs/standpoints.md:63-64`
  and `:203-204`, and `standpoint/module.ttl:72`; see (a) above. Suggested amendment: "(a standpoint
  tenure whose holder withdrew it is suppressed, not deleted; a tenure that merely closed is not)".
  The same lines also cite `tests/test_standpoint.py` and `tests/test_suppress_gen.py`, which no
  longer exist.
- **Out of the four named constraints.** Two other constraints still fire on a bare `validUntil`,
  the pattern ruled out for observations. This branch rightly leaves them alone (the ruling names
  four):
  - accessibility (`slices/extensions/accessibility/module.ttl:201`, "one carrying
    gmeow:validUntil");
  - email `MessageParticipant` (`slices/extensions/email/module.ttl:1382`).

## Note for the separate lowering defect

- **The definitions.** These sentences in the ranker definitions predate this branch and are false
  on every validation path today:
  - "Both edges are matched through their RDF 1.2 reifiers" (`standpoint/module.ttl:1190`);
  - "All three edges are matched through their RDF 1.2 reifiers" (`:1286`);
  - "must have an RDF 1.2 reifier of the same statement" (`:1156`).
- **Where the gap is.** The SHACL view flattens without lowering. As a result, the attribution
  constraint rejects every correctly attributed RDF 1.2 ranking, and the order and closure
  constraints never see one.
- **What that change needs.** Lower on the validation paths, add the RDF 1.2 twins of the two ranker
  fixtures, and reconcile these definitions with the result.
- **The rdf-work rule.** A projection that drops a layer must report it, never silently (`rdf-work`
  SKILL.md:42-45). The flattening drops the component layer without reporting it.

## Verdict scope

- **What the verdict rests on.** It covers the whole branch: my reading of the review at cd0a85620
  (which covered `origin/main...HEAD`) plus this delta.
- **Earlier findings.** The delta changes no behaviour, so they stand as that review left them:
  - **No behaviour changed.** Every changed line under `tests/` and `crates/` is a `#` or `//`
    comment. No module literal or constraint changed, and no msgid.
  - **No golden key changed.** The reviewed-coverage golden is untouched, and a msgstr rewrite keeps
    its key.
  - **What that leaves.** F1-F4 and F6 stay resolved; R2 and R3 are now resolved.
- **What blocks.** D1 and D2 belong to R1's class: remaining sentences that state the removed rule.
  The earlier review did not catch them, and they block now.

## Not shown (unexecuted)

- No `make check`, producer run, `make produce-test-fixtures`, nextest, conformance runner,
  reasoning or slice-quality sweep exists at `bcb94a541`, from me or from the coder.
- The coder's checks ran on the pre-commit tree at `cd0a85620`, not at the commit:
  - `cargo check` (no test targets);
  - lint;
  - `msgid-check lifecycle`;
  - `prose-check` of one document;
  - rapper.
- The pre-commit hooks in `r5-commit.log` did run on the commit itself.
- No case was ever executed, including the two ranker cases, the standpoint warning cases and the
  events projection test. All behaviour statements here and in the previous review come from reading
  code and working formulas by hand.
- No RDF 1.2 ranking was validated on any path.
- `corpus_reviewed_coverage_matches_frozen_golden` was not run.
