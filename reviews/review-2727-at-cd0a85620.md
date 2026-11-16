BLOCKED at cd0a85620

# Review of issue 2727 at cd0a85620

Branch `paudley/2727-displayable-is-not-scoped-to-a`, head
`cd0a85620936c34c61ec0021dd8d7c80360180f1` (checked with `git rev-parse HEAD`). The worktree is
clean (`git status --porcelain` printed nothing). The head is one merge commit: first parent
`f401d6b52`, second parent `origin/main` = `e32f90c96`. The review fixes and the merge of main are
in that single commit. Range `origin/main...HEAD`: 63 files. Review only; I changed nothing in the
worktree.

## Scope: what I read at this head

- **Issue and earlier review.** Issue 2727 via `stagectl issue 2727`: the body and all four
  comments. The earlier review `review-2727-at-f401d6b52.md`, all of it.
- **The whole diff `origin/main...HEAD`.** I read every hunk of the modules, catalogs, fixtures,
  Rust cases, structural tests, examples, the query and the documents. The golden JSON I compared
  with `jq` and a recomputation (item 3).
- **Constraint lowering.**
  - `crates/logic-compile/src/ir/constraint.rs:30-45,720-860` (`target_from_integrity`).
  - `crates/logic-compile/src/projections/shapes.rs:1620-1712` (selector stripping, `violation_where`)
    and `:1839-1990` (`constraint_select`, `procedural_target_clause`, `try_project_block`).
  - `relational_core.rs:986-1010` (admission).
- **Statement lowering and validation paths.**
  - `crates/logic/src/statement_lowering.rs:1-115`.
  - `crates/logic/src/operator.rs:506-600`, the only caller, in `cli_world_dataset`.
  - `crates/validate/src/data_validate.rs:780-940`.
  - `crates/validate/tests/conformance_support/mod.rs:110-240,534-537,1180-1240`.
  - `crates/logic/src/reason/enactment.rs:1-80,200-260`; `reason/math_gate.rs:1-140`;
    `relational_core.rs:240-330`.
  - `docs/GATE-AND-PIPELINE.md:880-930`.
  - A `grep` for `reifiedStatement` over the locked purrdf 4.0.0 crate sources: no match.
- **Modules at head.**
  - `gender/module.ttl:20-80,425-445`; `sexuality` subclass lines.
  - `standpoint/module.ttl:1140-1425`; `observations/module.ttl:820-945`.
  - `lifecycle/module.ttl:95-200`; `names/module.ttl:300-320,1600-1660`.
  - `epistemics/module.ttl` hunks vs main; `music/module.ttl:1845-1901`.
  - `dreaming/module.ttl:10-60,500-520`; `affect/module.ttl:3110-3132`.
- **Fixtures.**
  - The new and changed files under `tests/fixtures/shapes/`, plus `suppression-warning-only.ttl`.
  - `slices/core/affect/examples/background-moods.ttl`; `slices/core/tags/examples/contested-tagging.ttl:1-80`.
- **Rust cases and pins.**
  - `ontology_conformance.rs:40-75`, `conformance_standpoint.rs` hunk,
    `conformance_events.rs` hunk, `conformance_music_oral_tradition.rs:160-200,485-535`.
  - `crates/pipeline/.../corpus_tests/procedural_constraints.rs:1-120`,
    `.../corpus_tests/disclosure_decisions.rs:1-60`, `.../validation_fixtures/selection.rs` (grep).
  - `crates/pipeline/src/stages/constraint_shapes.tests.rs:75-140`.
  - `crates/test-batch-macros/src/lib.rs:1-80,320-345`; `crates/validate/tests/conformance_suite.rs:1-60`.
- **Golden machinery.** `crates/docs/tests/po_parser_parity.rs:51-67`;
  `crates/docs-model/src/reviewed_coverage.rs` (all); `i18n_compile.rs:215-230,471-496`;
  `i18n.rs:107-147`; `Makefile:1322-1323`.
- **Other text.**
  - `docs/standpoints.md:55-66,193-205`; `docs/reasoning.md:130-145`;
    `docs/term-authority.toml:305-320`; `docs/SLICE_GUIDE.md:205-236,420-436`.
  - `CONSTITUTION.md:222` (grep).
- **Not read.**
  - The rest of each touched module and catalog outside the hunks above.
  - The skill files `rdf-work/SKILL.md` and `rust-work/SKILL.md`.
  - The coder's `r4-hand-eval.md`, `r4-*.log` and `msgid-check.pl`. I did my own checks instead.
  - The purrdf-shapes engine source beyond the `SPARQLTarget` support grep.
  - The `gmeow-slicetest` and producer example-validation paths.
- **A hook I did not run.** A context hook asked me to run a checkpoint script that writes under
  the home directory. My limits allow no writes there, so I did not run it.

## Evidence (commands I ran)

- `rapper -c` on every changed `.ttl`: all exit 0.
- `prose-check` on the 9 changed documents: exit 1. The one finding is
  `docs/location-mapping.md:19:11` (`WGS84`), which predates this branch.
  - I first sent that output to a temporary file under `.stage/reviews/`, against my write limit.
    I read it, listed it, then deleted that one exact path.
- **Catalog check (Perl, read-only).** For every `@x-gmeow-english` literal that changed in the 9
  touched modules (39 literals), both the fr and the zh catalog have an entry whose unescaped
  `msgid` equals it byte for byte.
  - There are no duplicate `msgctxt` and no fuzzy entries in the 18 changed catalogs.
  - The branch adds no orphan entry. Software has 1 orphan on main and 1 at head, the same one.
- **Contract counts** by `grep` of the `batch_test`/`batch_cases`/`#[case` attributes in the 94
  files that `conformance_suite.rs` links (item 5).
- **Golden.** A `jq` diff, plus a Perl recomputation of the reviewed-coverage predicate (item 3).
- Nothing was built, run or reasoned. Every behaviour statement below comes from reading source
  and working formulas by hand.

## Status of the seven earlier findings

- **F1 (the typed gender guard broke two witnesses): resolved.** Work-through in item 1(a).
- **F2 (the ended-observation fixture failed): resolved.**
  - The fixture now has one `gmeow:vantage` and one `gmeow:observedFeature`, so all three
    direct-Observation Violation constraints (`observations/module.ttl:829-903`) are satisfied.
  - The `SupersededObservationSuppressionConstraint` is deleted, so `.no_warning("displayable false")`
    can hold.
- **F3 (DreamReport comment had no catalog entry): resolved.** Both catalogs carry the entry, with a
  byte-equal `msgid` (newlines escaped). The translations are faithful.
- **F4 (lifecycle text contradicted the place rule): resolved by rewording.** Each of the three
  lifecycle literals now confines itself to what this slice prompts and says "A more specific
  relation may state a stricter display rule of its own". That no longer contradicts the places
  rule, which is out of scope here by decision. A residual wording point is O1.
- **F5 (`docs/standpoints.md` stated the removed rule): open.** Lines 61-66 were fixed. Lines
  202-203 still state the removed rule. See R1.
- **F6 (shipped data hid a contested claim): resolved.** Work-through in item 1(c).
- **F7 (the ranker constraints had no witness): open in substance.** The case pair exists and does
  check the projected query, but over a form of data that no validation path produces. See R3.

## REQUIRED findings

**R1. `docs/standpoints.md:202-203` still states the rule the branch removed.**
- It reads: "A withdrawn claim or a closed `StandpointTenure` sets `gmeow:displayable false` during
  ordinary revision".
- This contradicts the same document's revised lines 61-66 ("a tenure that merely closed ... takes
  no flag"). It also contradicts the narrowed `WithdrawnStandpointSuppressionConstraint` and the
  `StandpointTenure` definition at head.
- This is the governing standpoint document, and the commit message says the guide now "states that
  only a tenure its holder withdrew is suppressed". Here it does not.
- **Smallest correction:** reword the bullet: "A claim its holder withdrew, or a
  `StandpointTenure` whose held claim its holder retracted, sets `gmeow:displayable false`; a
  tenure that merely closed takes no flag."
- `CONSTITUTION.md:222` makes the same equation. It is normative, so the maintainer decides (O2).

**R2. The French lifecycle definition renders display suppression as deletion.**
- Where: `slices/core/lifecycle/i18n/fr.po`, entry
  `gmeow/supersededBy|skos:definition`. The English reads "the only suppression the lifecycle slice
  prompts is for a superseded name or identity label". The French reads "la seule suppression que
  suscite la tranche lifecycle vise un nom remplacé ...".
- These catalogs use "masquer"/"masquage" for display suppression and "supprimer"/"suppression" for
  deletion. The same entry says "conservée (jamais supprimée)" one sentence earlier, and
  `standpoint/i18n/fr.po` uses "jamais une suppression" for "never a deletion".
- So the bare noun "suppression" here reads as "the only deletion the lifecycle slice prompts".
  That inverts Principle 10's central distinction, and the entry counts as reviewed (not fuzzy).
- The zh entry is correct (隐藏).
- **Smallest correction:** "le seul masquage que suscite la tranche lifecycle vise un nom remplacé
  ou une étiquette d'identité remplacée". No msgid change is needed.

**R3. The new ranker cases check the query against data that RDF 1.2 input never becomes. The
ranker constraints never see real rankings.**
- **What the fixtures contain.** `standpoint-ranker-other-party-succession.ttl` and
  `standpoint-ranker-own-succession-conforms.ttl` author `logic:reifiedStatementSubject` /
  `Predicate` / `Object` triples directly. They contain no `rdf:reifies <<( ... )>>`.
- **Where those edges come from in production.** They are produced only by
  `statement_lowering::lower_reifiers` (`crates/logic/src/statement_lowering.rs:83-111`). Its only
  caller is `cli_world_dataset` in `crates/logic/src/operator.rs:566`, the operator CLI's reasoning
  world.
- **What the SHACL path sees instead.**
  - `gmeow validate` Tier 1 uses `data_dataset_flat`/`flatten_to_default_graph`
    (`data_validate.rs:899-933`), which keeps base quads, reifiers and annotations and adds no
    component edges.
  - The conformance `validate()` path (`conformance_support/mod.rs:183-187,534-537`) does the same.
  - The pipeline fixture validation does the same.
  - No purrdf 4.0.0 source mentions `reifiedStatement`.
  - The ranker constraints carry no `gmeow:enforcesFailureClass`. So by `enactment.rs:233-247`
    (`NoFailureClass`), their only enforcement surface is the derived SHACL, and that runs on the
    flat view.
- **Hand evaluation on the fixtures (the projected `sh:sparql` over the literal triples).**
  - Other-party fixture, `moreEntrenchedThanRankerOrderConstraint`, focus a:
    first = rankAB, second = rankBA, ranker = `ex:ranker`. The only link is
    `thirdPartySuccession` (rankBA `succeedsClaim` rankAB), but it is `accordingTo ex:third-party`,
    so neither exemption matches. There is no correction or retraction. Result: violation (focus b
    likewise), with message "...in both directions...".
  - Other-party fixture, closure constraint, focus a: first = rankAB (live: the only link naming
    rankAB is the third party's), second = rankBA (live), lower = a. No claim ranks a over a.
    Result: violation, message "...must also hold a live claim...". Focus b likewise.
  - Own-succession fixture, order constraint: `ownSuccession` (rankBA → rankAB, `accordingTo
    ex:ranker`) matches the second→first exemption for focus a and the first→second exemption for
    focus b. No violation.
  - Own-succession fixture, closure constraint: rankAB is not live (`ownSuccession` names its
    ranker), so in each direction one premise fails the guard. No violation.
  - Attribution constraint on both fixtures: rankAB and rankBA supply `accordingTo` and `validFrom`.
    Satisfied.
  - Both cases can fail. Dropping the `accordingTo ?ranker` atom from the link patterns would exempt
    the other-party pair and clear both expected violations.
- **The same data written as RDF 1.2** (`ex:a gmeow:moreEntrenchedThan ex:b ~ ex:rankAB {|
  gmeow:accordingTo ex:ranker ; gmeow:validFrom ... |}`, the form the constraints' own definitions
  and `howToUse` prescribe):
  - The order and closure guards never match. No reversed pair, and no missing closure, is ever
    reported, whoever made the succession.
  - `moreEntrenchedThanAttributionConstraint` reports every correctly attributed edge as a
    Violation, because its consequent finds no component edge.
  - So the behaviour the case pair is named for ("another party's succession is flagged") does not
    occur on any validation entry point for real data. The commit message's "A case pair covers the
    ranker order and closure rules" overstates what is shown.
- **Where it comes from.** The use of lowered components in these three constraints predates the
  branch. It is shared with other slices' constraints (affect, dreaming, record-metadata and
  others). This branch changed the ranker semantics and supplied the witness, so it now presents a
  rule enforced on real data that is not.
- **Correction (maintainer chooses):**
  - (a) Feed the `lower_reifiers` rows into the SHACL validation view: the flat data view in
    `data_validate.rs`, the conformance `validate()` and the pipeline fixture validation. Then add
    an RDF 1.2 twin of each ranker fixture.
  - (b) Get a recorded maintainer ruling that moves the lowering gap to its own issue. Then reword
    the fixture headers and the publication text so they say the cases check the projected query
    over pre-lowered components only, and that RDF 1.2 input does not reach these constraints yet.
  - Either way, the false Violation from the attribution constraint on real rankings needs a named
    owner.

## OPTIONAL findings

- **O1. "or identity label" overstates the lifecycle constraint.** The `supersededBy` definition,
  `avoidWhen` and `howToUse` (`lifecycle/module.ttl:111,113,115`) say the lifecycle slice prompts
  suppression for "a superseded name or identity label". Its only constraint targets
  `gmeow:Appellation` (`:187`). Identity facets are prompted by the gender slice. Drop "or identity
  label", or say "(identity labels: see the gender slice)".
- **O2.** `CONSTITUTION.md:222` still equates a closed `StandpointTenure` with suppression. It is
  normative; ask the maintainer to align it with R1.
- **O3. Gender selector is a closed list.** The selector names five classes explicitly
  (`gender/module.ttl:439-440`). Today no other class sits below these (grep for subclasses of the
  four leaf classes and `IdentityFacet` finds only the four, in `gender` and `sexuality`). A future
  facet subclass would be silently missed, and the definition's "reaches nothing else" would still
  read as true. The precedent `logic:comTarget` (`observations/module.ttl:932-934`) uses
  `a/rdfs:subClassOf*`, which is hierarchy-dependent. That is why it was not chosen, and the
  trade-off is defensible.
- **O4. Implementation mirror.** The loop added at `conformance_music_oral_tradition.rs:515-521`
  asserts that every returned row's authority is the Kirana standpoint, which the query's `FILTER`
  already guarantees. The count and exclusion assertions are what protect the behaviour.
- **O5. French style.** The French `ClaimEvaluation` `howToUse` mixes infinitives ("Créer",
  "pointer") from the branch with imperatives ("Donnez", "évaluez") kept from main. It also changed
  main's "une ClaimEvaluation" to "un ClaimEvaluation". The meaning is right.
- **O6. Earlier O4 (unchanged).** `succeedsClaim` is still `logic:subPropertyOf gmeow:supersedes`.
  Only relevant where a reasoned closure is validated; Tier-1 SHACL does not entail it.
- **O7.** `docs/SLICE_GUIDE.md:227-230` and `slices/core/tags/examples/contested-tagging.ttl:14-16,72-79`
  describe the tagger's own retracted tagging as suppressed. It is a self-revision, so the text is
  not false under the ruling. The "contested claim, the suppressed value" pairing could be clearer.
- **O8.** `docs/location-mapping.md:19` (`WGS84`) fails `prose-check`. It predates the branch and
  sits in a touched file.

## Item 1: hand work

**(a) Gender guard.**
- **Lowering.** `target_from_integrity` (`constraint.rs:778-791`) finds no `rdf:type` atom. It
  takes the `logic:sparqlTarget(this, "...")` atom and yields `ShapeTarget::Sparql(select)`.
  - `procedural_target_clause` (`shapes.rs:1905-1907`) emits
    `sh:target [ a sh:SPARQLTarget ; sh:select """SELECT DISTINCT ?this WHERE { ?this a ?facetClass ;
    <…/validUntil> ?until . FILTER ( ?facetClass IN (<…/GenderIdentity>, <…/GenderExpression>,
    <…/SexualOrientation>, <…/RomanticOrientation>, <…/IdentityFacet>) ) }""" ]`.
  - The literal has no quotes or backslashes, so it embeds safely.
  - `strip_direct_type_guard` removes the marker (`shapes.rs:1666-1685`). `violation_where` then
    lowers only the negated consequent, which gives
    `SELECT $this WHERE { FILTER NOT EXISTS { $this gmeow:displayable "false"^^xsd:boolean } }`.
  - The comTarget precedent strips its guard the same way.
- **Against `suppression-warning-only.ttl` with no ontology loaded.** `ex:laggedFacet a
  gmeow:GenderIdentity ; gmeow:validUntil ... ; gmeow:displayable true`.
  - The target matches on the directly asserted type; no `rdfs:subClassOf` is needed.
  - `displayable false` is absent, so the shape reports the warning "A superseded identity facet
    ... should set gmeow:displayable false ...".
  - `ontology_conformance.rs:59-62` expects "should set gmeow:displayable false": it holds.
  - `procedural_constraints.rs:67-74` expects a finding focused on `laggedFacet`: it holds by the
    same reading, because the producer validates the fixture against the gender module's shapes.
  - Neither was executed.
- **Fixtures that must stay silent.** The ended observation and the ended participation are not of
  the five classes, so neither is a focus.

**(b) Ranker fixtures:** see R3. They use the form the projected `sh:sparql` reads, which is not the
form any SHACL validation path presents from RDF 1.2 data.

**(c) Music.**
- `gmeow:fixtureRagaYamanContestedMembership` loses its flag and its "suppressed" wording. The
  query selects `FILTER ( ?authority = gmeow:fixtureKiranaGharanaStandpoint )` (`.rq:16-20`).
- **The query test** (`:494-531`) can fail:
  - removing the `FILTER` returns four rows, so the count and exclusion assertions fail;
  - changing the contested membership's authority to Kirana makes it a fourth row.
- **The retained-membership test** (`:169-192`) can fail if the node is deleted or retyped, if its
  authority changes, or if the flag is re-added.
- **What they protect now.** The tests were written for retention, never deletion, and they still
  protect it. The suppression they also protected was the very behaviour the ruling forbids, and
  they now protect the reverse: attribution, with no flag. The three Kirana memberships still carry
  `gmeow:displayable true`, which does no harm.

## Item 2: the merge of main

- **The module.** Main's additions survive: the `gmeow:putForwardByClaim` sentence, the
  `gmeow:statedRationale` sentence and the evaluation `avoidWhen` ("it never suppresses, ranks or
  replaces that claim ...") are all present and unchanged.
  - The combined `ClaimEvaluation` `howToUse` keeps main's text and replaces only main's sentence
    "Supersede by minting ... (Principle 10)". The replacement is the branch's two sentences
    (same-evaluator revision suppresses; another party's evaluation never flags).
- **The catalogs.** Each has exactly one `ClaimEvaluation|howToUse` entry, and its `msgid` equals
  the combined literal byte for byte.
  - fr: faithful to all six sentences, including main's last two, carried over verbatim.
  - zh: faithful as well.
  - Main's `putForwardByClaim` entries remain. Nothing is duplicated.
- **Epistemics `docs.md`.** Both sides survive.
- **The golden:** see item 3.

## Item 3: the reviewed-coverage golden

- **Size of the change.** Catalog keys went from 148 to 190. About 29,000 entries were added across
  about 85 catalogs, and these were removed:
  - affect: 35 fr and 35 zh;
  - slice-quality-rubric: 19 fr and 3 zh;
  - grounding/logic: 18 fr and 18 zh.
  Almost all of this is outside the 9 touched slices.
- **Recomputation.** I re-implemented `observe()`'s predicate:
  - non-fuzzy entries with a non-empty `msgstr`;
  - `msgctxt` predicate expanded the way `expand_predicate` does;
  - every key the golden at head lists.
  - Against the head catalogs, the golden matches all but 12 keys out of tens of thousands.
  - 6 keys are candidates absent from the golden. They are plausible integrity rejections, for
    example `math:RHat` label "R-hat", which equals its source.
  - 6 golden keys my simple parser missed, in affect, software and lang.
- **Why that explains the moves.** Every catalog outside the touched slices is byte-identical to
  main's (the branch diff does not touch it). So every move outside this branch's slices is
  explained by catalogs already on main.
  - Main's golden was stale. `corpus_reviewed_coverage_matches_frozen_golden` compares with
    `assert_eq!` (`po_parser_parity.rs:63-66`), so by this reading it is red on main.
  - The refresh is harmless and true, assuming `translation_has_integrity` behaves as my residue
    suggests. That was not executed.
- **The touched slices.** Their added keys match the entries this branch added.

## Item 4: translations

- I read every `msgstr` this branch added or changed against its English source:
  - names, lifecycle, gender, epistemics (including the merged entries), events, standpoint,
    dreaming, music, software.
  - Chinese throughout uses 隐藏 for display suppression and 删除 for deletion. French uses
    masquer/masquage for display suppression and supprimer for deletion, with one exception (R2).
  - Byte-equality and orphans are covered under Evidence.
- **Mistranslation marked reviewed:** R2 only. Style: O5.

## Item 5: the conformance contract count

- **The assertions.** `run_registered_conformance_contracts` (`conformance_support/mod.rs:1215-1228`)
  asserts `contracts.len() == 519` and, after that, a logical-case sum of 708. The file is unchanged
  by the branch.
- **Registration.** Each `#[gmeow_test_batch_macros::batch_test]` registers one contract with one
  case. Each `#[batch_cases]` registers one contract with as many cases as it has `#[case]` rows
  (`test-batch-macros/src/lib.rs:13-20,37-74`). There is no other registration (grep).
- **Counts over the 94 linked files:**

  | | batch_test | batch_cases | `#[case]` rows | contracts | logical cases |
  |---|---|---|---|---|---|
  | origin/main | 463 | 45 | 237 | 508 | 700 |
  | head | 463 | 45 | 243 | 508 | 706 |

- **What the branch changes.** It does not change the contract count: 508 on both, against the
  asserted 519. It adds 6 logical cases: 4 in standpoint, 2 in `ontology_conformance`.
- **Effect.** The 519 assertion fails first, on main and at head alike. Whoever corrects it must
  also set the case pin to the value counted at that time. That is 706 at this head by my count, not
  main's 700 and not the stale 708.
- This is a pre-existing red pin, not caused by this branch. It was counted by `grep`, not by a run.

## Item 6: ledgers, pins and text

| Surface | Effect at head |
|---|---|
| insta snapshots, `docs/term-authority.toml`, functional-carrier ledgers | No reference to any changed constraint IRI or changed string. `term-authority.toml:317` and `docs/reasoning.md:139` name the retired `SupersededFacetSuppressionShape`, which is unchanged and still true. |
| `constraint_shapes.tests.rs` 24-shape pin | Not affected: no FOL characteristic changed. |
| Pipeline procedural findings (`procedural_constraints.rs:67`) | Holds by item 1(a); not run. |
| Disclosure-decision corpus test | Reads the observations module's `DisclosureDecision` projection; the deletion does not touch it. |
| Conformance count pins | Item 5. |
| Reviewed-coverage golden | Item 3. |
| Projection-vocabulary ratchet | No added `rdfs:sub*`, `owl:`, `sh:` or `gufo:` lines in changed modules; not run. |
| Slice-quality floors | Not measured. Removing a constraint from observations and events and adding catalog entries could move axis scores; unproven either way. |
| Process references | No issue/PR numbers or process wording added in committed files (grep of added lines). The `#41` and `#39`-style tags were removed from `participation-wellformed.ttl`; `suppression-warning-only.ttl:1` still carries `(#39)` from main. |
| Text the branch makes false | `docs/standpoints.md:202-203` (R1); `CONSTITUTION.md:222` (O2). |

## Not shown (unexecuted)

- No `make check`, producer run, `make produce-test-fixtures`, nextest, conformance runner,
  structural assertion run, reasoning or slice-quality sweep exists for this head, by me or (as far
  as I read) by the coder.
- **Read but never run:**
  - the new conformance cases: deadname warning, ended observation, ended participation, the four
    standpoint cases;
  - the rewritten music cases;
  - the pipeline gender finding;
  - the structural assertions added in gender, lifecycle and names.
- `corpus_reviewed_coverage_matches_frozen_golden`, including whether
  `translation_has_integrity` rejects exactly the residue I found.
- Whether other Violation-severity constraints in the union fire on the new fixtures' nodes. I
  checked the observation, Place and Standpoint guards I could find; I did not run the whole union.
- The real-data behaviour in R3 is derived from code reading (no lowering on any SHACL path). No
  RDF 1.2 ranking was validated.
