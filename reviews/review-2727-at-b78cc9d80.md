BLOCKED at b78cc9d80

# Delta review of issue 2727 at b78cc9d80 (range bcb94a541..b78cc9d80)

## Head and tree

- `git rev-parse HEAD` printed `b78cc9d809d2907dd1a60c3101135d957b80815c`. `git status --porcelain`
  printed nothing, so the tree is clean. `origin/paudley/2727-displayable-is-not-scoped-to-a` is at
  the same commit.
- The delta is one commit, `b78cc9d80`, whose parent is `bcb94a541`. It changes 19 files (41 lines
  added, 23 removed). The merge base with `origin/main` is still `e32f90c96`.
- I changed nothing in the worktree. I wrote only this file.

## What I read and searched, and what I did not

**Read:**
- **The issue.** `stagectl issue 2727`: the body and all 4 comments, including the decision
  (comment 2), the scope list by slice (comment 3) and the later rulings (comment 4).
- **The earlier reviews.** `review-2727-at-bcb94a541.md` in full. In `review-2727-at-cd0a85620.md`,
  the status of F1-F7 and the R1-R3 headings.
- **The delta.** `git diff bcb94a541..HEAD`, every hunk, and the commit message.
- **The four named constraints at head:**
  - lifecycle `module.ttl:165-195`;
  - names `module.ttl:1621-1655`;
  - gender `module.ttl:420-443`;
  - standpoint `module.ttl:947-990`.
- **The merge-base forms**, compared at `e32f90c96`:
  - the lifecycle guard (any `gmeow:Entity`);
  - the deleted observations constraint;
  - the deleted events participation constraint.
- **Edited text, read in context:**
  - `slices/core/standpoint/module.ttl:70-82` and `:822`;
  - `slices/core/standpoint/docs.md:115-125` and `:196-205`;
  - `docs/standpoints.md:55-68`;
  - `docs/claim-lifecycle.md:1-40`;
  - `slices/extensions/genealogy/module.ttl:231` and `docs.md:14-20`;
  - the four new catalog entries;
  - `tests/fixtures/coverage/standpoint.ttl:40-56`, `events.ttl:70-86` and
    `genealogy-contested.ttl:1-72`;
  - `conformance_events.rs:10-22` and `:315-337`.
- **The six edited projection mappings**, each read whole (pattern and binding):
  `mapSosaSensoryResult`, `mapSchemaPlaceName`, `mapSchemaMaximumAttendeeCapacity`,
  `mapSchemaOccupancy`, `mapSchemaCoordinateObservation` and `mapIcalAttendeeReified`.
- **The sweep material.** `.stage/issue-2727/sweep.md` (structure, totals and the rows cited below),
  `classify.pl` in full and `run6.sh`.
- **The evidence.** Every `r6-*.log` in full.

**Searched.** All searches were mine and independent of the sweep. I used `git grep` over tracked
files, excluding `generated/`, `imports/`, `*.po`, `*.json`, `*.snap` and `*.wasm` unless named:
1. A lifecycle word (supersed, withdrawn, closed, ended, expired, validUntil, correct, disput,
   contest, retract, outdated, obsolete, replaced) within 120 characters of `displayable`,
   `suppress`, `hidden` or `withheld`, in either order. The tree has 425 such lines; I read every
   hit in `README.md`, `docs/`, `dsl/`, `tests/fixtures/`, and the lifecycle, observations, events,
   standpoint, genealogy, names and organization slices (208 lines).
2. `supersed` with a suppression word on the same line or the next line, over `*.md`, `*.ttl`,
   `*.rs` and `*.rq`, outside the excluded slices.
3. `observation` with a suppression word, outside places.
4. Participation, attendee or event with a suppression word and a lifecycle word.
5. `tenure` with a suppression word, outside places and epistemics.
6. The literal `opened-then-closed`, and "withdrawn … claim (".
7. The shape and constraint names: the removed observation and participation constraints, and the
   `…SuppressionShape` names.
8. The `crates/` the sweep script never tables (`pipeline`, `mcp`, `logic`, `logic-compile`,
   `gmeow-cli`, `gmeow-dev-cli`, `build-inputs`) for a lifecycle word near suppression.
9. `#` comments in `*.rq`.
10. Every `…SuppressionConstraint` declared in any slice module.

**Commands I ran:**
- **The msgid check.** An `awk` extraction of the two ontology-header literals, compared by SHA-256
  with each new fr and zh `msgid` (after unescaping `\n` and `\"`).
- **The golden check.** `jq` set differences of the reviewed-coverage golden at `bcb94a541` against
  head.

**Not read:**
- The rest of each module and catalog outside the ranges above.
- `msgid-check.pl` and `gen-msgid.pl`.
- `rust-work/SKILL.md`.
- The purrdf SHACL engine.
- The earlier delivery logs.
- The sweep inputs `h1.txt` and `h4.txt`. Because of that, I do not know the exact patterns that
  produced the 2489 hits.

## D1 and D2

**D1: fixed and true.**
- **The new bullet.** `slices/core/standpoint/module.ttl:822` reads: "a claim its holder withdrew,
  or a tenure whose held claim its holder retracted, sets gmeow:displayable false; a tenure that
  merely closed takes no flag; nothing is deleted."
- **Agreement with the rest of the slice.** It agrees with:
  - `StandpointTenure` (`:72`, `:74`);
  - `docs/standpoints.md:61-66` and `:202-204`;
  - the constraint at `:954-990` (interval ended, plus a `retractsClaim` by an activity associated
    with the claim's own `vantage`).
- **What the header leaves out.** It omits the "interval ended" conjunct, which is acceptable for a
  doctrine bullet.
- **The catalog entries.** The fr and zh entries exist, keyed
  `…/slices/standpoint|…rdf-schema#comment`. Their `msgid` hashes equal the module literal's hash
  (`9dff77c3…`) for both languages.
- **French.** "Masquage pour la révision ordinaire … reçoit gmeow:displayable false ; une tenure
  simplement close ne reçoit aucun indicateur ; rien n'est supprimé". "Masquage" is used for
  hiding and "supprimé" for deletion, as required.
- **Chinese.** "日常修订的隐藏 … 设置 gmeow:displayable false；仅仅结束的任期不带标志；任何内容都不会被删除"
  is faithful.

**D2: fixed and true.** `tests/fixtures/coverage/events.ttl:79-80` now reads "A participation its
recorder withdrew as mistaken: the recorder's own retraction, so it is suppressed … retained, never
deleted". Correction is no longer described as suppression, and `validUntil` is no longer given as
the reason. The data is unchanged. See O2 for a fixture-narrative note.

## The other changed sentences

All are true at head unless noted.

- **Genealogy header.** `slices/extensions/genealogy/module.ttl:231` now reads: "A parentage claim
  its own holder withdrew (a StandpointTenure whose held claim its holder retracted, or a claim its
  holder superseded) sets gmeow:displayable false …; a tenure that merely closed takes no flag, and
  a claim another party disputes or corrects takes none either."
  - It is true, and it removes the old "opened-then-closed StandpointTenure" equation.
  - The fr and zh `msgid` hashes equal the module literal's hash (`64eb2c8b…`).
  - The fr text uses "jamais une suppression" for deletion and "masquée" for hiding, which is
    correct. Both translations add a short clause; see O3.
- **Genealogy docs.** `docs.md:17-19` is the same rule in prose. True.
- **Standpoint coverage fixture** (`standpoint.ttl:48-50`). "A closed interval alone would not
  prompt the flag" is true of the constraint. On the claimed withdrawal, see O2.
- **Standpoint docs.md:117-120.** "suppression is how a record its holder withdrew leaves display".
  True.
- **`docs/claim-lifecycle.md:6-8`.** "suppresses a record its holder withdrew or superseded". True
  under the comment-4 ruling that only a claim's own holder supersedes it.
- **The projection loss notes.** Every edited mapping carries
  `gmeow:suppressWhen [ …displayable … false ]` on the subject named in the note, and nothing that
  keys on supersession or `validUntil`. So each new note ("… flagged gmeow:displayable false are
  suppressed/dropped") is exact:
  - sosa `mapSosaSensoryResult` (`obs`);
  - schema-org `mapSchemaPlaceName` (`pn`);
  - capacity (`cap`);
  - occupancy (`occ`);
  - coordinate observation (`obs`; it also suppresses under `coarsenTo`, which the note never
    covered);
  - ical `mapIcalAttendeeReified` (`p`).
- **The Rust comment** (`ontology_conformance.rs:51`). It names
  `gmeow:SupersededGenderIdentitySuppressionConstraint`, which exists at gender `module.ttl:425`.
  True. This resolves the earlier O1.
- **The trailing "yet"** at `conformance_standpoint.rs:65` is removed. This resolves the earlier O2.
- **The stale path** at `docs/standpoints.md:8` is now `slices/core/standpoint/module.ttl`. This
  resolves the earlier O6.
- **The genealogy fixture** (`genealogy-contested.ttl:58`). "withdrawn by its own standpoint, so
  suppressed" matches the data: `accordingTo ex:standpoint-parish`, `refuted`, flagged. This
  resolves the earlier O4.

## The sweep test

**What the sweep is.**
- **The table.** It has 888 rows: 577 TRUE AS WRITTEN, 11 CORRECTED (plus 5 more in the
  "edits that are not pattern hits" table) and 295 OUT OF SCOPE.
- **The appendix.** It lists 1601 hits.
- **How rows are classified.** `classify.pl` classifies largely by path and keyword, not by reading:
  - Every row under `slices/core/{gender,sexuality}/`, `AGENTS.md`, `docs/reasoning.md`,
    `docs/RATIONALE.md` and `docs/identity-mapping.md` is TRUE, as "the retained global rule".
  - Every path matching `INHABITED` or `lifecycle/docs` is TRUE, as "ontic ending is stated to
    imply no suppression".
  - Every row under `tests/`, `crates/`, `dsl/` or `conformance/` that no earlier rule takes is
    TRUE, as "test or fixture over data that carries the flag".
  - Every row in a fixed list of "other" slices is OUT OF SCOPE unless a keyword matches.
  - Every hit under `crates/{pipeline,mcp,logic,…}` goes to the appendix whatever its context.
- **Result.** Those rules produced both misses below.

**(a) My independent searches against the table.** Every hit of mine that states a removed or
narrowed rule:
- is CORRECTED in the table; or
- is rightly in the maintainer bucket (another slice's own doctrine, which no named constraint
  prompts); or
- is one of the two misses in the REQUIRED findings below.

Further results:
- The search of the `crates/` the script never tables found one hit, `mcp/src/storage.rs:1179`. It
  is the agent's own revise operation, so not a miss.
- `.rq` comments: none.
- References to the removed constraint names: none outside the comments already corrected.

**(b) TRUE AS WRITTEN spot-check: 62 rows read in context.**
- **Root and docs:**
  - `README.md:41-42, 58, 113, 768, 793`;
  - `docs/SLICE_GUIDE.md:423`;
  - `docs/claim-lifecycle.md:35`;
  - `docs/identity-mapping.md:43`;
  - `docs/reasoning.md:99, 139`;
  - `dsl/mappings/projections/mailmap.ttl:11`.
- **Conformance tests:**
  - `conformance_email.rs:414`;
  - `conformance_employment.rs:72`;
  - `conformance_epistemics.rs:95, 561`;
  - `conformance_identity_over_history.rs:200`;
  - `ontology_conformance.rs:68`;
  - `conformance_events.rs:14, 322, 323, 335`.
- **Core slices, part 1:**
  - affect `AFFECT-DESIGN.md:37`;
  - agentic `mcp-action-policy.ttl:138`, `memory-triad-execute.ttl:108`;
  - ai `module.ttl:128`;
  - cognition `docs.md:91`, `module.ttl:307`;
  - concepts `module.ttl:55`;
  - coreference `module.ttl:74`;
  - epistemics `docs.md:183, 274`, `evaluating-a-reified-statement.ttl:19`, `module.ttl:45, 516,
    1019, 1044`;
  - gender `docs.md:115`, `module.ttl:282, 441`.
- **Core slices, part 2:**
  - inference `docs.md:139`;
  - inhabitation `INHABITED-IDENTITY.md:214`, `INHABITED-TOPOLOGY.md:224`, `INHABITED.md:122`;
  - inquiry `module.ttl:111`;
  - learning `module.ttl:222`;
  - lifecycle `tests/structural.ttl:142`;
  - metacognition `module.ttl:142`;
  - names `docs.md:304`, `module.ttl:1701`;
  - norms `module.ttl:218, 229`;
  - notes `docs.md:29`;
  - organization `docs.md:87`, `module.ttl:758`.
- **Core slices, part 3:**
  - record-metadata `module.ttl:477`;
  - refinement-readings `docs.md:155`;
  - sexuality `module.ttl:76`;
  - standpoint `module.ttl:951`, `tests/structural.ttl:552`;
  - tags `module.ttl:67`, `tests/competency.ttl:35`;
  - teleology `module.ttl:71, 73, 75`.
- **Extensions:**
  - dreaming `module.ttl:15, 57`;
  - employment `docs.md:24`;
  - finance `module.ttl:1017`.
- **Other:**
  - `LOGIC-COGNITION.md:174`;
  - fixtures `employment-contested.ttl:67`, `genealogy-contested.ttl:4`,
    `participation-wellformed.ttl:23`.

**What the check found:**
- **In scope and wrongly TRUE: 1 row,** `conformance_events.rs:322`. This is REQUIRED finding R-A.
- **Wrong class or reason, but in the maintainer bucket and not this branch's scope: 3 rows.**
  - `INHABITED-IDENTITY.md:214`. The stated reason is the opposite of the sentence.
  - `concepts/module.ttl:55`. The row is TRUE, while its neighbours `:59` and `:137` (the same
    doctrine) are OUT OF SCOPE.
  - `finance/module.ttl:1017`. It is finance's own cancelled-invoice constraint, labelled a
    holder's own withdrawal.
  - See O1.
- **The rest: true** as read in context.
- **Organization and README.** `organization/module.ttl:758` ("Recognition granted and later
  withdrawn is modelled as a gmeow:StandpointTenure with gmeow:displayable false") and
  `README.md:793` read as the holder's own withdrawal, which agrees with the narrowed constraint.

**(c) Appendix spot-check: 23 hits read in context. None is a miss.**
- `README.md:93, 118, 720, 777, 946`;
- `docs/identity-mapping.md:82, 139, 160`;
- `docs/projections.md:68, 105, 237`;
- `docs/projects-mapping.md:121, 137`;
- `docs/mcp-server.md:57`;
- `docs/identity-and-disclosure.md:46, 118`;
- `conformance_events.rs:78`;
- `conformance_epistemics.rs:535`;
- `conformance_gender.rs:55`;
- `conformance_suppression.rs:7, 57`;
- `mcp/src/storage.rs:1182`;
- `mcp/src/lib.rs:4908`.

**(d) OUT OF SCOPE rows (295), grouped by stated reason:**

| Rows | Stated reason | Verdict |
|---:|---|---|
| 11 | CONSTITUTION | excluded |
| 60 | places | excluded |
| 13 | accessibility | excluded |
| 12 | email | excluded |
| 20 | rights, expired trademark | another slice's own constraint, so the maintainer bucket |
| 171 | "the slice's own unchanged doctrine" | see below |
| 1 | mentation | maintainer bucket |
| 7 | "build or timing text" | not an exclusion, but none concerns display of an ended thing, so harmless |

The 171 rows in the "slice's own unchanged doctrine" bucket:
- Most are other-slice doctrine, so the maintainer bucket.
- Some are not "a slice's own doctrine" at all: `docs/SLICE_GUIDE.md` (6), `docs/mcp-server.md`
  (8) and kernel `module.ttl:1202, 1231`. I read all 16, and none states a removed rule, so the
  class is harmless.
- **Really in scope:** `slices/extensions/dreaming/module.ttl:29` and
  `slices/extensions/dreaming/docs.md:100`. Comment 3 puts the dreaming slice in this issue, and
  the branch already rewrote the identical phrase in the same slice. This is REQUIRED finding R-B.
- **Wrong class, harmless:** `attestation/docs.md:223` and `software/docs.md:98`. Both state the
  retained global rule for a superseded label, so they are true, not out of scope.

## REQUIRED findings

**R-A. A participation test docstring still equates "superseded" with the flag:
`crates/validate/tests/conformance_cases/conformance_events.rs:322`.**
- **The text.** "A superseded participation (`gmeow:displayable false`) is NOT projected — the flat
  downcast honours suppression-not-erasure (Principle 10)."
- **Why it blocks.**
  - This is the participation rule the branch deleted (the events
    `SupersededParticipationSuppressionConstraint`: a participation with `validUntil` should be
    flagged). At head, a superseded participation takes no flag and is projected.
  - The delta corrected the identical wording in `ical.ttl:114-115`, with the sweep's own reason
    "comment said a superseded (displayable false) participation; now names the flag only". It
    also corrected this test's fixture (`events.ttl:79-80`, D2).
  - The sweep marked the line TRUE AS WRITTEN only through the blanket `crates/` rule.
- **Smallest correction (comment only):** "A participation flagged `gmeow:displayable false` (its
  recorder's own withdrawal) is NOT projected — …".

**R-B. The DreamReport definition and its doc still say "superseded reports are suppressed":
`slices/extensions/dreaming/module.ttl:29` and `slices/extensions/dreaming/docs.md:100`.**
- **The text.** "A standpoint-indexed, low-metacognitive-reliability claim: competing reports
  coexist and superseded reports are suppressed, not deleted."
- **Why it blocks.**
  - Comment 3 lists the dreaming slice as scope of this issue.
  - The branch replaced this exact phrase at `docs.md:31` ("superseded reports are suppressed with
    `gmeow:displayable false`" became "a report its own author withdrew or replaced is suppressed
    … A report that another party disputes or corrects takes no display flag"). It also replaced
    the header gloss "A superseded report (for example, a corrected or withdrawn dream-journal
    entry)".
  - It left the class definition and its doc mirror unchanged. Those sit next to "competing
    reports", which invites the reading that the branch removed: an analyst's competing report
    supersedes, and so suppresses, the dreamer's.
  - The slice now says two different things about the same act. The sweep put both lines in OUT OF
    SCOPE as other-slice doctrine, which this slice is not.
- **Smallest correction.** Reword both to: "competing reports coexist; a report its own author
  withdrew or replaced is suppressed, not deleted, and a report another party disputes or corrects
  takes no display flag."
  - The catalogs have no entry today for `…/DreamReport|…skos/core#definition` (grep finds only
    the `rdfs:comment` key at `fr.po:1063`).
  - Under comment 4 ("Every changed English literal gets its French and Chinese catalog entry"),
    the change adds fr and zh entries with a byte-equal `msgid` and refreshes the reviewed-coverage
    golden for the two new keys.

## OPTIONAL findings

- **O1. The sweep's classifier reads paths, not sentences.** The rows to fix if the table is kept as
  a record:
  - `INHABITED-IDENTITY.md:214`: the reason "ontic ending is stated to imply no suppression"
    contradicts the sentence ("a higher authority `gmeow:supersedes` the determination (suppressed,
    never deleted …)").
  - `concepts/module.ttl:55`: TRUE, against its OUT OF SCOPE neighbours.
  - `finance/module.ttl:1017`: the reason does not fit.
  - The blanket `crates/` TRUE rule, which hid R-A.
  - The forced appendix for seven crates.
- **O2. Two fixtures narrate a withdrawal that their data does not record.**
  - **Where.** `tests/fixtures/coverage/standpoint.ttl:48-56` says the holder withdrew, but has no
    `gmeow:standpointClaim`, `retractsClaim` or `wasAssociatedWith`. `events.ttl:79-86` says the
    recorder retracted, with no retraction activity.
  - **Why it does not block.** The flag on them is legitimate; the constraint only warns when the
    flag is missing.
  - **Fix.** Record the retraction, or say "flagged as withdrawn".
- **O3. The new genealogy translations add a clause the English lacks:** fr "et est donc masquée",
  zh "即对读者隐藏". The meaning is preserved; drop the clause for strict parity.
- **O4. The earlier O3 ("held claim" names two properties) persists in the new header text.** It
  appears in standpoint `module.ttl:822` and genealogy `module.ttl:231`. The constraint means
  `gmeow:standpointClaim`, while `StandpointTenure` `howToUse` binds "the held claim with
  gmeow:tenurePosition".

## Golden and evidence (items 4 and 5)

**The reviewed-coverage golden.**
- **The jq difference** between `bcb94a541` and head:
  - no top-level catalog was added or removed;
  - exactly four lists changed, each gaining one key and losing none;
  - `slices/core/standpoint/i18n/{fr,zh}.po` gained `…/slices/standpoint|…rdf-schema#comment`;
  - `slices/extensions/genealogy/i18n/{fr,zh}.po` gained `…/slices/genealogy|…rdf-schema#comment`.
- **Nothing else moved.** Every list is still sorted, and the scalar count went from 64321 to 64325.
  `r6-golden.log` reports "190 catalogs (64325 reviewed entries)".

**No behaviour change.**
- Every changed line under `tests/` and `crates/` is a `#` or `//` comment.
- The module changes are inside two `rdfs:comment` literals.
- The `dsl/` changes are three `lossyDrop` literals and one comment.
- No constraint, formula, guard or data triple changed.

**The evidence logs.**
- **Header and exit lines.** Each `r6-*.log` records the working directory, head, status,
  `COMMAND:`, `BEFORE:` and `AFTER:` (wrapper modification time and SHA-256), and `EXIT=`.
- **Wrapper checksums.**
  - Within each run, BEFORE equals AFTER.
  - The wrapper changed between runs: `r6-golden` saw `dd74e5ad…` (modified 19:48:12), and every
    later run saw `857ea5b9…` (modified 20:39:04). Each run is self-consistent.
- **Which head each log names.**
  - `r6-golden` and `r6-commit` name `bcb94a541` with the dirty or staged list, as expected before
    the commit. The commit line in `r6-commit` names `b78cc9d80`, and every hook passed.
  - `r6-cargo`, `r6-lint`, `r6-msgid`, `r6-prose`, `r6-prose2` and `r6-rapper` name `b78cc9d80`
    with a clean status.
- **Inner exit codes.**
  - `r6-msgid` and `r6-prose*` wrap `echo …-rc=$?`, so `EXIT=` is the echo's status. The inner
    codes printed are `msgid-rc=0` and `prose-rc=0`.
  - `r6-rapper` prints `rapper rc=0` for each of its 8 files.
- **Scope of the cargo run.** `r6-cargo` is `cargo check -p gmeow-validate --tests`. It compiles the
  edited test file, but it is compile-only.

## For the maintainer

**What this list is.** Doctrine in OTHER slices that still says a closed, ended, cancelled or
superseded thing is suppressed (or prompts the flag), with no named constraint prompting it. This
excludes places, accessibility, email and the constitution, which are already excluded.

**Own warning constraints on an ended or cancelled record** (the pattern ruled out for
observations):
- tags: `RetractedTaggingSuppressionConstraint`, `slices/core/tags/module.ttl:255-268`, fires on an
  ended `taggingInterval`. Also `tests/fixtures/coverage/tags.ttl:57`.
- calendar: `CancelledOccurrenceSuppressionConstraint`, `slices/core/calendar/module.ttl:758-763`.
- finance: `ReversedTransactionSuppressionConstraint` (`:985`), `CancelledInvoiceSuppressionConstraint`
  (`:1012-1017`) and `CancelledOrderSuppressionConstraint` (`:1039`), all in
  `slices/extensions/finance/module.ttl`.
- rights: `ExpiredTrademarkSuppressionConstraint`, `slices/core/rights/module.ttl:2099`; also
  `README.md:827`.

**Closed tenure means suppressed** (the "StandpointTenure idiom" the standpoint slice has now
narrowed):
- concepts: `slices/core/concepts/module.ttl:55, 59, 137`, `docs.md:40` and
  `examples/conceptual-change.ttl:8`.
- inquiry: `slices/core/inquiry/module.ttl:108, 110, 111, 113, 163`. Line 108 calls itself "The
  StandpointTenure idiom applied to inquiry".
- teleology: `slices/core/teleology/module.ttl:32, 71, 73, 75`. Line 71 says "The StandpointTenure
  idiom applied to teleology".
- norms: `slices/core/norms/module.ttl:229` ("The StandpointTenure idiom … a withdrawn precedence is
  kept with gmeow:displayable false") and `:218`.
- learning: `slices/core/learning/module.ttl:77, 159, 222`, `docs.md:110` and
  `examples/skill-acquisition-trajectory.ttl:40`.
- cognition: `slices/core/cognition/docs.md:91` ("Lapsed knowledge is a closed … interval and/or
  displayable false").
- inference: `slices/core/inference/module.ttl:196, 677` and `docs.md:143`. This is the reasoner
  revising its own conclusion, which is the closest to self-revision.

**Superseded X means suppressed** (the lifecycle constraint used to prompt this for every entity
and now prompts it only for an `Appellation`):
- inhabitation: `slices/core/inhabitation/design/INHABITED-IDENTITY.md:213-214`, "a higher authority
  `gmeow:supersedes` the determination (suppressed, never deleted …)". This is a supersession by
  another party, which comment 4 says is "that party's claim about the target". It is the strongest
  candidate.
- imagination: `slices/core/imagination/docs.md:77` ("a superseded attribution is suppressed").
- lexicon: `slices/extensions/lexicon/docs.md:70, 180` and `module.ttl:47`.
- languages: `slices/extensions/languages/docs.md:202`.
- images: `slices/extensions/images/docs.md:17`, `module.ttl:592`, and the fixture
  `tests/fixtures/coverage/logo.ttl:18`.
- narrative: `slices/extensions/narrative/docs.md:19` and `module.ttl:960` (retcons use
  `supersedes` with `displayable false`).
- sensory-environment: `slices/extensions/sensory-environment/docs.md:86`.
- documents and creative-works: `slices/core/documents/docs.md:61`, `documents/module.ttl:322` and
  `slices/core/creative-works/docs.md:41` (titles are label-like, so possibly global safety).
- coreference: `slices/core/coreference/module.ttl:74`, a conditional ("if it should no longer
  surface").
- affect: `slices/core/affect/module.ttl:251` and `design/AFFECT-DESIGN.md:384`.
- mentation: `docs/mentation-architecture.md:142` ("outdated or withdrawn mental content is marked
  `displayable false`").
- software: `slices/extensions/software/docs.md:98`, `dsl/mappings/projections/mailmap.ttl:11` and
  `docs/projects-mapping.md:134`. A superseded author identity is a label, so this probably falls
  under the retained global rule.

**"Withdrawn" without saying by whom:**
- employment: `slices/extensions/employment/module.ttl:194` and `docs.md:24`.
- organization: `slices/core/organization/docs.md:87` and `module.ttl:758`.

## Verdict scope

- **What the verdict covers.** The whole branch. It rests on:
  - the review at `cd0a85620` (F1-F7 status: F1-F4 and F6 resolved; F5 and F7 carried as R1 and
    R3);
  - the review at `bcb94a541` (R1-R3 resolved except R1's residue D1 and D2, plus O1-O7);
  - my reading of this delta.
- **Earlier findings.** D1 and D2 are closed. Of the earlier OPTIONAL items, O1, O2, O4, O5 and O6
  are done; O3 persists as O4 here. O7 is addressed: the post-commit logs name the head, and cargo
  now checks the tests.
- **What blocks.** R-A and R-B belong to the R1/D1/D2 class: remaining sentences in this branch's
  scope that state the removed rule. The sweep missed them.

## Not shown (unexecuted)

- **No gate or test run at `b78cc9d80`, by me or by the coder:** no `make check`, producer run,
  `make produce-test-fixtures`, nextest, conformance runner, reasoning or slice-quality sweep.
- **Not executed:** the events projection test, the standpoint and gender suppression cases, and
  `corpus_reviewed_coverage_matches_frozen_golden`.
- **The coder's checks at head:** `cargo check` (compile only), `make lint`, the msgid script,
  `prose-check` on 4 docs and 5 code/fixture files, and `rapper` on 8 files. The golden refresh and
  the hooks ran on the pre-commit tree.
- **My own checks:** `git`, `jq`, `awk`/`grep`/`sed` text extraction, `sha256sum`, and
  `stagectl issue`. I ran `prose-check` on this review only (exit 0); I did not run `rapper` or `prose-check` on any repository file.
- **Where behaviour claims come from.** Every statement about constraint or projection behaviour
  here comes from reading formulas and mapping patterns, not from execution.
