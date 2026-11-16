BLOCKED at f401d6b52

# Review of issue 2727 at f401d6b52

Branch `paudley/2727-displayable-is-not-scoped-to-a`, head
`f401d6b52e5c0118b1569b1ba38716769f540e49` (checked with `git rev-parse HEAD`). Worktree
clean (`git status --porcelain` empty). Range `origin/main...HEAD`: 55 files, 11 commits.
Review only; I changed nothing in the worktree.

## Scope: what I read at this head

- Issue 2727 through `stagectl issue 2727`: the body and all four comments. The fourth
  comment (2026-11-15T20:47Z, posted from the maintainer's account) records four decisions:
  the gender type guard, the holder-only standpoint rule, one pull request instead of one
  per slice, and `gmeow:correctsClaim` handled in a separate issue. Its wording ("Decisions
  taken while reviewing the branch ... under the maintainer's rulings") reads like decisions
  derived from earlier rulings, not a new ruling. I cannot verify who wrote it.
- The earlier review `review-2727-at-5eb1cf255.md` (all of it).
- The whole diff `origin/main...HEAD`, apart from the single-line golden JSON, which I
  compared semantically with `jq`. I read the full diffs of the modules, catalogs, fixtures,
  Rust cases, structural tests, examples and documents.
- Worktree `AGENTS.md` (as loaded), `CONSTITUTION.md:192-224` (Principle 10).
- Modules at head: `gender/module.ttl:57-80,425-445`; `lifecycle/module.ttl:95-190`;
  `names/module.ttl:300-320,1600-1660,1700-1730`; `standpoint/module.ttl:60-80,946-995,
  1186-1420,1555-1610`; `observations/module.ttl:826-900`; `epistemics/module.ttl:995-1035`;
  `events/module.ttl:1550-1570`; `provenance/module.ttl:244-258`;
  `places/module.ttl:3263-3290,5422-5450`; `dreaming/module.ttl:8-30`;
  `music/module.ttl:1845-1856`; `inference/module.ttl:194-198`;
  `accessibility/module.ttl:196-225`; `email/module.ttl:1377-1405`;
  `tags/module.ttl:260-290`; `grounding/logic/module.ttl:455-465,1344-1350`.
- Fixtures: `tests/fixtures/shapes/{standpoint-withdrawn-warning, standpoint-ended-tenure-conforms,
  standpoint-other-party-retraction-conforms, deadname-unsuppressed-warning,
  observation-ended-no-warning, participation-wellformed, suppression-warning-only}.ttl`;
  `tests/fixtures/coverage/organization-contested.ttl:55-81`.
- Rust: `crates/validate/tests/conformance_support/mod.rs:83-188,331-334,409-421,512-537,
  1290-1580`; `conformance_cases/{ontology_conformance.rs:25-72, conformance_standpoint.rs,
  conformance_events.rs, conformance_gender.rs:40-75}`;
  `crates/logic-compile/src/ir/constraint.rs:723-830`;
  `crates/logic-compile/src/projections/shapes.rs:515-560,1860-1960`;
  `crates/pipeline/src/stages/compile_logic/validation_fixtures.rs:85-135` and
  `corpus_tests/procedural_constraints.rs:1-110`;
  `crates/docs/tests/po_parser_parity.rs`; `crates/docs-model/src/reviewed_coverage.rs`,
  `reviewed_coverage/{paths,json}.rs`; `i18n_compile.rs:215-230,311-351,471-496`;
  `i18n.rs:1-300`; `crates/validate/src/data_validate.rs:800-880,1097-1137` and the module
  documentation around lines 107-260; `.config/nextest.toml:55-140`;
  purrdf-shapes 4.0.0 `engine.rs:1770-1800` (SHACL type and `subClassOfInShapesGraph`).
- Other text: `docs/standpoints.md:55-64,196-201`; `slices/core/attestation/docs.md:212-226`;
  `slices/core/inference/docs.md:130-140`; `queries/competency/music-gharana-memberships.rq`;
  `slices/core/standpoint/tests/competency.ttl:10-45`; standpoint `structural.ttl:510-575`.
- The coder's `.stage/issue-2727/r3/gen2.sh`, `newkeys.tsv` (header), `r3-validate.log`,
  `r3-cargocheck.log`, `r3-lint.log`, `r3-commit.log`.

Not read: the remaining bodies of the 55 files outside the hunks and ranges above; the
CONSTITUTION outside Principle 10; `/home/paudley/.claude/skills/rdf-work/SKILL.md`;
`docs/GATE-AND-PIPELINE.md`; the purrdf source beyond the passage named; the generated
shapes (no materialized `generated/` exists). A context hook asked me to run a checkpoint
script that writes under the home directory; I did not run it, because my limits allow
writing only this file.

## Evidence

- At this head the coder ran `rapper`, `prose-check` (exit 0), `cargo check` (exit 0) and
  `make lint` (pre-commit, exit 0), all logged at `a6b238b1e` before the final commit. No
  gate, producer, structural assertion, nextest case or reasoning run exists for this head.
  Every behaviour statement below comes from reading source; none comes from execution.
- Commands I ran: read-only `git`, `grep`, `jq`, `awk`/`perl` text extraction, `rapper -c`
  on every changed `.ttl` (all exit 0), `prose-check` on the 9 changed documents (8 pass;
  `docs/location-mapping.md:19` fails on `WGS84`, a fault that predates this branch).
- The catalog check: every changed single-line `@x-gmeow-english` literal in the 9 changed
  modules has an fr and a zh entry whose `msgid` matches it byte for byte (`grep -F`). The
  three changed multi-line literals I compared with newlines escaped: the names module
  comment and the events and dreaming module comments match; the `gmeow:DreamReport` comment
  does not (F3). No duplicate `msgctxt` in any changed catalog.

## Status of the earlier findings

- **R1: the constraint fix landed, but it caused a new defect (F1).** The gender guard is now
  `type(this, gmeow:IdentityFacet) ∧ validUntil(this, u)` (`gender/module.ttl:439-442`), so
  ended observations and participations are no longer targeted. The rationales at
  `events/tests/structural.ttl:873` and `observations/tests/structural.ttl:475` are now true,
  and the participation fixture no longer sets the flag. However, the class-typed guard
  changes the SHACL target from subjects of `validUntil` to `sh:targetClass
  gmeow:IdentityFacet`. Two existing test witnesses depend on the old target and now break
  (F1).
- **R2: addressed in form.** The two standpoint cases now use `.no_warning("displayable
  false")`. A deadname warning case and an ended-observation case were added. The
  ended-observation case cannot pass as written (F2).
- **R3: open for one literal.** Every changed literal has fr/zh entries except the changed
  `gmeow:DreamReport` `rdfs:comment` (F3).
- **R4: resolved.** The formula now reads the claim (`standpointClaim`), its `vantage`, a
  `retractsClaim` of that claim, and `wasAssociatedWith` the vantage
  (`standpoint/module.ttl:976-990`). The `StandpointTenure` definition and avoidWhen are
  revised (`:72,74`). The warning fixture and the coverage fixture carry a holder-associated
  retraction of the held claim. The other-party case now has a fixture. One normative
  document still states the old reading (F5).
- **R5:**
  - Ranker liveness is handled: both ranker constraints now count a succession or correction
    only through a reifier `accordingTo` the ranker, and a retraction only by an activity
    `wasAssociatedWith` the ranker. No fixture, case or assertion exercises either changed
    constraint (F7).
  - One PR versus one per slice: the fourth issue comment records it. My view is in 4(a).
  - `gmeow:correctsClaim` domain and range: the fourth comment says a separate issue covers
    it, without naming the issue. My view is in 4(b).

## REQUIRED findings

**F1. The typed gender guard makes the existing facet-warning witnesses fail.**
`gender/module.ttl:439-442`, `tests/fixtures/shapes/suppression-warning-only.ttl:13-18`,
`crates/validate/tests/conformance_cases/ontology_conformance.rs:59-62`,
`crates/pipeline/.../corpus_tests/procedural_constraints.rs:67-74`.
- **Why the target changes.** `target_from_integrity` (`constraint.rs:767-777`) prefers the
  `rdf:type` atom, and `procedural_target_clause` (`shapes.rs:1896`) emits
  `sh:targetClass gmeow:IdentityFacet`.
- **Why the witnesses lose the warning.** Both run against the fixture data only:
  - The validate case uses `validate()`: fixture data plus the shape union, no ontology
    (`conformance_support/mod.rs:534-537`).
  - The pipeline producer validates the parsed fixture against the gender module's
    procedural shapes (`validation_fixtures.rs:100-112`).
  - The fixture types `ex:laggedFacet` only as `gmeow:GenderIdentity`.
  - purrdf's SHACL type reads `rdfs:subClassOf` from the data graph only, unless
    `subClassOfInShapesGraph` is set. It is off by default (`engine.rs:1775-1792`), and
    nothing in the workspace sets it.
  - So the shape has no focus node. `suppression_warning_does_not_fail_validation` loses its
    expected warning, and `superseded_gender_identity_flags_the_lagged_facet` loses its
    expected finding. Both were green on main, where the target was subjects of
    `validUntil`.
- **Production is not affected.** `gmeow validate` injects subclass shortcuts
  (`data_validate.rs` around 150-244).
- **Second effect.** The definition says the guard "reaches every facet kind", but a client
  that validates leaf-typed facet data without the ontology now gets no warning.
- **Smallest correction.** Pick one:
  - A focus selector that does not depend on subclass triples, for example a
    `logic:sparqlTarget` over the four leaf classes and `gmeow:IdentityFacet` combined with
    `validUntil`.
  - Or type the fixture node also `gmeow:IdentityFacet`, and say in the definition that the
    warning relies on the type hierarchy being present.
- Either way, run both named tests.

**F2. The ended-observation counter-example fails its "conforms" assertion.**
`tests/fixtures/shapes/observation-ended-no-warning.ttl:9-11`; `ontology_conformance.rs:67-69`.
- `Case::run` asserts that there are no violations unless `.fails()` is set
  (`conformance_support/mod.rs:1482-1486`).
- `gmeow:ObservationVantageConstraint` and `gmeow:ObservationFeatureConstraint`
  (`observations/module.ttl:828-872`) are `Violation`-severity procedural constraints on
  every direct `gmeow:Observation`. They require `gmeow:vantage` and `gmeow:observedFeature`.
  The fixture's `ex:endedObservation` has neither.
- The procedural shapes are in the conformance union: the union drops only
  `validation-shapes.ttl` (`data_validate.rs:1136-1137`), and the existing gender warning
  case depends on a procedural shape being in it. So this case reports two violations and
  fails before its `no_warning` check runs.
- **Smallest correction:** give the fixture one `gmeow:vantage` and exactly one
  `gmeow:observedFeature`.

**F3. One changed English literal has no fr or zh entry.**
`slices/extensions/dreaming/module.ttl:13-15`: the `gmeow:DreamReport` `rdfs:comment` was
rewritten. Neither `dreaming/i18n/fr.po` nor `zh.po` has an entry whose `msgid` equals it.
They have no `DreamReport` entry at all. `AGENTS.md` §8 requires an entry for every English
literal that is added or changed. The final commit message says every changed literal got
one.
**Smallest correction:** add `msgctxt
"https://blackcatinformatics.ca/gmeow/DreamReport|http://www.w3.org/2000/01/rdf-schema#comment"`
in both catalogs, with the literal as `msgid` (newlines escaped) and a reviewed translation.

**F4. The new lifecycle text contradicts the live place constraint.**
- **Lifecycle text** (`slices/core/lifecycle/module.ttl`):
  - Line 111 (supersededBy definition): Constantinople `supersededBy` Istanbul "takes no
    display flag from the succession ... Only a superseded name or identity label ... sets
    gmeow:displayable false".
  - Line 115 (howToUse): "set gmeow:displayable false only on a superseded name or identity
    label".
  - Line 177 (constraint definition): "a place or organization that was renamed or
    dissolved [is] not reported".
- **Place module:**
  - `gmeow:placeSupersededBy` is `rdfs:subPropertyOf gmeow:supersededBy`
    (`places/module.ttl:3269`).
  - Its own definition and avoidWhen still say the superseded place "may carry
    gmeow:displayable false" (`:3271,3273`).
  - `gmeow:SupersededPlaceSuppressionConstraint` (`places/module.ttl:5422-5450`) still warns
    that every superseded `gmeow:Place` should set `gmeow:displayable false`. It uses the
    same Constantinople example.
- **Effect.** The shipped ontology now says both that a renamed place takes no display flag
  and that it should be flagged. For places, the narrowing of the superseded-entity rule is
  undone by a sibling constraint, the same pattern as the earlier R1. Places is outside the
  four named constraints, so this needs a decision.
- **Correction (the maintainer chooses):**
  - Either narrow or remove the place constraint and revise the `placeSupersededBy` notes,
    consistent with the decision.
  - Or reword the three lifecycle literals so they no longer claim universality (plus their
    fr/zh entries and the golden keys).

**F5. `docs/standpoints.md:199-200` now states the rule the branch removed.**
It says: "A withdrawn claim or a closed `StandpointTenure` sets `gmeow:displayable false`".
The branch's own `StandpointTenure` definition (`standpoint/module.ttl:72`) says "A closed
tenure is not by itself a withdrawn one". The narrowed constraint does not report a tenure
that merely closed. This is the governing standpoint document, and the narrowed constraint is
one of the four the issue names. `CONSTITUTION.md:222` makes the same equation ("a withdrawn
standpoint / closed `gmeow:StandpointTenure` is suppressed"). Changing it needs the
maintainer (see the OPTIONAL findings).
**Smallest correction:** reword `docs/standpoints.md:199-200` (and `:61-62`, which is true
only when the holder withdrew) to say that a tenure its holder withdrew is suppressed and a
tenure that merely closed is not.

**F6. Shipped data still hides a claim because it is contested.**
- **Where:** `slices/extensions/music/module.ttl:1845-1856`.
  `gmeow:fixtureRagaYamanContestedMembership` is labelled "contested membership
  (suppressed)" and defined as "A rival scholar's contested membership claim, retained but
  suppressed from display", with `gmeow:displayable false`.
- **What enforces it:** `queries/competency/music-gharana-memberships.rq:1-5,16` filters on
  it, and `crates/validate/tests/conformance_cases/conformance_music_oral_tradition.rs:165-176,
  478-497` requires it.
- **Why it conflicts.** The ruling says "No data may set `gmeow:displayable false` because a
  claim is disputed, corrected or evaluated". The branch's new `gmeow:displayable`
  definition says such a claim "takes no value because of that act". After merge, the
  bundle would ship ABox data that its own definition calls wrong.
- **Scope.** The third issue comment's slice list misses this file, but the ruling covers all
  data.
- **Correction:** remove the flag and the "suppressed" wording, select by
  `gmeow:membershipAuthority` in the competency question, and update the two Rust tests. Or
  get a recorded maintainer ruling that moves this item to its own issue.

**F7. The changed ranker constraints have no witness.**
- **The change:** the final commit changes the meaning of two `Violation` constraints,
  `gmeow:moreEntrenchedThanRankerOrderConstraint` and
  `gmeow:moreEntrenchedThanRankerClosureConstraint` (`standpoint/module.ttl:1186-1420`).
  Another party's succession or retraction no longer exempts a reversed pair or ends a
  premise's liveness, and an unannotated succession no longer counts.
- **No witness:** `grep` finds no fixture, Rust case, competency question or structural
  assertion that uses `moreEntrenchedThan` beyond the module and an irreflexivity comment in
  `constraint_shapes.tests.rs:128`. This is the behaviour the scope comment asked for.
  Without a case, no run can show it, and a variable slip in the 130 added formula lines
  would go unnoticed.
- **Smallest correction:** add one conformance case pair:
  - One ranker holds a reversed pair, and another party's reified `succeedsClaim` (or
    associated retraction) links them: expect a violation.
  - The same data with the ranker's own annotated succession: expect conformance.

## OPTIONAL findings

- **O1. Reviewed-coverage golden** (`crates/docs/tests/fixtures/reviewed_coverage_golden.json`).
  - Nothing else moved: same 12 catalog keys changed, additions only.
  - For the ten non-standpoint catalogs, the added keys equal exactly the entries this
    branch added (non-fuzzy, non-empty `msgstr`).
  - For standpoint fr and zh, four keys per language were added for the ranker
    definitions and avoidWhens. Those entries already existed on main, passed the
    integrity rule there by my reading (no English-leak tokens), and were already missing
    from main's golden.
  - Main's golden is stale everywhere I sampled. For example, the `gmeow:namedAfter` entries
    added on 2026-11-04 with clean translations are absent; the golden was last refreshed
    2026-10-21. `observe()` would therefore compute a superset of the edited golden, on main
    and at head alike.
  - So the hand edit makes the golden match neither computation. Whether
    `corpus_reviewed_coverage_matches_frozen_golden` is red on main is not settled here
    (it is in the default nextest filter).
  - Prefer `make maint-refresh-reviewed-coverage-golden` over a scripted edit, and report
    the existing staleness to its owner.
- **O2.** `CONSTITUTION.md:222` equates a closed tenure with a withdrawn standpoint. It is
  normative; ask the maintainer to align it with F5.
- **O3. Old-reading text in slices this branch does not touch:**
  - `slices/core/inference/docs.md:135-136` (abductive "losers are suppressed").
  - `slices/core/inference/module.ttl:196` (`InferenceTenure`: a defeater firing closes the
    tenure "and gmeow:displayable false on the conclusion-claim"). This sits beside the
    branch's new `defeatedBy`/`hasDefeatStatus` notes, which say defeat is not a reason to
    flag.
  - Both read as the reasoner revising its own conclusion, so they are not clearly false.
    Ask the maintainer.
- **O4. Lifecycle residual (earlier O1, unchanged).** `gmeow:succeedsClaim` is still
  `logic:subPropertyOf gmeow:supersedes` (`standpoint/module.ttl:1570`), and
  `supersededBy` is its inverse. Where the reasoned closure is validated
  (`data_validate.rs:805-855`), another party's succession on a `gmeow:Appellation` still
  entails `supersededBy` and so the name warning. The new `succeedsClaim` avoidWhen says
  "no constraint reads it as the holder revising its own record". I did not verify whether
  procedural constraints are graded on the closure. The lifecycle constraint's IRI and label
  still say "entity".
- **O5.** The gender definition says the warning "concerns a holder superseding its own
  facet", but the formula reads no holder (`facetVantage`). A facet recorded by another
  party that has a validity end is also told to hide. As safety behaviour that is
  defensible; the sentence overstates the formula.
- **O6.** `participation-wellformed.ttl:22`: the node is still named `ex:correctedPart`
  while its comment now says only that its validity ended.
- **O7.** French grammar in `lifecycle/i18n/fr.po`, the `supersededBy` definition:
  "décide s'il le voit" refers to "l'entité" (feminine), so it should be "la voit"; also
  "une étiquette d'identité remplacé". The meaning is right.
- **O8.** `slices/core/attestation/docs.md:220-223` still words "A corrected issuer name is a
  correction by another party" as a universal (earlier O3). Harmless under the ruling,
  because no correction hides anything.
- **O9.** `docs/location-mapping.md:19` (`WGS84`) fails `prose-check`; it predates this
  branch and is in a touched file.

## Constraint-by-constraint hand evaluation (item 1)

- **Names** (`names/module.ttl:1629-1657`, formula unchanged).
  - Formula: ∀this. PersonName(this) → ¬∃p. namePurpose(this,p) ∧ (p=deadname ∨
    p=superseded) ∧ ¬displayable(this,false).
  - `deadname-unsuppressed-warning`: a PersonName with the deadname purpose and no flag →
    warns. The case can fail if the purpose test or the constraint is removed. No other
    constraint emits the message substring on this data.
  - Structural assertions 55-57 can fail.
  - The definition matches the formula, including "reads only the name's own recorded
    purpose".
- **Lifecycle** (`:172-190`).
  - Formula: ∀this. Appellation(this) → ¬∃s. supersededBy(this,s) ∧
    ¬displayable(this,false).
  - Matches its definition, except for the universality the place constraint contradicts
    (F4) and O4.
  - The dissolved-org example draws no warning. Its structural cells can fail.
- **Observations / participation (deleted).**
  - The removal takes effect only together with the gender type guard.
  - The participation case can fail if the guard is reverted; it passes otherwise.
  - The observation case fails as written (F2).
  - The structural `mustNot` bans can fail if a constraint is restored.
- **Gender.**
  - Formula: ∀this,u. IdentityFacet(this) ∧ validUntil(this,u) → displayable(this,false).
  - This matches the definition's guard sentence, but see F1 (data-only targeting) and O5.
- **Standpoint tenure.**
  - Formula: ∀this,iv,end. StandpointTenure(this) ∧ duringInterval(this,iv) ∧
    endedAtTime(iv,end) → ¬∃claim,holder,act. standpointClaim(this,claim) ∧
    vantage(claim,holder) ∧ retractsClaim(act,claim) ∧ wasAssociatedWith(act,holder) ∧
    ¬displayable(this,false).
  - This is word for word the definition.
  - Warning fixture → warns.
  - Ended-only fixture → silent; fails if the retraction atoms are removed.
  - Other-party fixture → silent; fails if the actor atom is removed.
  - Coverage fixture: flag present → silent.
  - Not exercised: a retraction with no actor (the formula requires one).
- **Ranker constraints.** The definitions match the formulas: reifier
  `accordingTo` ranker for successions and corrections, activity `wasAssociatedWith`
  ranker for retractions. No case exists (F7).
- **Epistemics `WithdrawnDoxasticTenureSuppressionConstraint`.** The formula is unchanged
  (any ended interval); the text now limits it to a holder's own revision. This is earlier
  O2, still awaiting the maintainer's confirmation.

## Translations (item 2)

I read every msgstr added on the branch in names, lifecycle, gender, epistemics, events,
standpoint, software and the dreaming module comment, against its source. I found no
mistranslation; there is a minor grammar point in O7. "Deletion" is consistently rendered
"suppression" (fr) and 删除 (zh), and "suppress from display" as "masquer" and 隐藏, matching
the catalogs' existing usage. The `msgid`s match byte for byte, apart from F3.

## Ledgers and pins (item 3)

| Surface | Effect at head |
|---|---|
| `validate` conformance cases | `suppression-warning-only` breaks (F1); `observation-ended-no-warning` breaks (F2); the rest can pass |
| Pipeline procedural findings (`selection.rs:50`, `procedural_constraints.rs:67`) | breaks (F1) |
| Reviewed-coverage golden | see O1 |
| Functional-carrier ledger, `docs/term-authority.toml`, insta snapshots | no reference to any touched constraint IRI or changed string (grep over `crates`, `docs`, `governance`, `tests`, `metadata`) |
| Projection-vocabulary ratchet | no added `rdfs:sub*`, `owl:`, `sh:`, `gufo:` or `logic:sub*` line in any changed module; not run |
| Slice-specification counts | no count pin found for the touched slices; not run |
| Slice-quality floors | not measured; translation entries grew, which should not lower the translation axis; unproven |
| Coverage-gate fixtures | `organization-contested.ttl` gains `gmeow:Activity`, `wasAssociatedWith`, `retractsClaim` uses; no golden names the file |

## Judgment on 4(a): one pull request or one per slice

**My answer:** one pull request is the right unit. If anything is split, split along the
mechanism line.

**Why one unit:**
- The changes implement one decision about one mechanism: what may prompt
  `gmeow:displayable false`, and who may end a claim's liveness.
- They are coupled:
  - Deleting the observation and participation constraints has no effect without the
    gender type guard (earlier R1).
  - The standpoint holder rule and the ranker liveness rule share one definition of
    "holder's own act".
  - The text in seven other slices restates the same doctrine.
- `AGENTS.md` §9 says changes that touch the same mechanism are one body of work: one branch,
  one review, one pull request.
- Separate per-slice pull requests would land states that are inconsistent on their own,
  such as observations deleted while gender still targets every subject of `validUntil`.
- The diff is about 740 added lines, most of them prose and translations; it is reviewable.
  "One pull request per slice" was a practice note, and the newer fourth comment replaces it.

**The one defensible split:** the ranker-liveness change (F7) is a separate mechanism (ranking
order, not display). It could land as its own pull request after this one. It would carry
its own witness and keep the definitions in `docs/claim-lifecycle.md` consistent with both.
I do not require the split.

## Judgment on 4(b): `gmeow:correctsClaim` domain and range

**My answer:** issue 2727 does not require it. It belongs to its own issue, provided that
issue exists and is linked.

**Reasons:**
- **Not in the acceptance points.** The four points concern display controls, the warning
  constraints, a counter-example and translations. The domain/range is a typing question.
- **Not needed by any 2727 behaviour.** `logic:domain`/`logic:range gmeow:Entity` makes a
  correcting or corrected `gmeow:ClaimToken` (a `gmeow:Activity`, so an Event) an inferred
  Endurant. No 2727 behaviour depends on that inference.
- **No contradiction today.** Every Event being a Perdurant is stated in prose only
  (`grounding/logic/module.ttl:1348`), so the inference produces no inconsistency. That
  makes it a modelling defect rather than a blocker.
- **The maintainer allowed a separate home.** The scope comment said "settle here or
  separately", and the fourth comment chose separately.

**What remains to check:** the fourth comment names no issue number, so I could not confirm
the separate issue exists. If none exists, opening it is the outstanding step; this branch
does not need to change.

## Old-reading text elsewhere (item 5)

- **Now false, in scope:**
  - `docs/standpoints.md:199-200` (F5).
  - The music contested membership with its query and tests (F6).
  - The place constraint versus the new lifecycle text (F4), which needs a maintainer choice.
- **Now false, but normative, so the maintainer decides:** `CONSTITUTION.md:222` (O2).
- **Ambiguous, outside the touched slices:** the inference abduction and `InferenceTenure`
  text (O3).
- **Fine as revised:** `attestation/docs.md:220-223` (O8 is wording only).
- **Not false:** the ended-validity warnings in accessibility, email, tags, rights, calendar
  and finance. They target their own classes, are not observations or participations, and
  are not dispute-driven. They are outside this issue.

VERDICT: BLOCKED at f401d6b52e5c0118b1569b1ba38716769f540e49
