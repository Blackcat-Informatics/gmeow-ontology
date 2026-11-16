BLOCKED at 1d39b88d5

# Final full review of issue 2727 at 1d39b88d5

Written 2026-11-16 00:46 CST.

## Head, tree and range

- `git rev-parse HEAD` printed `1d39b88d50acf9b4b12950edb6ca5fc91b375a3c`. `git status --porcelain`
  printed nothing, so the tree is clean.
- The delta since the last review (`4e3231719..1d39b88d5`) has two commits:
  - `60e660c6b`, the fix commit, changes 10 files;
  - `bae740871` is a merge of `origin/main`, which is now the merge base.
- I reviewed the whole branch with `git diff origin/main...HEAD`: 75 files, 1026 lines added, 338
  removed.
- I wrote nothing in the worktree. Scratch files are under
  `.stage/issue-2727/review-scratch/`. The only other file I wrote is this one.

## Verdict

BLOCKED. There are two REQUIRED-RULE findings and one REQUIRED-OTHER finding.
- The two rule findings are sentences outside the eight slices that earlier rounds read. They
  state the rule the maintainer's decision removes, and each contradicts text this branch now
  ships.
- The other finding is development-history wording in four modules the branch changes.

The rest of the contract holds:
- the four named constraints are narrowed, and the observation constraint is removed;
- the `gmeow:displayable` notes and the claim-relation notes say which case is which;
- the counter-examples exist;
- every changed literal has a byte-equal French and Chinese entry with a faithful translation;
- the merged golden is exact.

## REQUIRED-RULE findings

### R1. The inference slice says a solver's evaluation or another party's attack sets `gmeow:displayable false`

**The text:**

- `slices/core/inference/docs.md:135-136`: in abduction, "the winner's `gmeow:claimModality` is
  promoted ... and the losers are suppressed (`gmeow:displayable false`)".
- `slices/core/inference/docs.md:139-145`: "When the solver evaluates a `gmeow:Argument` as
  `gmeow:acceptanceOut` via a `gmeow:ArgumentEvaluation` — a verdict to which a `gmeow:Attack`
  contributes — suppression follows: the conclusion-claim is marked `gmeow:displayable false`".
- `slices/core/inference/module.ttl:677` (the slice `rdfs:comment`): "a solver-fired gmeow:Attack
  sets the conclusion-claim gmeow:displayable false". Line 678 says "the losers are suppressed".
- `slices/core/inference/module.ttl:196` (`gmeow:InferenceTenure` definition): the tenure is
  "closed when a defeater fires ... Belief revision is the closing of a tenure (and
  gmeow:displayable false on the conclusion-claim)".
- `slices/core/inference/examples/belief-revision.ttl:4, 13, 38, 90`: the worked example sets the
  flag because the argument was evaluated OUT after an undercut.
- `README.md:41-42`: "A fired defeater marks the old conclusion `displayable false`".

**Why this is a wrong rule:**

- The maintainer's decision (comment 2) says: "No data may set `gmeow:displayable false` because a
  claim is disputed, corrected or evaluated." An `gmeow:ArgumentEvaluation` is an evaluation, and
  an abductive ranking that suppresses its losers is one too. A `gmeow:Attack` can come from any
  party's argument, and none of these sentences names the holder.
- The sentences also contradict text this branch ships:
  - `gmeow:hasDefeatStatus` avoidWhen (`epistemics/module.ttl:553`): a defeat status "is not a
    reason to set gmeow:displayable false, because a defeated claim stays retrievable and its
    display is reader policy";
  - `gmeow:defeatedBy` avoidWhen (`:436`) says the same.
- The issue asks that the guidance on the claim relations say which cases are global safety and
  which are reader policy. The ontology now answers the defeat question two opposite ways.
- The coder's sweep (`.stage/issue-2727/sweep.md:376-386`) marked these lines "OUT OF SCOPE ...
  open for the maintainer". No issue comment records that the maintainer took them out of scope.
  They are the same class as the epistemics defeater sentences the last review required (R-1).

**Smallest correction:**

- Restate the six places so that suppression is the holder's own act. The agent whose conclusion
  it is closes its own tenure and flags its own conclusion when it accepts the defeat.
- Say that a solver verdict, an attack from another party, or losing an abductive ranking sets no
  flag by itself, and that what a reader is shown is decided at projection.
- Keep the worked example's data only if its comments say the operator withdrew its own
  conclusion.
- Give the changed `skos:definition` and slice `rdfs:comment` literals French and Chinese entries
  with a byte-equal `msgid`, and refresh the reviewed-coverage golden.
- If the maintainer rules the inference slice out of this issue, give it a tracked issue as was
  done for places (#2830) before merge.

### R2. `gmeow:supersedes` treats display preference as data and contradicts its inverse

**The text:**

- `slices/core/coreference/module.ttl:72` (definition): "suppression from display is handled
  separately with gmeow:displayable false when appropriate".
- `:74` (avoidWhen): "suppress it from display with gmeow:displayable false if it should no longer
  surface".
- `:76` (howToUse): "mark them gmeow:displayable false when they should drop out of projections".
- `slices/core/coreference/docs.md:64` repeats this.

**Why this is a wrong rule:**

- Decision comment 2 says: "display preference is reader policy, not data ... What a particular
  reader sees is decided at projection". "When they should drop out of projections" is exactly a
  display preference written into the data. It names no holder and no safety case.
- The inverse relation `gmeow:supersededBy`, which this branch rewrote
  (`lifecycle/module.ttl:111, 113, 115`), now says the opposite: "The relation itself prompts no
  display flag: whether a reader is shown the predecessor is that reader's policy, decided at
  projection". One supersession read in two directions now carries contradicting guidance.
- Comment 3 names `gmeow:supersedes` as the property through which succession "prompts
  suppression of the other party's claim". Comment 4 rules that "Only a claim's own holder ends
  its liveness or supersedes it".

**Smallest correction:**

- Align the three `gmeow:supersedes` literals and the guide sentence with the `gmeow:supersededBy`
  text: the relation prompts no display flag, a superseded name or identity label its holder hides
  is the only global case, and anything else is reader policy decided at projection.
- Add French and Chinese entries for the changed literals and refresh the golden.

## REQUIRED-OTHER findings

### RO1. Development-history headings remain in four modules this branch changes

- `slices/core/events/module.ttl:1614`, which this branch edited:
  "# --- Wave B1 shape-drain: event well-formedness ---".
- `slices/core/gender/module.ttl:452`, `slices/core/names/module.ttl:1701` and
  `slices/core/standpoint/module.ttl:1102` carry the same "Wave B1 shape-drain" prefix.
- AGENTS.md § 1 bans development history in the repository ("remove such text where you see
  it"), and the brief's item 5 asks for no process wording in any file the branch changes.
- The search found no issue or pull-request number left in the 74 changed files (the golden JSON
  excluded). "Principle #3" in `docs/standpoints.md:72` cites a principle, not a process.
- Smallest fix: delete "Wave B1 shape-drain: " from the four headings.

## OPTIONAL findings

- **O1. Suppression rules outside the four named constraints.**
  - Several constraints still prompt `gmeow:displayable false` from an ended interval, a status or
    a cascade, with no holder act:
    - `gmeow:RetractedTaggingSuppressionConstraint` (`tags/module.ttl:260`);
    - `gmeow:RetractedAccessibilityAssertionSuppressionConstraint`
      (`accessibility/module.ttl:196`);
    - `gmeow:WithdrawnMessageParticipantSuppressionConstraint` (`email/module.ttl:1377`);
    - `gmeow:ExpiredTrademarkSuppressionConstraint` (`rights/module.ttl:2099`);
    - the three finance status constraints (`finance/module.ttl:990, 1017, 1044`);
    - the cancelled-occurrence constraint (`calendar/module.ttl:763`);
    - the reply cascade (`notes/module.ttl:312`).
  - Prose states the same pattern for lapsed knowledge (`cognition/module.ttl:67, 226`).
  - Places is tracked as #2830. These are not.
  - The issue's acceptance names only the four constraints, so this does not block this branch.
    The rule they state is the one the decision removed for observations, so they need a tracked
    owner.
- **O2. The `gmeow:displayable` definition does not name the holder-withdrawal case.**
  - The definition (`names/module.ttl:310`) opens "Whether a name or identity facet may be shown"
    and names only superseded names and identity labels as the global case.
  - Elsewhere the branch applies the flag to records their own holder withdrew: tenures, claim
    tokens, evaluations, defeaters and dream reports.
  - A clause saying that a record its own holder withdrew or replaced also takes the flag, and that
    this too is never set by another party, would make the term state the whole rule.
- **O3. The gender example is consistent, but its data does not record the supersession.** See
  item 3 below.
- **O4. The new French and Chinese `gmeow:defeatedBy` definitions render "undercut" with the word
  used for "undermined".**
  - French "(Gettier / défait / sapé / réfuté)" against "sapé, coupé" in `hasDefeatStatus`.
  - Chinese "被削弱" against "被削弱、被截断".
  - The display rule is translated faithfully.
- **O5. Test wording.**
  - `conformance_music_oral_tradition.rs:165` still opens "Twin of
    `test_contested_membership_is_suppressed_not_deleted`". That retired name asserts the removed
    rule, while the test now asserts the opposite. The "Twin of" references in the other changed
    test files are history too.
  - The loop at `:513-519`, which asserts that every returned row's authority is the Kirana
    standpoint, restates the query's own `FILTER`.
- **O6.** `epistemics/module.ttl:555` (`hasDefeatStatus` howToUse) still says "suppressing the old
  one" without saying whose. This is carried over from the last review.

## 1. The rule, and the whole-worktree search

**The rule the issue establishes.** I took it from the issue body, the decision (comment 2) and the
maintainer's rulings (comment 4):

- `gmeow:displayable false` is a global safety control: a deadname, or another superseded name or
  identity label, is hidden from every reader.
- A record its own holder withdrew or replaced may be suppressed. Comment 4 rules this for the
  standpoint tenure.
- No data sets the flag because a claim is disputed, corrected or evaluated by another party. What
  a reader sees is reader policy, decided at projection.
- An ended validity interval is not by itself a reason (the observation clause).
- Only a claim's own holder ends its liveness or supersedes it.

**The search command.** One search, run from the worktree root:

```
git grep -n -P '(?i)^(?=.*(displayable|suppress))(?=.*(supersed|withdr|retract|correct|disput|ended|expired|closed|lapse|revis|replac|outrank|deadname|\bold\b|earlier))' -- '*.ttl' '*.md' '*.rq' '*.rs' '*/i18n/*.po'
```

- It exited 0 with 723 hits, saved in `review-scratch/rule-search.txt`. The full list is at the
  end of this file.
- I read every hit. Where the hit line did not settle the class, I read the line in context.
- The classification is reproducible: `review-scratch/classify-hits.pl` encodes it.

**Classes:**

| Class | Hits | Meaning |
|---|---|---|
| RULE | 403 | states the issue's rule, or applies it |
| GENERIC | 135 | a Principle 10 or own-revision statement that names no other party (an agent revising its own memory, a tag retracted by its tagger); consistent with the rule |
| OLD-defeat-or-evaluation | 5 | R1 |
| OLD-display-preference-as-data | 3 | R2 |
| OLD-places (tracked separately) | 35 | the places supersession rule, tracked as #2830 |
| OLD-expiry | 14 | O1 |
| OLD-ended-interval | 22 | O1 |
| OLD-status | 6 | O1 |
| OLD-cascade | 3 | O1 |
| NONE | 97 | no display rule: engine or session retraction, snapshot-delta suppression, cue suppression, k-anonymity, projections that honour an existing flag |

**What the line search misses.** It matches single lines. R1 also covers
`inference/docs.md:135-136, 142-143`, `module.ttl:678` and `examples/belief-revision.ttl:13, 38,
90`. These state the rule without a trigger word on the same line, and I found them by reading
the hits in context.

**Catalog entries.** French and Chinese catalog hits are classified like their English source. A
`msgstr` line appears only when it repeats an English trigger word.

## 2. Changed term definitions and their catalog entries

**The msgid check.**
- `review-scratch/msgid-eq.pl` resolves each changed literal in the 11 changed modules to its
  subject and predicate, then compares it byte for byte with the `msgid` of the matching
  `msgctxt` in `fr.po` and `zh.po`.
- It covered 43 literals (86 catalog entries). Each line came from the `git diff -U0` hunk list;
  the four triple-quoted slice comments were checked from their opening line.
- Every entry exists and every `msgid` is EQUAL. The output is in `review-scratch/msgid-eq.out`.
- `logic:message` literals are untagged and have no catalog entries, here or on `main`.

**Translations.** I read every French and Chinese `msgstr` against the English and found them
faithful:
- "masquer/masqué" and "隐藏" mean hiding, and "supprimer" and "删除" mean deletion;
- the holder, other-party and reader-policy clauses are all present.
- The only defect is the term choice in O4.

**Catalog syntax.** `msgfmt -c -o /dev/null` returned 0 for all 20 changed catalogs, printing only
the usual header warnings.

## 3. The gender example `self-asserted-facets.ttl`

**The data.**
- `ex:registryId` is a `gmeow:GenderIdentity` about Robin. Its vantage is the registry, its value
  is `genderWoman`, and it carries `selfAsserted false` and `displayable false`.
- `ex:selfId` is Robin's self-asserted `genderNonBinary`.

**Judgment.** The data is consistent with the rule. I rely on:
- CONSTITUTION Principle 10 (`CONSTITUTION.md:194`): "A superseded label — a deadname, a former
  gender — is recorded with `gmeow:displayable false`".
- Decision comment 2: `gmeow:displayable` "keeps its global safety meaning (a deadname or other
  superseded label is suppressed for every reader".
- `gmeow:displayable` useWhen (`names/module.ttl:317`): "a superseded gender/orientation label that
  must never be surfaced for any reader".
- The gender slice comment (`gender/module.ttl:381`): tenet (4) calls the flag "the deadname
  analog for a superseded label". The `gmeow:IdentityFacet` definition (`:98`) says "a superseded
  facet is kept with gmeow:displayable false". `gmeow:hasGenderIdentity` (`:293`) relates the
  person to the facets "they hold", and `ex:robin` bears both facets.

A gender recorded by a registry, which the person has replaced by asserting their own identity, is
the "former gender" case. Showing it would misgender the person. It is a superseded identity label
held for the person, not another party's claim about some other matter. The revised comments now
say this and no longer give precedence as the reason.

**What the data does not show (O3).**
- The data does not record the supersession: `ex:registryId` has no `gmeow:validUntil`, so
  `gmeow:SupersededGenderIdentitySuppressionConstraint` does not prompt it, and the flag is
  asserted rather than prompted.
- The names example draws the contrasting line. Its registry name "Robin Chan"
  (`names/examples/person-names.ttl:88-112`), a disputed name with no safety element, carries no
  flag.
- One sentence in the gender example contrasting the two cases would make the boundary visible.

## 4. The merge of main and the reviewed-coverage golden

**The conflict log.** `r8-merge.log` records the conflict in
`crates/docs/tests/fixtures/reviewed_coverage_golden.json` only (EXIT=1, the expected conflict
status).

**The regeneration.**
- `r8-golden2.log` records `cargo run -p gmeow-docs-model --example
  refresh-reviewed-coverage-golden`, run during the merge with HEAD at `60e660c6b`, EXIT=0:
  "190 catalogs (64507 reviewed entries)".
- That is the command the `make maint-refresh-reviewed-coverage-golden` target runs
  (`Makefile:1322-1323`).
- `r8-mergecommit.log` shows the merge commit's hooks passing, EXIT=0.

**My three-way check.** I compared the golden at the old base `e32f90c96`, at main `bae740871`, at
the branch side `60e660c6b` and at head:
- for every catalog, head equals the branch list plus main's additions minus main's removals;
- the `jq` comparison found 0 discrepancies;
- every list is sorted and has no duplicates.

**My catalog check.**
- I parsed all 190 golden catalogs at head with `review-scratch/po-keys.pl`, keeping each
  non-fuzzy entry with a non-empty `msgstr` and its key expanded as the producer expands it:
  64507 keys, the same total as the golden.
- The 10 keys that differ in each direction are artefacts of my parser:
  - `affect` and `software` catalogs have an entry with no blank line before it (also so on
    `main`);
  - a `lang` entry has a leading space before `msgctxt`;
  - three entries have identical `msgid` and `msgstr`, which the producer's integrity rule
    excludes.
- No difference is in a catalog this branch changes, except the `software` layout, which is
  unchanged from `main`.

**The 198 tracked `*/i18n/*.po` files.** They are the 190 golden catalogs plus 8 test fixtures,
which the producer does not discover.

## 5. Process references and commit messages

**Process references.** See RO1. The fix commit removed the issue numbers the last review listed
(R-2), from `ical.ttl`, `events.ttl`, `organization-contested.ttl` and `standpoint.ttl`; the
search confirms none remain.

**Commit messages.** I read all 15 non-merge commit messages on the branch.
- Each describes its own commit.
- `a6b238b1e` says the facet guard requires `gmeow:IdentityFacet`. A later commit replaced that
  guard with the explicit class list, so the message is history, not the head state.
- `60e660c6b` is accurate: its title says "drop process references", and it dropped the numbered
  ones.
- The squash message has not been written; it should describe the head state.

## 6. Tests the branch adds or changes

Every test I read protects a named behaviour, and its stated reason is true at head.

| Test | Behaviour it protects |
|---|---|
| `ontology_conformance.rs` `deadname_without_flag_draws_the_suppression_warning` | an unsuppressed deadname still warns (acceptance 3) |
| `ontology_conformance.rs` `ended_observation_draws_no_suppression_warning` | the observation clause |
| `conformance_standpoint.rs` `ended_tenure_without_retraction_draws_no_warning` | an ended tenure alone does not warn |
| `conformance_standpoint.rs` `retraction_by_another_party_draws_no_warning` | another party's act prompts nothing |
| the two ranker cases | the holder-only liveness ruling (comment 4) |
| `conformance_events.rs` `.no_warning` on `participation-wellformed` | the removed participation warning |
| the music test, renamed | a contested membership is attributed and unflagged |
| the structural assertions in events, observations, lifecycle, gender and names | the narrowed guards and the disputed-name example |

- **The ranker fixtures.** Their comments say truthfully that they exercise pre-lowered data only.
  The lowering gap is tracked as #2807.
- **Weak test value.** O5 is the only one.

## Evidence and what nobody has shown

**What I read:**
- the issue body and all four comments (`stagectl issue 2727`);
- the last review in full, `r8-working.md` and every `r8-*.log`;
- `AGENTS.md` and `CONSTITUTION.md` Principle 10;
- the full branch diff for every module, test, fixture and catalog entry;
- the inference, coreference, gender and names example texts cited above;
- each hit of the search.

**What I did not read:**
- `rdf-work/SKILL.md`;
- the catalogs outside the changed entries, except as parsed for item 4;
- the producer's `translation_has_integrity`;
- the purrdf SHACL engine.

**The commands I ran:** `git`, `jq`, byte comparison in Perl, `msgfmt -c`,
`rapper -c` (7 files from the fix commit, rc 0 each), `stagectl issue`, `gh issue list`
(read-only) and `prose-check` on this file.

**What I did not run:** no cargo, make, producer, gate, test, nextest or reasoning run, and I did
not touch the build lock.

**The coder's evidence at r8:**
- `r8-lint.log` names head `1d39b88d5`: `pre-commit run --all-files` with every hook Passed,
  EXIT=0.
- `r8-golden*.log`, `r8-merge*.log` and `r8-commit.log` are as described above.
- There is no r8 msgid, prose-check or rapper log. My own checks above cover the msgid and parse
  questions.

**Not shown by anyone at head:**
- no `make check`, producer run, `make produce-test-fixtures` or nextest;
- `corpus_reviewed_coverage_matches_frozen_golden` has not executed against this golden;
- none of the added validator cases, structural assertions or the music competency test has
  executed;
- every statement here about constraint behaviour comes from reading formulas, not execution.

## Full list of search hits

Each line is `file:line` and its class.

- `AGENTS.md:125` RULE
- `CONSTITUTION.md:192` RULE
- `CONSTITUTION.md:194` RULE
- `CONSTITUTION.md:293` GENERIC
- `CONSTITUTION.md:304` NONE
- `README.md:41` OLD-defeat-or-evaluation
- `README.md:58` NONE
- `README.md:59` NONE
- `README.md:112` GENERIC
- `README.md:113` GENERIC
- `README.md:118` RULE
- `README.md:443` NONE
- `README.md:729` RULE
- `README.md:777` RULE
- `README.md:802` GENERIC
- `README.md:836` OLD-expiry
- `README.md:868` OLD-places (tracked separately)
- `conformance/logic/cases/enactment/cc1-continuing-cluster/input.logic.ttl:136` NONE
- `conformance/logic/cases/transaction/memory-triad-execute/expected/explanation/cb7b16225038b47cf8acf4c5cda5520501b6e266.md:25` NONE
- `conformance/logic/cases/transaction/trajectory-audit/expected/explanation/5d4159e6890914b8dbcbe97b297e4d3ae9a889bf.md:25` NONE
- `conformance/logic/cases/transaction/trajectory-audit/expected/explanation/6d8bee53ca10d13947d7e0fc8db5e32e4c4e4480.md:35` NONE
- `crates/gmeow-cli/src/commands.rs:4518` NONE
- `crates/gmeow-cli/src/commands.rs:4599` NONE
- `crates/gmeow-cli/src/lib.rs:1355` NONE
- `crates/gmeow-cli/tests/logic_session_checkpoint_retract_cli.rs:80` NONE
- `crates/gmeow-cli/tests/logic_session_checkpoint_retract_cli.rs:157` NONE
- `crates/gmeow-cli/tests/logic_session_checkpoint_retract_cli.rs:177` NONE
- `crates/logic-compile/src/frontend/tests.rs:4136` NONE
- `crates/logic/src/runtime/session/facade.rs:455` NONE
- `crates/logic/src/runtime/session/facade.rs:462` NONE
- `crates/logic/src/teleology/tests.rs:750` NONE
- `crates/logic/src/transaction/execute.tests.rs:69` NONE
- `crates/logic/tests/reasoning_session_annotation_parity.rs:162` NONE
- `crates/logic/tests/reasoning_session_parity.rs:4` NONE
- `crates/logic/tests/reasoning_session_parity.rs:143` NONE
- `crates/logic/tests/reasoning_session_parity.rs:177` NONE
- `crates/logic/tests/reasoning_session_parity.rs:224` NONE
- `crates/logic/tests/reasoning_session_parity.rs:309` NONE
- `crates/logic/tests/reasoning_session_parity.rs:445` NONE
- `crates/mcp/src/storage.rs:553` GENERIC
- `crates/mcp/src/storage.rs:648` GENERIC
- `crates/mcp/src/storage.rs:1179` GENERIC
- `crates/mcp/src/storage.rs:1182` GENERIC
- `crates/mcp/src/tests.rs:2151` GENERIC
- `crates/mcp/src/tests.rs:2175` GENERIC
- `crates/mcp/src/tests.rs:2182` GENERIC
- `crates/validate/src/abductive.rs:562` NONE
- `crates/validate/src/shape_oracle.rs:2557` NONE
- `crates/validate/tests/conformance_cases/conformance_employment.rs:10` GENERIC
- `crates/validate/tests/conformance_cases/conformance_employment.rs:12` GENERIC
- `crates/validate/tests/conformance_cases/conformance_employment.rs:47` GENERIC
- `crates/validate/tests/conformance_cases/conformance_employment.rs:48` GENERIC
- `crates/validate/tests/conformance_cases/conformance_employment.rs:72` GENERIC
- `crates/validate/tests/conformance_cases/conformance_employment.rs:75` GENERIC
- `crates/validate/tests/conformance_cases/conformance_employment.rs:84` GENERIC
- `crates/validate/tests/conformance_cases/conformance_epistemics.rs:19` RULE
- `crates/validate/tests/conformance_cases/conformance_epistemics.rs:95` RULE
- `crates/validate/tests/conformance_cases/conformance_epistemics.rs:184` RULE
- `crates/validate/tests/conformance_cases/conformance_epistemics.rs:192` RULE
- `crates/validate/tests/conformance_cases/conformance_epistemics.rs:270` RULE
- `crates/validate/tests/conformance_cases/conformance_epistemics.rs:337` RULE
- `crates/validate/tests/conformance_cases/conformance_epistemics.rs:343` RULE
- `crates/validate/tests/conformance_cases/conformance_epistemics.rs:557` RULE
- `crates/validate/tests/conformance_cases/conformance_epistemics.rs:561` RULE
- `crates/validate/tests/conformance_cases/conformance_epistemics.rs:579` RULE
- `crates/validate/tests/conformance_cases/conformance_epistemics.rs:581` RULE
- `crates/validate/tests/conformance_cases/conformance_epistemics.rs:584` RULE
- `crates/validate/tests/conformance_cases/conformance_events.rs:14` RULE
- `crates/validate/tests/conformance_cases/conformance_events.rs:20` RULE
- `crates/validate/tests/conformance_cases/conformance_events.rs:320` RULE
- `crates/validate/tests/conformance_cases/conformance_events.rs:322` RULE
- `crates/validate/tests/conformance_cases/conformance_events.rs:326` RULE
- `crates/validate/tests/conformance_cases/conformance_events.rs:335` RULE
- `crates/validate/tests/conformance_cases/conformance_genealogy.rs:11` RULE
- `crates/validate/tests/conformance_cases/conformance_genealogy.rs:13` RULE
- `crates/validate/tests/conformance_cases/conformance_genealogy.rs:48` RULE
- `crates/validate/tests/conformance_cases/conformance_genealogy.rs:92` RULE
- `crates/validate/tests/conformance_cases/conformance_genealogy.rs:93` RULE
- `crates/validate/tests/conformance_cases/conformance_genealogy.rs:96` RULE
- `crates/validate/tests/conformance_cases/conformance_genealogy.rs:105` RULE
- `crates/validate/tests/conformance_cases/conformance_identity_over_history.rs:220` RULE
- `crates/validate/tests/conformance_cases/conformance_identity_over_history.rs:229` RULE
- `crates/validate/tests/conformance_cases/conformance_notes.rs:239` GENERIC
- `crates/validate/tests/conformance_cases/conformance_organization.rs:74` GENERIC
- `crates/validate/tests/conformance_cases/conformance_places.rs:17` RULE
- `crates/validate/tests/conformance_cases/conformance_places.rs:28` OLD-places (tracked separately)
- `crates/validate/tests/conformance_cases/conformance_places.rs:33` OLD-places (tracked separately)
- `crates/validate/tests/conformance_cases/conformance_places.rs:39` OLD-places (tracked separately)
- `crates/validate/tests/conformance_cases/conformance_places.rs:43` OLD-places (tracked separately)
- `crates/validate/tests/conformance_cases/conformance_places.rs:577` RULE
- `crates/validate/tests/conformance_cases/conformance_places.rs:579` RULE
- `crates/validate/tests/conformance_cases/conformance_places.rs:793` OLD-places (tracked separately)
- `crates/validate/tests/conformance_cases/conformance_places.rs:795` OLD-places (tracked separately)
- `crates/validate/tests/conformance_cases/conformance_places.rs:866` OLD-places (tracked separately)
- `crates/validate/tests/conformance_cases/conformance_places.rs:868` OLD-places (tracked separately)
- `crates/validate/tests/conformance_cases/conformance_places.rs:997` OLD-places (tracked separately)
- `crates/validate/tests/conformance_cases/conformance_places.rs:999` OLD-places (tracked separately)
- `crates/validate/tests/conformance_cases/conformance_places.rs:1072` OLD-places (tracked separately)
- `crates/validate/tests/conformance_cases/conformance_places.rs:1074` OLD-places (tracked separately)
- `crates/validate/tests/conformance_cases/conformance_standpoint.rs:57` RULE
- `crates/validate/tests/conformance_cases/ontology_conformance.rs:51` RULE
- `crates/validate/tests/conformance_cases/ontology_conformance.rs:63` RULE
- `crates/validate/tests/conformance_cases/ontology_conformance.rs:64` RULE
- `crates/validate/tests/conformance_cases/ontology_conformance.rs:67` RULE
- `crates/validate/tests/conformance_cases/ontology_conformance.rs:68` RULE
- `docs/APPLIED_CATEGORY_THEORY/usecase_openehr_taskplan_rchops21.md:123` GENERIC
- `docs/SLICE_GUIDE.md:423` GENERIC
- `docs/SLICE_GUIDE.md:424` GENERIC
- `docs/SLICE_GUIDE.md:629` GENERIC
- `docs/claim-lifecycle.md:6` RULE
- `docs/claim-lifecycle.md:7` RULE
- `docs/claim-lifecycle.md:25` RULE
- `docs/claim-lifecycle.md:27` RULE
- `docs/claim-lifecycle.md:31` RULE
- `docs/claim-lifecycle.md:33` RULE
- `docs/identity-mapping.md:41` RULE
- `docs/identity-mapping.md:140` RULE
- `docs/location-mapping.md:393` OLD-places (tracked separately)
- `docs/location-mapping.md:517` RULE
- `docs/mcp-server.md:61` GENERIC
- `docs/mcp-server.md:71` GENERIC
- `docs/mcp-server.md:80` GENERIC
- `docs/mcp-server.md:105` GENERIC
- `docs/mcp-server.md:135` GENERIC
- `docs/mentation-architecture.md:41` GENERIC
- `docs/mentation-architecture.md:142` GENERIC
- `docs/projections.md:110` RULE
- `docs/projections.md:237` RULE
- `docs/projects-mapping.md:24` RULE
- `docs/projects-mapping.md:134` RULE
- `docs/projects-mapping.md:209` RULE
- `docs/reasoning.md:98` RULE
- `docs/reasoning.md:133` RULE
- `docs/reasoning.md:137` RULE
- `docs/reasoning.md:138` RULE
- `docs/reasoning.md:139` RULE
- `docs/reasoning.md:164` RULE
- `docs/rights.md:71` OLD-expiry
- `docs/rights.md:173` OLD-expiry
- `docs/rights.md:174` OLD-expiry
- `docs/standpoints.md:202` RULE
- `docs/standpoints.md:203` RULE
- `dsl/mappings/projections/foaf.ttl:47` RULE
- `dsl/mappings/projections/foaf.ttl:146` RULE
- `dsl/mappings/projections/foaf.ttl:163` RULE
- `dsl/mappings/projections/mailmap.ttl:11` RULE
- `dsl/mappings/projections/schema-org-accessibility.ttl:12` NONE
- `dsl/mappings/projections/schema-org.ttl:255` RULE
- `dsl/mappings/projections/schema-org.ttl:622` NONE
- `dsl/mappings/projections/schema-org.ttl:638` NONE
- `dsl/mappings/projections/schema-org.ttl:669` NONE
- `dsl/mappings/projections/schema-org.ttl:699` NONE
- `dsl/mappings/projections/sosa.ttl:141` NONE
- `dsl/mappings/projections/vcard.ttl:63` RULE
- `dsl/mappings/projections/web-annotation.ttl:144` NONE
- `dsl/mappings/vocabulary.ttl:327` RULE
- `dsl/mappings/vocabulary.ttl:655` RULE
- `dsl/mappings/vocabulary.ttl:663` RULE
- `dsl/mappings/vocabulary.ttl:671` RULE
- `dsl/mappings/vocabulary.ttl:679` RULE
- `dsl/mappings/vocabulary.ttl:791` RULE
- `dsl/mappings/vocabulary.ttl:872` RULE
- `governance/constitution.ttl:439` RULE
- `metadata/gmeow-self.ttl:27` GENERIC
- `ontology/gmeow.ttl:13` GENERIC
- `slices/core/affect/design/AFFECT-DESIGN.md:380` GENERIC
- `slices/core/affect/design/AFFECT-DESIGN.md:384` GENERIC
- `slices/core/affect/design/AFFECT-DESIGN.md:385` GENERIC
- `slices/core/affect/design/AFFECT-DESIGN.md:955` GENERIC
- `slices/core/affect/module.ttl:251` GENERIC
- `slices/core/agency-records/i18n/fr.po:3612` NONE
- `slices/core/agency-records/i18n/zh.po:3612` NONE
- `slices/core/agency-records/module.ttl:1984` NONE
- `slices/core/agentic/examples/mcp-action-policy.ttl:37` GENERIC
- `slices/core/agentic/examples/mcp-action-policy.ttl:94` GENERIC
- `slices/core/agentic/examples/mcp-action-policy.ttl:108` GENERIC
- `slices/core/agentic/examples/mcp-action-policy.ttl:126` GENERIC
- `slices/core/agentic/examples/mcp-action-policy.ttl:138` GENERIC
- `slices/core/agentic/examples/mcp-action-policy.ttl:148` GENERIC
- `slices/core/agentic/examples/mcp-action-policy.ttl:154` GENERIC
- `slices/core/agentic/examples/mcp-action-policy.ttl:161` GENERIC
- `slices/core/agentic/examples/memory-triad-execute.ttl:28` GENERIC
- `slices/core/agentic/examples/memory-triad-execute.ttl:108` GENERIC
- `slices/core/agentic/examples/memory-triad-execute.ttl:116` GENERIC
- `slices/core/agentic/examples/trajectory-audit.ttl:81` GENERIC
- `slices/core/ai/docs.md:67` GENERIC
- `slices/core/ai/docs.md:224` GENERIC
- `slices/core/ai/examples/record-intake.ttl:70` RULE
- `slices/core/ai/module.ttl:128` GENERIC
- `slices/core/ai/module.ttl:132` GENERIC
- `slices/core/ai/module.ttl:134` GENERIC
- `slices/core/calendar/module.ttl:198` GENERIC
- `slices/core/calendar/module.ttl:763` OLD-status
- `slices/core/cognition/docs.md:91` OLD-ended-interval
- `slices/core/cognition/examples/knowledge-proficiency.ttl:8` OLD-ended-interval
- `slices/core/cognition/examples/knowledge-proficiency.ttl:54` OLD-ended-interval
- `slices/core/cognition/module.ttl:67` OLD-ended-interval
- `slices/core/cognition/module.ttl:226` OLD-ended-interval
- `slices/core/cognition/module.ttl:300` GENERIC
- `slices/core/cognition/module.ttl:305` GENERIC
- `slices/core/concepts/docs.md:8` GENERIC
- `slices/core/concepts/docs.md:40` GENERIC
- `slices/core/concepts/docs.md:52` GENERIC
- `slices/core/concepts/examples/conceptual-change.ttl:8` GENERIC
- `slices/core/concepts/examples/conceptual-change.ttl:28` GENERIC
- `slices/core/concepts/module.ttl:55` GENERIC
- `slices/core/concepts/module.ttl:59` GENERIC
- `slices/core/concepts/module.ttl:137` GENERIC
- `slices/core/coreference/docs.md:18` GENERIC
- `slices/core/coreference/docs.md:64` OLD-display-preference-as-data
- `slices/core/coreference/module.ttl:72` OLD-display-preference-as-data
- `slices/core/coreference/module.ttl:76` OLD-display-preference-as-data
- `slices/core/creative-works/docs.md:41` GENERIC
- `slices/core/creative-works/module.ttl:1528` GENERIC
- `slices/core/documents/docs.md:61` RULE
- `slices/core/documents/module.ttl:322` RULE
- `slices/core/documents/module.ttl:827` RULE
- `slices/core/documents/module.ttl:829` RULE
- `slices/core/epistemics/docs.md:176` RULE
- `slices/core/epistemics/docs.md:183` RULE
- `slices/core/epistemics/docs.md:184` RULE
- `slices/core/epistemics/docs.md:185` RULE
- `slices/core/epistemics/docs.md:255` RULE
- `slices/core/epistemics/docs.md:274` RULE
- `slices/core/epistemics/examples/belief-revision.ttl:4` RULE
- `slices/core/epistemics/examples/belief-revision.ttl:11` RULE
- `slices/core/epistemics/examples/belief-revision.ttl:50` RULE
- `slices/core/epistemics/examples/evaluating-a-reified-statement.ttl:19` RULE
- `slices/core/epistemics/examples/flagship-epistemic-ledger.ttl:10` RULE
- `slices/core/epistemics/i18n/fr.po:509` RULE
- `slices/core/epistemics/i18n/fr.po:513` RULE
- `slices/core/epistemics/i18n/fr.po:517` RULE
- `slices/core/epistemics/i18n/fr.po:521` RULE
- `slices/core/epistemics/i18n/fr.po:522` RULE
- `slices/core/epistemics/i18n/fr.po:525` RULE
- `slices/core/epistemics/i18n/fr.po:526` RULE
- `slices/core/epistemics/i18n/fr.po:529` RULE
- `slices/core/epistemics/i18n/fr.po:537` RULE
- `slices/core/epistemics/i18n/fr.po:544` RULE
- `slices/core/epistemics/i18n/fr.po:545` RULE
- `slices/core/epistemics/i18n/fr.po:546` RULE
- `slices/core/epistemics/i18n/fr.po:589` RULE
- `slices/core/epistemics/i18n/zh.po:509` RULE
- `slices/core/epistemics/i18n/zh.po:513` RULE
- `slices/core/epistemics/i18n/zh.po:517` RULE
- `slices/core/epistemics/i18n/zh.po:521` RULE
- `slices/core/epistemics/i18n/zh.po:522` RULE
- `slices/core/epistemics/i18n/zh.po:525` RULE
- `slices/core/epistemics/i18n/zh.po:526` RULE
- `slices/core/epistemics/i18n/zh.po:529` RULE
- `slices/core/epistemics/i18n/zh.po:537` RULE
- `slices/core/epistemics/i18n/zh.po:544` RULE
- `slices/core/epistemics/i18n/zh.po:545` RULE
- `slices/core/epistemics/i18n/zh.po:546` RULE
- `slices/core/epistemics/i18n/zh.po:589` RULE
- `slices/core/epistemics/module.ttl:43` RULE
- `slices/core/epistemics/module.ttl:45` RULE
- `slices/core/epistemics/module.ttl:54` RULE
- `slices/core/epistemics/module.ttl:59` RULE
- `slices/core/epistemics/module.ttl:136` RULE
- `slices/core/epistemics/module.ttl:140` RULE
- `slices/core/epistemics/module.ttl:434` RULE
- `slices/core/epistemics/module.ttl:516` RULE
- `slices/core/epistemics/module.ttl:551` RULE
- `slices/core/epistemics/module.ttl:555` RULE
- `slices/core/epistemics/module.ttl:1013` RULE
- `slices/core/epistemics/module.ttl:1015` RULE
- `slices/core/epistemics/module.ttl:1019` RULE
- `slices/core/epistemics/module.ttl:1021` RULE
- `slices/core/epistemics/module.ttl:1024` RULE
- `slices/core/epistemics/module.ttl:1025` RULE
- `slices/core/epistemics/module.ttl:1027` RULE
- `slices/core/epistemics/module.ttl:1039` RULE
- `slices/core/events/i18n/fr.po:709` RULE
- `slices/core/events/i18n/zh.po:709` RULE
- `slices/core/events/module.ttl:154` RULE
- `slices/core/events/tests/structural.ttl:39` RULE
- `slices/core/events/tests/structural.ttl:873` RULE
- `slices/core/gender/docs.md:22` RULE
- `slices/core/gender/docs.md:113` RULE
- `slices/core/gender/i18n/fr.po:73` RULE
- `slices/core/gender/i18n/fr.po:74` RULE
- `slices/core/gender/i18n/fr.po:76` RULE
- `slices/core/gender/i18n/fr.po:77` RULE
- `slices/core/gender/i18n/zh.po:73` RULE
- `slices/core/gender/i18n/zh.po:76` RULE
- `slices/core/gender/i18n/zh.po:77` RULE
- `slices/core/gender/manifest.ttl:12` RULE
- `slices/core/gender/module.ttl:74` RULE
- `slices/core/gender/module.ttl:98` RULE
- `slices/core/gender/module.ttl:104` RULE
- `slices/core/gender/module.ttl:282` RULE
- `slices/core/gender/module.ttl:293` RULE
- `slices/core/gender/module.ttl:300` RULE
- `slices/core/gender/module.ttl:381` RULE
- `slices/core/gender/module.ttl:425` RULE
- `slices/core/gender/module.ttl:427` RULE
- `slices/core/gender/module.ttl:430` RULE
- `slices/core/gender/module.ttl:431` RULE
- `slices/core/gender/module.ttl:433` RULE
- `slices/core/gender/module.ttl:537` RULE
- `slices/core/gender/tests/structural.ttl:264` RULE
- `slices/core/gender/tests/structural.ttl:275` RULE
- `slices/core/imagination/docs.md:77` GENERIC
- `slices/core/imagination/module.ttl:32` GENERIC
- `slices/core/inference/docs.md:139` OLD-defeat-or-evaluation
- `slices/core/inference/examples/belief-revision.ttl:4` OLD-defeat-or-evaluation
- `slices/core/inference/manifest.ttl:24` GENERIC
- `slices/core/inference/module.ttl:168` GENERIC
- `slices/core/inference/module.ttl:196` OLD-defeat-or-evaluation
- `slices/core/inference/module.ttl:677` OLD-defeat-or-evaluation
- `slices/core/inhabitation/design/INHABITED-RUNTIME-STACK.md:168` GENERIC
- `slices/core/inhabitation/design/INHABITED-RUNTIME-STACK.md:171` RULE
- `slices/core/inhabitation/design/INHABITED-TOPOLOGY.md:224` NONE
- `slices/core/inhabitation/i18n/fr.po:277` RULE
- `slices/core/inhabitation/i18n/zh.po:277` RULE
- `slices/core/inhabitation/module.ttl:830` RULE
- `slices/core/inhabitation/module.ttl:1092` RULE
- `slices/core/inquiry/module.ttl:108` GENERIC
- `slices/core/inquiry/module.ttl:110` GENERIC
- `slices/core/inquiry/module.ttl:111` GENERIC
- `slices/core/kernel/module.ttl:957` NONE
- `slices/core/kernel/module.ttl:1202` NONE
- `slices/core/learning/docs.md:73` GENERIC
- `slices/core/learning/docs.md:140` GENERIC
- `slices/core/learning/examples/skill-acquisition-trajectory.ttl:41` GENERIC
- `slices/core/learning/module.ttl:64` GENERIC
- `slices/core/learning/module.ttl:159` GENERIC
- `slices/core/learning/module.ttl:219` GENERIC
- `slices/core/learning/module.ttl:222` GENERIC
- `slices/core/lifecycle/docs.md:9` RULE
- `slices/core/lifecycle/i18n/fr.po:69` RULE
- `slices/core/lifecycle/i18n/fr.po:73` RULE
- `slices/core/lifecycle/i18n/fr.po:89` RULE
- `slices/core/lifecycle/i18n/fr.po:125` RULE
- `slices/core/lifecycle/i18n/fr.po:126` RULE
- `slices/core/lifecycle/i18n/fr.po:129` RULE
- `slices/core/lifecycle/i18n/fr.po:130` RULE
- `slices/core/lifecycle/i18n/fr.po:132` RULE
- `slices/core/lifecycle/i18n/fr.po:133` RULE
- `slices/core/lifecycle/i18n/fr.po:134` RULE
- `slices/core/lifecycle/i18n/zh.po:69` RULE
- `slices/core/lifecycle/i18n/zh.po:73` RULE
- `slices/core/lifecycle/i18n/zh.po:89` RULE
- `slices/core/lifecycle/i18n/zh.po:125` RULE
- `slices/core/lifecycle/i18n/zh.po:126` RULE
- `slices/core/lifecycle/i18n/zh.po:129` RULE
- `slices/core/lifecycle/i18n/zh.po:130` RULE
- `slices/core/lifecycle/i18n/zh.po:132` RULE
- `slices/core/lifecycle/i18n/zh.po:133` RULE
- `slices/core/lifecycle/i18n/zh.po:134` RULE
- `slices/core/lifecycle/module.ttl:97` RULE
- `slices/core/lifecycle/module.ttl:111` RULE
- `slices/core/lifecycle/module.ttl:113` RULE
- `slices/core/lifecycle/module.ttl:115` RULE
- `slices/core/lifecycle/module.ttl:121` RULE
- `slices/core/lifecycle/module.ttl:167` RULE
- `slices/core/lifecycle/module.ttl:172` RULE
- `slices/core/lifecycle/module.ttl:174` RULE
- `slices/core/lifecycle/module.ttl:177` RULE
- `slices/core/lifecycle/module.ttl:178` RULE
- `slices/core/lifecycle/module.ttl:180` RULE
- `slices/core/lifecycle/tests/structural.ttl:109` RULE
- `slices/core/lifecycle/tests/structural.ttl:116` RULE
- `slices/core/lifecycle/tests/structural.ttl:126` RULE
- `slices/core/memory-records/docs.md:25` NONE
- `slices/core/mentation/module.ttl:204` NONE
- `slices/core/metacognition/docs.md:124` RULE
- `slices/core/metacognition/docs.md:128` RULE
- `slices/core/metacognition/module.ttl:89` RULE
- `slices/core/metacognition/module.ttl:94` RULE
- `slices/core/metacognition/module.ttl:142` RULE
- `slices/core/names/docs.md:66` RULE
- `slices/core/names/docs.md:179` RULE
- `slices/core/names/docs.md:193` RULE
- `slices/core/names/docs.md:202` RULE
- `slices/core/names/docs.md:372` RULE
- `slices/core/names/examples/person-names.ttl:6` RULE
- `slices/core/names/examples/person-names.ttl:88` RULE
- `slices/core/names/examples/person-names.ttl:92` RULE
- `slices/core/names/i18n/fr.po:381` RULE
- `slices/core/names/i18n/fr.po:382` RULE
- `slices/core/names/i18n/fr.po:397` RULE
- `slices/core/names/i18n/fr.po:469` RULE
- `slices/core/names/i18n/fr.po:470` RULE
- `slices/core/names/i18n/fr.po:473` RULE
- `slices/core/names/i18n/fr.po:476` RULE
- `slices/core/names/i18n/fr.po:477` RULE
- `slices/core/names/i18n/fr.po:478` RULE
- `slices/core/names/i18n/fr.po:481` RULE
- `slices/core/names/i18n/fr.po:485` RULE
- `slices/core/names/i18n/zh.po:381` RULE
- `slices/core/names/i18n/zh.po:397` RULE
- `slices/core/names/i18n/zh.po:469` RULE
- `slices/core/names/i18n/zh.po:470` RULE
- `slices/core/names/i18n/zh.po:473` RULE
- `slices/core/names/i18n/zh.po:476` RULE
- `slices/core/names/i18n/zh.po:477` RULE
- `slices/core/names/i18n/zh.po:478` RULE
- `slices/core/names/i18n/zh.po:481` RULE
- `slices/core/names/i18n/zh.po:485` RULE
- `slices/core/names/manifest.ttl:12` RULE
- `slices/core/names/manifest.ttl:31` RULE
- `slices/core/names/mappings/equivalences.ttl:27` RULE
- `slices/core/names/module.ttl:114` RULE
- `slices/core/names/module.ttl:170` RULE
- `slices/core/names/module.ttl:185` RULE
- `slices/core/names/module.ttl:200` RULE
- `slices/core/names/module.ttl:219` RULE
- `slices/core/names/module.ttl:311` RULE
- `slices/core/names/module.ttl:316` RULE
- `slices/core/names/module.ttl:317` RULE
- `slices/core/names/module.ttl:399` RULE
- `slices/core/names/module.ttl:401` RULE
- `slices/core/names/module.ttl:413` RULE
- `slices/core/names/module.ttl:415` RULE
- `slices/core/names/module.ttl:958` RULE
- `slices/core/names/module.ttl:1546` RULE
- `slices/core/names/module.ttl:1606` RULE
- `slices/core/names/module.ttl:1608` RULE
- `slices/core/names/module.ttl:1622` RULE
- `slices/core/names/module.ttl:1627` RULE
- `slices/core/names/module.ttl:1629` RULE
- `slices/core/names/module.ttl:1632` RULE
- `slices/core/names/module.ttl:1633` RULE
- `slices/core/names/module.ttl:1635` RULE
- `slices/core/names/module.ttl:1935` RULE
- `slices/core/names/tests/structural.ttl:669` RULE
- `slices/core/names/tests/structural.ttl:676` RULE
- `slices/core/names/tests/structural.ttl:680` RULE
- `slices/core/names/tests/structural.ttl:683` RULE
- `slices/core/names/tests/structural.ttl:689` RULE
- `slices/core/names/tests/structural.ttl:697` RULE
- `slices/core/names/tests/structural.ttl:708` RULE
- `slices/core/names/tests/structural.ttl:713` RULE
- `slices/core/norms/docs.md:208` GENERIC
- `slices/core/norms/docs.md:305` GENERIC
- `slices/core/norms/module.ttl:216` GENERIC
- `slices/core/norms/module.ttl:218` GENERIC
- `slices/core/norms/module.ttl:229` GENERIC
- `slices/core/norms/module.ttl:231` GENERIC
- `slices/core/norms/module.ttl:1213` GENERIC
- `slices/core/notes/docs.md:28` GENERIC
- `slices/core/notes/module.ttl:302` OLD-cascade
- `slices/core/notes/module.ttl:312` OLD-cascade
- `slices/core/notes/module.ttl:313` OLD-cascade
- `slices/core/notes/tests/structural.ttl:38` GENERIC
- `slices/core/observations/tests/structural.ttl:475` RULE
- `slices/core/organization/docs.md:87` GENERIC
- `slices/core/organization/module.ttl:758` GENERIC
- `slices/core/organization/tests/structural.ttl:19` GENERIC
- `slices/core/persona-context/i18n/fr.po:169` NONE
- `slices/core/persona-context/i18n/zh.po:169` NONE
- `slices/core/persona-context/module.ttl:93` NONE
- `slices/core/places/docs.md:22` OLD-places (tracked separately)
- `slices/core/places/module.ttl:69` OLD-places (tracked separately)
- `slices/core/places/module.ttl:147` OLD-places (tracked separately)
- `slices/core/places/module.ttl:168` OLD-places (tracked separately)
- `slices/core/places/module.ttl:171` OLD-places (tracked separately)
- `slices/core/places/module.ttl:305` OLD-places (tracked separately)
- `slices/core/places/module.ttl:3271` OLD-places (tracked separately)
- `slices/core/places/module.ttl:3273` OLD-places (tracked separately)
- `slices/core/places/module.ttl:5339` NONE
- `slices/core/places/module.ttl:5351` NONE
- `slices/core/places/module.ttl:5417` OLD-places (tracked separately)
- `slices/core/places/module.ttl:5422` OLD-places (tracked separately)
- `slices/core/places/module.ttl:5424` OLD-places (tracked separately)
- `slices/core/places/module.ttl:5427` OLD-places (tracked separately)
- `slices/core/places/module.ttl:5428` OLD-places (tracked separately)
- `slices/core/places/module.ttl:5430` OLD-places (tracked separately)
- `slices/core/places/module.ttl:5461` NONE
- `slices/core/places/tests/structural.ttl:38` OLD-places (tracked separately)
- `slices/core/record-metadata/i18n/fr.po:703` RULE
- `slices/core/record-metadata/i18n/zh.po:703` RULE
- `slices/core/record-metadata/module.ttl:477` RULE
- `slices/core/record-metadata/module.ttl:606` NONE
- `slices/core/refinement-cues/i18n/fr.po:669` NONE
- `slices/core/refinement-cues/i18n/fr.po:713` NONE
- `slices/core/refinement-cues/i18n/zh.po:669` NONE
- `slices/core/refinement-cues/i18n/zh.po:713` NONE
- `slices/core/refinement-cues/module.ttl:263` NONE
- `slices/core/refinement-cues/module.ttl:300` NONE
- `slices/core/refinement-cues/module.ttl:821` NONE
- `slices/core/refinement-cues/module.ttl:836` NONE
- `slices/core/refinement-cues/module.ttl:851` NONE
- `slices/core/refinement-cues/module.ttl:866` NONE
- `slices/core/refinement-cues/module.ttl:881` NONE
- `slices/core/refinement-cues/module.ttl:896` NONE
- `slices/core/refinement-cues/module.ttl:911` NONE
- `slices/core/refinement-cues/module.ttl:926` NONE
- `slices/core/refinement-cues/module.ttl:941` NONE
- `slices/core/rights/docs.md:219` OLD-expiry
- `slices/core/rights/manifest.ttl:12` OLD-expiry
- `slices/core/rights/module.ttl:2026` OLD-expiry
- `slices/core/rights/module.ttl:2097` OLD-expiry
- `slices/core/rights/module.ttl:2099` OLD-expiry
- `slices/core/rights/module.ttl:2101` OLD-expiry
- `slices/core/rights/module.ttl:2104` OLD-expiry
- `slices/core/rights/module.ttl:2107` OLD-expiry
- `slices/core/rights/module.ttl:2405` OLD-expiry
- `slices/core/sexuality/docs.md:24` RULE
- `slices/core/sexuality/docs.md:40` RULE
- `slices/core/sexuality/module.ttl:23` RULE
- `slices/core/sexuality/module.ttl:54` RULE
- `slices/core/sexuality/module.ttl:76` RULE
- `slices/core/sexuality/module.ttl:83` RULE
- `slices/core/sexuality/module.ttl:97` RULE
- `slices/core/sexuality/module.ttl:104` RULE
- `slices/core/sexuality/module.ttl:253` RULE
- `slices/core/standpoint/docs.md:76` RULE
- `slices/core/standpoint/docs.md:120` RULE
- `slices/core/standpoint/docs.md:132` RULE
- `slices/core/standpoint/docs.md:147` RULE
- `slices/core/standpoint/docs.md:158` RULE
- `slices/core/standpoint/docs.md:159` RULE
- `slices/core/standpoint/docs.md:165` RULE
- `slices/core/standpoint/examples/correction-and-disagreement.ttl:19` RULE
- `slices/core/standpoint/i18n/fr.po:340` RULE
- `slices/core/standpoint/i18n/fr.po:344` RULE
- `slices/core/standpoint/i18n/fr.po:345` RULE
- `slices/core/standpoint/i18n/fr.po:353` RULE
- `slices/core/standpoint/i18n/fr.po:372` RULE
- `slices/core/standpoint/i18n/fr.po:412` RULE
- `slices/core/standpoint/i18n/fr.po:420` RULE
- `slices/core/standpoint/i18n/fr.po:424` RULE
- `slices/core/standpoint/i18n/fr.po:444` RULE
- `slices/core/standpoint/i18n/fr.po:445` RULE
- `slices/core/standpoint/i18n/fr.po:496` RULE
- `slices/core/standpoint/i18n/fr.po:497` RULE
- `slices/core/standpoint/i18n/fr.po:580` RULE
- `slices/core/standpoint/i18n/fr.po:581` RULE
- `slices/core/standpoint/i18n/fr.po:583` RULE
- `slices/core/standpoint/i18n/fr.po:584` RULE
- `slices/core/standpoint/i18n/fr.po:585` RULE
- `slices/core/standpoint/i18n/fr.po:596` RULE
- `slices/core/standpoint/i18n/fr.po:608` RULE
- `slices/core/standpoint/i18n/fr.po:612` RULE
- `slices/core/standpoint/i18n/zh.po:340` RULE
- `slices/core/standpoint/i18n/zh.po:344` RULE
- `slices/core/standpoint/i18n/zh.po:345` RULE
- `slices/core/standpoint/i18n/zh.po:372` RULE
- `slices/core/standpoint/i18n/zh.po:412` RULE
- `slices/core/standpoint/i18n/zh.po:420` RULE
- `slices/core/standpoint/i18n/zh.po:424` RULE
- `slices/core/standpoint/i18n/zh.po:444` RULE
- `slices/core/standpoint/i18n/zh.po:496` RULE
- `slices/core/standpoint/i18n/zh.po:497` RULE
- `slices/core/standpoint/i18n/zh.po:580` RULE
- `slices/core/standpoint/i18n/zh.po:581` RULE
- `slices/core/standpoint/i18n/zh.po:583` RULE
- `slices/core/standpoint/i18n/zh.po:584` RULE
- `slices/core/standpoint/i18n/zh.po:585` RULE
- `slices/core/standpoint/i18n/zh.po:596` RULE
- `slices/core/standpoint/i18n/zh.po:608` RULE
- `slices/core/standpoint/i18n/zh.po:612` RULE
- `slices/core/standpoint/module.ttl:74` RULE
- `slices/core/standpoint/module.ttl:199` RULE
- `slices/core/standpoint/module.ttl:486` RULE
- `slices/core/standpoint/module.ttl:520` RULE
- `slices/core/standpoint/module.ttl:757` RULE
- `slices/core/standpoint/module.ttl:822` RULE
- `slices/core/standpoint/module.ttl:954` RULE
- `slices/core/standpoint/module.ttl:956` RULE
- `slices/core/standpoint/module.ttl:959` RULE
- `slices/core/standpoint/module.ttl:960` RULE
- `slices/core/standpoint/module.ttl:962` RULE
- `slices/core/standpoint/module.ttl:975` RULE
- `slices/core/standpoint/module.ttl:980` RULE
- `slices/core/standpoint/module.ttl:1286` RULE
- `slices/core/standpoint/module.ttl:1587` RULE
- `slices/core/standpoint/module.ttl:1588` RULE
- `slices/core/standpoint/module.ttl:1602` RULE
- `slices/core/standpoint/module.ttl:1604` RULE
- `slices/core/standpoint/module.ttl:1609` RULE
- `slices/core/standpoint/tests/competency.ttl:37` RULE
- `slices/core/standpoint/tests/structural.ttl:510` RULE
- `slices/core/standpoint/tests/structural.ttl:565` RULE
- `slices/core/standpoint/tests/structural.ttl:571` RULE
- `slices/core/tags/docs.md:16` GENERIC
- `slices/core/tags/docs.md:43` GENERIC
- `slices/core/tags/examples/contested-tagging.ttl:14` GENERIC
- `slices/core/tags/examples/contested-tagging.ttl:15` GENERIC
- `slices/core/tags/examples/contested-tagging.ttl:72` GENERIC
- `slices/core/tags/i18n/fr.po:163` GENERIC
- `slices/core/tags/i18n/zh.po:163` GENERIC
- `slices/core/tags/module.ttl:67` GENERIC
- `slices/core/tags/module.ttl:186` GENERIC
- `slices/core/tags/module.ttl:254` OLD-ended-interval
- `slices/core/tags/module.ttl:256` OLD-ended-interval
- `slices/core/tags/module.ttl:260` OLD-ended-interval
- `slices/core/tags/module.ttl:262` OLD-ended-interval
- `slices/core/tags/module.ttl:265` OLD-ended-interval
- `slices/core/tags/module.ttl:266` OLD-ended-interval
- `slices/core/tags/module.ttl:268` OLD-ended-interval
- `slices/core/tags/module.ttl:280` OLD-ended-interval
- `slices/core/tags/queries/competency/current-tags.rq:2` GENERIC
- `slices/core/tags/tests/competency.ttl:28` GENERIC
- `slices/core/tags/tests/competency.ttl:35` GENERIC
- `slices/core/teleology/docs.md:49` GENERIC
- `slices/core/teleology/docs.md:103` GENERIC
- `slices/core/teleology/i18n/fr.po:241` GENERIC
- `slices/core/teleology/i18n/zh.po:241` GENERIC
- `slices/core/teleology/module.ttl:71` GENERIC
- `slices/core/teleology/module.ttl:75` GENERIC
- `slices/core/teleology/module.ttl:255` GENERIC
- `slices/core/versions/module.ttl:33` NONE
- `slices/core/work-orchestration/examples/weekly-adr-review.ttl:269` NONE
- `slices/core/work-orchestration/i18n/fr.po:310` NONE
- `slices/core/work-orchestration/i18n/zh.po:310` NONE
- `slices/core/work-orchestration/module.ttl:590` NONE
- `slices/extensions/accessibility/module.ttl:191` OLD-ended-interval
- `slices/extensions/accessibility/module.ttl:196` OLD-ended-interval
- `slices/extensions/accessibility/module.ttl:198` OLD-ended-interval
- `slices/extensions/accessibility/module.ttl:201` OLD-ended-interval
- `slices/extensions/accessibility/module.ttl:204` OLD-ended-interval
- `slices/extensions/aggregation/docs.md:77` NONE
- `slices/extensions/aggregation/module.ttl:110` NONE
- `slices/extensions/dreaming/docs.md:31` RULE
- `slices/extensions/dreaming/docs.md:100` RULE
- `slices/extensions/dreaming/i18n/fr.po:1032` RULE
- `slices/extensions/dreaming/i18n/fr.po:1064` RULE
- `slices/extensions/dreaming/i18n/fr.po:1068` RULE
- `slices/extensions/dreaming/i18n/zh.po:1032` RULE
- `slices/extensions/dreaming/i18n/zh.po:1064` RULE
- `slices/extensions/dreaming/i18n/zh.po:1068` RULE
- `slices/extensions/dreaming/module.ttl:15` RULE
- `slices/extensions/dreaming/module.ttl:29` RULE
- `slices/extensions/dreaming/module.ttl:57` RULE
- `slices/extensions/email/module.ttl:1377` OLD-ended-interval
- `slices/extensions/email/module.ttl:1379` OLD-ended-interval
- `slices/extensions/email/module.ttl:1382` OLD-ended-interval
- `slices/extensions/email/module.ttl:1385` OLD-ended-interval
- `slices/extensions/embedding-projection/tests/competency.ttl:489` NONE
- `slices/extensions/employment/docs.md:24` GENERIC
- `slices/extensions/employment/module.ttl:194` GENERIC
- `slices/extensions/finance/docs.md:31` OLD-status
- `slices/extensions/finance/module.ttl:972` OLD-status
- `slices/extensions/finance/module.ttl:990` OLD-status
- `slices/extensions/finance/module.ttl:1017` OLD-status
- `slices/extensions/finance/module.ttl:1044` OLD-status
- `slices/extensions/genealogy/docs.md:17` RULE
- `slices/extensions/genealogy/i18n/fr.po:73` RULE
- `slices/extensions/genealogy/i18n/zh.po:73` RULE
- `slices/extensions/genealogy/module.ttl:231` RULE
- `slices/extensions/images/docs.md:17` GENERIC
- `slices/extensions/images/module.ttl:592` GENERIC
- `slices/extensions/languages/docs.md:202` GENERIC
- `slices/extensions/lexicon/docs.md:70` GENERIC
- `slices/extensions/lexicon/docs.md:180` GENERIC
- `slices/extensions/lexicon/module.ttl:45` GENERIC
- `slices/extensions/lexicon/module.ttl:47` GENERIC
- `slices/extensions/narrative/docs.md:19` GENERIC
- `slices/extensions/narrative/module.ttl:455` GENERIC
- `slices/extensions/narrative/module.ttl:499` GENERIC
- `slices/extensions/narrative/module.ttl:960` GENERIC
- `slices/extensions/sensory-environment/docs.md:86` GENERIC
- `slices/extensions/sensory-environment/module.ttl:69` GENERIC
- `slices/extensions/software/docs.md:98` RULE
- `slices/extensions/software/i18n/fr.po:461` RULE
- `slices/extensions/software/i18n/fr.po:462` RULE
- `slices/extensions/software/i18n/zh.po:461` RULE
- `slices/extensions/software/module.ttl:1362` RULE
- `slices/grounding/lang/module.ttl:10813` NONE
- `slices/grounding/logic/design/LOGIC-COGNITION.md:174` RULE
- `slices/grounding/logic/design/LOGIC-TELEOLOGY.md:234` GENERIC
- `slices/grounding/logic/design/LOGIC.md:136` NONE
- `slices/grounding/logic/i18n/fr.po:8710` NONE
- `slices/grounding/logic/i18n/fr.po:9642` NONE
- `slices/grounding/logic/i18n/fr.po:13174` NONE
- `slices/grounding/logic/i18n/fr.po:13398` NONE
- `slices/grounding/logic/i18n/fr.po:13402` NONE
- `slices/grounding/logic/i18n/fr.po:16906` NONE
- `slices/grounding/logic/i18n/zh.po:8710` NONE
- `slices/grounding/logic/i18n/zh.po:9642` NONE
- `slices/grounding/logic/i18n/zh.po:13174` NONE
- `slices/grounding/logic/i18n/zh.po:13398` NONE
- `slices/grounding/logic/i18n/zh.po:13402` NONE
- `slices/grounding/logic/i18n/zh.po:16906` NONE
- `slices/grounding/logic/module.ttl:849` NONE
- `slices/grounding/logic/module.ttl:3595` NONE
- `slices/grounding/logic/module.ttl:9187` NONE
- `slices/grounding/logic/module.ttl:9200` NONE
- `slices/grounding/logic/module.ttl:9213` NONE
- `slices/grounding/logic/module.ttl:9214` NONE
- `slices/grounding/logic/module.ttl:9218` NONE
- `slices/grounding/logic/tests/competency.ttl:1467` NONE
- `slices/grounding/math/design/MATHEMATICS-EXTERNAL-CORPUS-CROSSWALK.md:115` GENERIC
- `slices/grounding/math/design/MATHEMATICS-EXTERNAL-CORPUS-CROSSWALK.md:131` NONE
- `slices/grounding/math/design/MATHEMATICS-EXTERNAL-CORPUS-CROSSWALK.md:140` NONE
- `tests/fixtures/coverage/coordinate-observations.ttl:3` OLD-places (tracked separately)
- `tests/fixtures/coverage/coordinate-observations.ttl:49` OLD-places (tracked separately)
- `tests/fixtures/coverage/employment-contested.ttl:67` GENERIC
- `tests/fixtures/coverage/epistemics-belief-revision.ttl:4` RULE
- `tests/fixtures/coverage/epistemics-belief-revision.ttl:11` RULE
- `tests/fixtures/coverage/epistemics-belief-revision.ttl:50` RULE
- `tests/fixtures/coverage/events.ttl:80` RULE
- `tests/fixtures/coverage/genealogy-contested.ttl:58` RULE
- `tests/fixtures/coverage/identity.ttl:5` RULE
- `tests/fixtures/coverage/identity.ttl:63` RULE
- `tests/fixtures/coverage/logo.ttl:18` GENERIC
- `tests/fixtures/coverage/names.ttl:6` RULE
- `tests/fixtures/coverage/names.ttl:107` RULE
- `tests/fixtures/coverage/names.ttl:293` RULE
- `tests/fixtures/coverage/names.ttl:307` RULE
- `tests/fixtures/coverage/organization-contested.ttl:62` RULE
- `tests/fixtures/coverage/places-cadastral.ttl:90` OLD-places (tracked separately)
- `tests/fixtures/coverage/places-capacity.ttl:3` OLD-places (tracked separately)
- `tests/fixtures/coverage/places-capacity.ttl:42` OLD-places (tracked separately)
- `tests/fixtures/coverage/places-contested.ttl:82` RULE
- `tests/fixtures/coverage/places-virtual.ttl:45` OLD-places (tracked separately)
- `tests/fixtures/coverage/standpoint.ttl:50` RULE
- `tests/fixtures/coverage/tags.ttl:57` GENERIC
- `tests/fixtures/shapes/deadname-unsuppressed-warning.ttl:2` RULE
- `tests/fixtures/shapes/observation-ended-no-warning.ttl:2` RULE
- `tests/fixtures/shapes/relator-malformed.ttl:22` RULE
- `tests/fixtures/shapes/relator-wellformed.ttl:4` RULE
- `tests/fixtures/shapes/relator-wellformed.ttl:49` RULE
- `tests/fixtures/shapes/rights-expired-warning.ttl:3` OLD-expiry
- `tests/fixtures/shapes/standpoint-ended-tenure-conforms.ttl:3` RULE
- `tests/fixtures/shapes/standpoint-other-party-retraction-conforms.ttl:4` RULE
- `tests/fixtures/shapes/standpoint-withdrawn-warning.ttl:3` RULE
- `tests/fixtures/shapes/standpoint-withdrawn-warning.ttl:4` RULE
- `tests/fixtures/shapes/suppression-warning-only.ttl:3` RULE
