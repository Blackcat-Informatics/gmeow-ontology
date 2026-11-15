# Review of PR 2804 (paudley/2725-correction-is-not-suppression)

- Head reviewed: `ed29648111aac75580f6625881b5b6997ef78224` (`git rev-parse HEAD` in the worktree, and `headRefOid` on the PR, both checked)
- Base: `331c554f87153e25c95919c933ecb6aa9f44d20b` (`git merge-base origin/main HEAD` after `git fetch origin main`)
- Commits in range: one, `ed2964811`
- Started: Sat 14 Nov 2026 07:40:24 PM CST. Written: Sat 14 Nov 2026 08:05:13 PM CST (`date`)
- Mode: read only. I ran no make, cargo, test or producer command. I ran read-only git and gh, grep, `prose-check`, and two inline `python3 -I` scripts. The scripts parsed the catalogs and the module text in memory and wrote no files.
- CI at review time: producer-build, helpers, lint and rust-prebuild passed. generation (a), generation (b) and fixture-prefix were still pending. CodeRabbit reported neutral.

## Scope

The requirement is the last maintainer comment on issue 2725 and the last maintainer comment on issue 2727. I read both issues in full, bodies and comments. Per the caller, this PR covers the standpoint slice and the cross-slice documents that stated the old reading. The parts of 2727 that belong to other slices are separate work.

## Coverage

| File | Lines read | Commit |
|---|---|---|
| issue 2725, issue 2727 (body and every comment) | all | gh, at review time |
| PR 2804 body | all | gh, at review time |
| commit message | all | ed2964811 |
| full `git diff 331c554f8..HEAD`, all 9 files | all hunks | ed2964811 |
| slices/core/standpoint/module.ttl | 945-974, 1160-1508; 191-200 through the diff | ed2964811 |
| slices/core/standpoint/module.ttl | NOT read: 1-190, 201-944, 975-1159 | — |
| slices/core/standpoint/manifest.ttl | all | ed2964811 |
| slices/core/standpoint/examples/claim-lifecycle.ttl, correction-and-disagreement.ttl | all | ed2964811 |
| slices/core/standpoint/examples/contested-authorship.ttl, claim-bullshit.yamlld | NOT read | — |
| slices/core/standpoint/tests/competency.ttl | all | ed2964811 |
| slices/core/standpoint/tests/structural.ttl | 1-40, 480-557 (NOT read: 41-479) | ed2964811 |
| slices/core/standpoint/docs.md | 40-170 (NOT read: the rest) | ed2964811 |
| docs/claim-lifecycle.md | all | ed2964811 |
| slices/core/standpoint/i18n/fr.po | every changed entry, read in full; all msgids checked by script | ed2964811 |
| slices/core/standpoint/i18n/zh.po | every new and old msgstr; all msgids checked by script | ed2964811 |
| CONSTITUTION.md | 35-224 (Principles 2-10), 315-419 (15-17) | ed2964811 |
| crates/logic-compile/src/projections/shapes.rs | 1380-1619 (`constraint_atom`, `lower_positive`, `lower_negative`) | ed2964811 |
| crates/logic-compile/src/ir/constraint.rs | function signatures only (grep). It holds the IR containers, not the formula lowering | ed2964811 |
| crates/logic-compile/src/frontend/formula_reader.rs | NOT read | — |
| crates/slicetest/src/repository.rs | 300-389, 455-540, 590-650, 1100-1129 | ed2964811 |
| crates/slicetest/src/dsl.rs, exec.rs | grep hits only (reasoning lanes, cqDataFile lane rule, module-scope projection) | ed2964811 |
| crates/pipeline/src/stages/result_shapes.rs | 95-175 | ed2964811 |
| purrdf-slice 4.0.0 (Cargo.lock version) src/ownership.rs | 150-185, 636-660, 740-756 | registry |
| crates/validate/src/slice_ownership.rs 1-37, slice_peerage.rs 1-60, validate_all.rs 1745-1772 | as listed | ed2964811 |
| crates/docs/tests/po_parser_parity.rs | 1-66 | ed2964811 |
| crates/lang-bridge/tests/gmn1_roundtrip.rs | 55-75 | ed2964811 |
| slices/core/events/module.ttl 1567, 1612-1650; events/docs.md 22; attestation/docs.md 218-223; extensions/dreaming/module.ttl 15; epistemics/module.ttl 30-52, 44, 58, 515; gender/module.ttl 537; names/module.ttl 1935; software/module.ttl 1362 (grep) | as listed | ed2964811 |
| tests/fixtures/coverage/events.ttl 76-86, coverage/genealogy-contested.ttl 55-66, shapes/participation-wellformed.ttl 20-30 | as listed | ed2964811 |
| slices/grounding/logic/module.ttl | the declarations of `logic:Endurant`, `logic:Event`, `logic:Perdurant` | ed2964811 |

## Findings

### F1 REQUIRED: the revised closure formula looks for the correcting standpoint on the wrong node

Location: `slices/core/standpoint/module.ttl:1261-1266`, `1276-1281` and `1302-1307` (the formula). The text that claims otherwise is at `module.ttl:198`, `1226`, `1227`, `1229` and `1232`, at `docs.md:72-76` and `88-91`, and in the PR body.

The new conjunct is `∃c. correctsClaim(c, X) ∧ accordingTo(c, ranker)`. It reads `gmeow:accordingTo` on `c`, the correcting entity, which is the subject of the correction. The slice's own guidance puts the correcting standpoint somewhere else: on the correction statement's annotation, that is, on the statement's reifier.

- `correctsClaim` howToUse (`module.ttl:1486`): "Annotate the relation with accordingTo for the standpoint that makes the correction".
- `docs.md:93-97`, for rankings in particular: the lifecycle relations are used "on the reifier of the ranking statement … each annotated with the same ranker and an effective time".
- The worked lifecycle example (`examples/claim-lifecycle.ttl:18-22`) has `ex:correctUsage` with no `accordingTo` on the node at all. The snippet at `docs.md:157-159` shows the standpoint only on the annotation.

The new example happens to state `accordingTo` in both places, so it hides the gap. No instance data anywhere in the repository uses `moreEntrenchedThan`, so nothing exercises this formula.

What breaks:
1. A ranker R corrects its own ranking exactly as the slice guide says: `ex:fix gmeow:correctsClaim ex:r1 {| gmeow:accordingTo R ; gmeow:validFrom … |}`, with `ex:fix` carrying no `accordingTo`. The closure check does not count the correction. If `ex:r1` is a premise, R still owes a live a-over-c claim, and the check reports a Violation. Before this PR any correction counted, so this is a new false Violation on data written to the guide. If `ex:r1` is the concluding claim, it still counts as live and satisfies the obligation, so a chain whose closure R has itself rejected passes without comment.
2. Another party X records `ex:r2 gmeow:correctsClaim ex:r1 {| gmeow:accordingTo X |}`, where `ex:r1` and `ex:r2` are both R's ranking reifiers. `ex:r2` carries `accordingTo R`, so the check counts X's correction as R's own, and `ex:r1` stops being live in R's closure. One party's correction still changes what another ranker is held to. That is the effect the PR body says is fixed ("It now counts only the ranker's own correction") and the effect the 2725 ruling forbids.

The commit message describes the formula accurately ("names the same ranker"). The usage notes do not. They say "a claim the ranker has itself corrected" (`module.ttl:198`, `docs.md:108`, `docs.md:113`) and "corrected by its own ranker" (`module.ttl:1226`, `1229`, `1232`, `docs.md:75`), and the formula does not express that.

Fix: read the attribution from the correction statement. The formula already uses this construction for the ranking edges:
`∃c, l. reifiedStatementSubject(l, c) ∧ reifiedStatementPredicate(l, gmeow:correctsClaim) ∧ reifiedStatementObject(l, X) ∧ accordingTo(l, ranker)`.
Make the matching edits to the definition, avoidWhen, howToUse, message, `docs.md:72-76` and `docs.md:88-91`, and add the fr and zh catalog entries for every changed literal. `lower_negative` lowers this form the same way as the current one, as a single `FILTER NOT EXISTS { … }` over four patterns. A fixture pair would turn the corrected rule into executed behaviour; see F4.

### F2 OPTIONAL: the competency question cannot detect a derived supersession

Location: `tests/competency.ttl:14-18` and `222-224`.

The question runs in `reasoningNone`. `crates/slicetest/src/exec.rs:342` honours `cqDataFile` only in that lane, so the question cannot use reasoning. Its `FILTER NOT EXISTS { … supersedes … }` therefore sees asserted triples only. Had the old sub-property axiom stayed, the question would still return both rows. The comment says "superseded by the other would be filtered out", and that holds only for an asserted supersession. The real guard for acceptance criterion 1 is the structural `correction-is-not-supersession` mustNot (`structural.ttl:499-510`). It runs on the module projected to OWL and RDFS, so it catches the axiom in either spelling. The question still has value as the acceptance-3 demonstration that both claims can be retrieved. Its comment should say it guards against asserted flags only. It also overlaps `structural.ttl:539-557`.

### F3 OPTIONAL: the new docs settle a 2727 question before 2727 does

Location: `docs/claim-lifecycle.md:33-34`, `docs.md:142-144`.

Both say "`gmeow:displayable false` is a global safety control: … and a standpoint's own closed tenure or withdrawn claim". Principle 10 supports the text: it lists "a withdrawn standpoint" as a suppression trigger and says a "closed `gmeow:StandpointTenure` is suppressed". The 2727 ruling, however, still requires each of the four warning constraints, `WithdrawnStandpointSuppressionConstraint` among them, to be examined and narrowed to global-safety cases. For the observation constraint, the ruling says it must "no longer fire merely because a validity interval has ended". `WithdrawnStandpointSuppressionConstraint` (`module.ttl:951-973`) fires on any tenure whose interval has ended, which is the same structure. Keeping it unchanged is defensible under Principle 10, and it never fires because of a dispute or a correction. Writing the classification into two documents still pre-empts the 2727 examination. Either record this judgment on issue 2727 so the maintainer sees it, or soften the sentence until 2727 lands.

### F4 OPTIONAL: no fixture exercises the closure liveness rule, before or after this PR

The closure constraint has no positive or negative fixture anywhere: no examples, no tests/fixtures, nothing in `crates/pipeline/src/stages/constraint_shapes.tests.rs`. The PR states this itself ("Not shown"). A two-case fixture in the slice would make the fix for F1 executed behaviour instead of a reading. One case is a self-correction annotated as the guide says, which must lapse the obligation. The other is a cross-party correction, which must leave the premise live.

### F5 OPTIONAL: the historical-order text contradicts itself

Location: `module.ttl:198`, `docs.md:110-116`.

"A historical order at t answers what the ranker held at t" sits next to "It leaves out a claim the ranker has itself corrected at every t". Before the correction took effect, the ranker did hold that claim. This tension existed before the PR, but the PR rewrote the sentence. Under the 2725 ruling's "does not erase the evidence of blue", a self-correction leaving the historical order for instants before it took effect is at least arguable. One fix: say the historical order at t omits a self-corrected claim only from the correction's effective instant on, as the text already does for retraction. The other: say explicitly that this order reflects the ranker's current view of t.

### F6 OPTIONAL: ownership of the remaining old-reading text

The events slice still says "Corrections use gmeow:displayable false" (`events/module.ttl:1567`, `events/docs.md:22`). The events constraint `SupersededParticipationSuppressionConstraint` (`events/module.ttl:1620-1645`), `attestation/docs.md:221`, `dreaming/module.ttl:15` and three test-fixture comments carry the same reading. The PR body says these "belong with #2727", but the 2727 text does not list any of them. It names the lifecycle, observations, standpoint and names constraints and epistemics lines 44 and 58. Add a comment on 2727 that lists them so the work has a visible owner. None of them uses `correctsClaim`, and none fails after this PR.

### F7 OPTIONAL: correction between claim tokens implies a sort mismatch

`correctsClaim` has `logic:domain` and `logic:range` `gmeow:Entity`, which is a subclass of `logic:Endurant`. The new example applies it between `gmeow:ClaimToken`s, which are subclasses of `gmeow:Activity`, then `gmeow:Event`, then `logic:Event`. Domain and range therefore make each claim token an inferred `gmeow:Entity`. The logic slice states in prose that every Event is a Perdurant, and `logic:Endurant` is `logic:disjointWith logic:Perdurant`. Because the Event-to-Perdurant link is prose only, no axiom makes the example inconsistent. Domain and range are open-world, and there is no closure entry for `correctsClaim`, so no SHACL is derived from them either. This is a modelling smell for the maintainer, not a failing gate. A producer pass would show whether any bridge projection flags it.

## Answers

### 1. Changed axioms, constraints and text in module.ttl

- `correctsClaim` (`1479-1491`): the `logic:subPropertyOf gmeow:supersedes` line is gone. Domain and range stay `gmeow:Entity`, and the property has no characteristics. The definition, howToUse, useWhen, avoidWhen and scopeNote match the ruling: the corrected claim is retained unchanged, there is no supersession, no suppression and no prompt to set display, the reader chooses at projection, and plain disagreement is a `ClaimEvaluation`. The new useWhen no longer copies the definition, which the old one did. The authoring is correct: no `owl:`, no shapes, and the axioms are EL-safe.
- Slice scopeNote (`1507`): matches the ruling.
- `moreEntrenchedThan` definition and howToUse (`194`, `198`): match the ruling in intent, but the "itself corrected" wording does not match the formula (F1).
- `moreEntrenchedThanRankerClosureConstraint` (`1222-1311`): still a `logic:Constraint` with a `logic:integrity` formula, `logic:formalizes` and `logic:message`, so the form is right. Its semantics are wrong (F1).
- `WithdrawnStandpointSuppressionConstraint`, `succeedsClaim`, `retractsClaim` and the order constraint are unchanged; see 4.

### 2. The closure formula, before and after

- Before: claim X is live iff ¬∃s.succeedsClaim(s,X) ∧ ¬∃c.correctsClaim(c,X) ∧ ¬∃w.retractsClaim(w,X). The constraint read: for a Standpoint `this` and reifiers `first` (this→middle) and `second` (middle→lower) that share `accordingTo ranker`, both live, there exists `third` (this→lower) with `accordingTo ranker` that is live. Any correction by anyone ended liveness.
- After: the correction conjunct becomes ¬∃c.(correctsClaim(c,X) ∧ accordingTo(c,ranker)) for each of first, second and third. `ranker` is bound in the outer antecedent, from the ranking reifiers. `c` is fresh in each negated existential.
- What it now says: a correction ends liveness iff the correcting entity is itself attributed to the ranker. It does not say "only the ranker's own correction counts", because it never reads who made the correction statement (F1).
- Where `accordingTo` is read: from the subject node of `correctsClaim`. The examples and the slice guide put it on the annotation reifier of the `correctsClaim` statement. The formula reads the correct node only when the corrector is itself a ranking reifier by the same ranker, which is the pattern the order constraint's howToUse describes.
- Vacuity: none. Premises become live in more cases (more obligations), and `third` becomes live in more cases (obligations easier to meet). The change is neither vacuous nor uniformly stricter.
- Existing examples and fixtures: none contains `moreEntrenchedThan` instance data, so no example or fixture becomes violated. The only Rust reference, `constraint_shapes.tests.rs:128`, concerns irreflexivity.
- Compiler: `shapes.rs:1454-1601`. The top-level ∀(A→C) is lowered for violation as `lower_negative(Forall)`, then `lower_negative(Implies)`, giving `lower_positive(A)` plus `lower_negative(C)`. In A, `¬∃c.(P∧Q)` becomes `lower_negative(Exists)`, which is `FILTER NOT EXISTS { ?c <correctsClaim> ?first . ?c <accordingTo> ?ranker . }`, because `lower_positive(And)` concatenates the two triple patterns. In C, `∃third.(… ∧ ¬∃c.(P∧Q))` under `lower_negative` becomes `FILTER NOT EXISTS { …third patterns… FILTER NOT EXISTS { ?c … ?third . ?c <accordingTo> ?ranker . } }`, correlated on the bound `?ranker`. No `BIND` is involved, so no reordering happens. The order constraint already places `logic:and` directly under `logic:exists`. I expect the formula to be admitted and lowered this way. I did not read `formula_reader.rs` or the admission checks, so whether the frontend accepts it is inferred, not verified.

### 3. Completeness

Search: `git grep` over the whole tree for `correctsClaim`, for correction words within 120 characters of supersede, suppress, displayable or hide, and for the old literals.

- Changed correctly by this PR: the standpoint module, docs.md, the lifecycle example, structural.ttl, the catalogs and `docs/claim-lifecycle.md`. Exception: the closure-formula semantics (F1).
- No hits in: mappings (no PROV or SKOS mapping of `correctsClaim`), reasoner rules, Rust tests or snapshots that assert the sub-property, or generated-view expectations. `crates/lang-bridge/tests/gmn1_roundtrip.rs:64` only round-trips the annotation and does not depend on the axiom. `crates/docs/tests/fixtures/reviewed_coverage_golden.json` is keyed by term and predicate; the PR changed only msgid and msgstr lines, so its key set is unchanged.
- Gender, names and software: their slice scopeNote still says "correction rejects the earlier claim only in the correcting standpoint". That statement remains true under the ruling, and it implies no supersession or suppression. Leaving it is correct, and it is not false after merge.
- Epistemics 44 and 58: say to supersede or retract one's own evaluation or token by suppression. That is a display consequence, listed in 2727. Correctly left; not made false by this PR.
- Events, attestation, dreaming and test fixtures: say "corrections use displayable false" in the sense of a source's own superseded record. None uses `correctsClaim`. They conflict with the 2727 ruling, and they did before this PR, which neither falsifies them further nor breaks a test. Correctly left as other-slice work, but see F6 on ownership.
- Missed: none in this PR's scope, apart from the F1 text.

### 4. The liveness judgments

- `succeedsClaim` and `retractsClaim` unchanged: consistent with the ruling, which changes only `correctsClaim`. Principles 9 and 10 do not forbid a standpoint changing or withdrawing its own position.
- `WithdrawnStandpointSuppressionConstraint` unchanged: it reads only an ended tenure, never a dispute. It is supported by Principle 10's text, in tension with the 2727 ruling's interval rule (F3).
- The order constraint unchanged: acceptable. A correction link between two claims of one ranker only exempts the pair from a Violation. It hides, ranks and suppresses nothing. The docs sentence "A correction is read only within the ranker's own claims" is literally true, but the order check does not check who made the link either.
- The author's residual: yes, one party can change what is live for another. An X-made `succeedsClaim` or `retractsClaim` on R's ranking reifier ends its liveness and exempts the pair. Because `succeedsClaim` is a sub-property of `supersedes`, it also derives `supersededBy`, which fires the lifecycle warning to set `displayable false` on R's claim. That is cross-party suppression routed through succession. The PR leaves this unchanged and documents it. The 2725 ruling covers corrections, so this is not a contract breach here. It is the 2727 lifecycle-constraint work and should be named there. For corrections, the PR improves on the old state where the correcting node is the corrector's own claim, but it does not close the gap (F1).

### 5. Examples and tests, evaluated by hand

- `distinct-lifecycle-relations` (`structural.ttl:487-497`, module scope, projected to OWL and RDFS): all three conjuncts hold. Passes.
- `correction-is-not-supersession` (mustNot, `499-510`): no `subPropertyOf` edge leaves `correctsClaim` in either spelling, so the ASK is false and the check passes. On the base commit the direct edge would make it true, so this is a real regression guard. Limit: module scope cannot see a chain through another slice. That is negligible.
- `lifecycle-examples` (`512-532`): `ex:mistakenUsage` now carries no flag, and no asserted `supersedes` exists. Every positive pattern is present in `claim-lifecycle.ttl`. ASK is true; passes.
- `disagreement-and-correction-retain-the-claim` (`539-557`): every positive pattern is present in `correction-and-disagreement.ttl`, including the annotation reifier with `inspectorView` and `2026-04-02T09:00:00Z`. `blueClaim` has no `displayable`, there is no `supersedes` or `supersededBy`, and `neighbourDoubt` has no `correctsClaim`. True; passes.
- Competency question: the asserted merged graph has no other `correctsClaim` triple (the module example is a string), so exactly one correction binding exists. The reifier carries `accordingTo case:inspectorView`. Both claims are typed `ClaimToken` directly, and the FILTER selects the two of them. Rows: (blueClaim, doorIsBlue) and (redClaim, doorIsRed), both IRIs. That matches `cqExactRows`, the expected rows and the ResultShape. Test value: F2.
- The example shows what acceptance 3 asks: the corrected claim and the correction are both retrievable, the correction is indexed to `inspectorView` with an effective time, and the disagreement is a `ClaimEvaluation` that evaluates and expresses.
- Harness discovery: `repository.rs:1100-1129` discovers every `slices/**/tests/competency.ttl`, and `result_shapes.rs:99-131` harvests it. The file's form matches `epistemics/tests/competency.ttl` and the existing inline-`cqQuery`, `cqDataFile`, triple-term and ResultShape cases (affect, observations, places). No count of spec files is pinned. This is the standpoint slice's first competency spec.
- Dependency gate: example and test edges are not semantic (`purrdf-slice` 4.0.0, `ownership.rs:178-184`, `640-646`). They never reconcile against `sliceDependsOn` and raise no undeclared-dependency diagnostic. The epistemics terms in the example and the inline query need no manifest change, and no gate rejects them.

### 6. Catalogs

The script compared every `@x-gmeow-english` literal the diff adds against both catalogs. All 11 are present in fr and in zh, byte-identical, each with a non-empty msgstr. The catalogs contain no orphan msgids: all 147 entries in each match a current literal. Two of the replaced literals survive as msgids because `succeedsClaim` and `retractsClaim` still use them. I read every French entry in full: each is a real translation and carries every clause. The Chinese entries have the right structure and terms. `纠正` (correctsClaim) versus `更正` (ranking text) is a split that already existed. Only msgid and msgstr lines changed, so the reviewed-coverage golden's keys are unchanged.

### 7. Text truth

- docs.md and `docs/claim-lifecycle.md` are true at this head, except the "itself corrected" and "own ranker" wording (F1) and the pre-emptive global-safety classification (F3).
- Commit message: accurate.
- PR body: "It now counts only the ranker's own correction" is false (F1). "The order constraint already read a correction only within one ranker's view" is true about the two claims, not about who made the link. The rest is accurate. Its "What was shown" claims (lint, msgfmt, hooks) have no logs I could read, so they are unverified.
- No issue numbers, PR numbers or process references in repository files.
- `prose-check` exited 0 on both docs, the new example and competency file, the PR body and the commit message.

## What only a producer pass can prove

- That the frontend admits the revised formula and lowers it as traced, and that the generated procedural constraint parses and runs.
- That the four structural assertions and the competency question pass in the slice-spec producer, and that the result-shape harvest validates the new expected rows.
- That `correction-and-disagreement.ttl` passes example validation, and that annotation-syntax parsing works for both example files (the author checked them with those blocks stripped).
- That no reasoning or bridge gate flags the claim-token sort issue (F7).
- None of these proves F1 either way: no data exercises the closure constraint.

VERDICT: BLOCKED at ed29648111aac75580f6625881b5b6997ef78224
