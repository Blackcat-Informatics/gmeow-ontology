PASS at 5c31fadbe

# Delivery review: evaluating a claim that exists as a reified statement, merged with main

- Head reviewed: `5c31fadbe9c34e355376ae3dca74130cb5df7f67`, which is 5b14a825d merged with
  origin/main `c0a3734f8a2447a1289ab1a8babb1e7babd4f436`. The merge base is c0a3734f8.
- Branch diff: `git diff origin/main...5c31fadbe`, nine files under `slices/core/epistemics`,
  404 lines added and 3 removed.
- Started 2026-11-15T10:44:29-06:00; written 2026-11-15T10:48:44-06:00 (`date -Is`).
- Mode: read-only.
  - No make, cargo, producer, gate or reasoning run.
  - The slice-quality mirrors ran under
    `flock /home/paudley/Active/gmeow-ontology/.stage/build.lock`.
  - Their outputs are in `.stage/reviews/prose-scratch/` (`tr-epi-main.txt` and
    `tr-epi-5c31fadbe.txt`; the prose and doc-maturity output is reproduced below).
- Earlier reviews: `review-2726-at-a08f86c98.md` and `review-2726-at-5b14a825d.md`.

## Coverage

| File | Lines | Commit | Read |
|---|---|---|---|
| the nine branch files | identity checked with `git diff --quiet 5b14a825d 5c31fadbe -- slices/core/epistemics`, exit 0; content read in full in the two earlier reviews | 5c31fadbe | identity checked; not reread |
| crates/logic-compile/src/frontend.rs | functional and inverse-functional family (the class-scoped caps with `is_authoring_ns`); every code line changed in `git diff 0ee2d5479 HEAD` (non-comment lines) | 5c31fadbe | yes |
| crates/logic-compile/src/projections/shapes/foreign_domain_tests.rs | 1-68 | 5c31fadbe | yes |
| slices/core/standpoint/module.ttl | `correctsClaim` (1491-1503) | 5c31fadbe | yes |
| slices/core/standpoint/docs.md | 140-175 | 5c31fadbe | yes |
| slices/core/standpoint/examples/correction-and-disagreement.ttl | 1-58 | 5c31fadbe | yes |
| docs/claim-lifecycle.md | line 43 (grep) | 5c31fadbe | partly |
| crates/logic/src/reason/program/equality_tests.rs | 85-145 | 5c31fadbe | yes |
| crates/logic/src/reason/mod.rs | 860-930 | 5c31fadbe | yes |
| crates/logic/src/reason/dl.tests.rs | grep hits on the unique-name policy (1581-1618, 2379, 2461, 3472) | 5c31fadbe | partly |
| crates/logic/src/reason/schema.rs | grep hits at 665-670 | 5c31fadbe | partly |
| slices/core/slice-quality-rubric/module.ttl | the seven `afc-epistemics-*` records | 5c31fadbe | yes |
| .stage/reviews/quality-floors-sources-inquiry-evidence.md and the five scratch scripts | all | n/a | yes |
| standpoint i18n, competency and structural files changed on main | none | 5c31fadbe | NOT read |
| `coherence_observations.rs` and `foundation.rs` functional handling | grep only | 5c31fadbe | NOT read in full |

## (1) The condition is met

- The axioms at this head are unchanged: `gmeow:putForwardByClaim logic:domain rdf:Statement` and
  `gmeow:putForwardByClaimFunctionality` with characteristic `logic:functionalProperty`.
- In `derive_validation_shapes` the class-scoped functional cap now reads
  `matches!(classify(&c), Some(ConstraintComponent::Class(_))) && is_authoring_ns(&c)`. The class
  that receives the cap comes from `logic:domain`. The inverse-functional cap applies the same
  check to the `logic:range` class. With `rdf:Statement` as the domain, no
  `ShapeTarget::Class(rdf:Statement)` entry is created. So no derived shape for this property is
  named in the `rdf:` namespace or targets an `rdf:` class.
- The property-scoped `ShapeTarget::SubjectsOf(gmeow:putForwardByClaim)` cap (`sh:maxCount 1`) is
  still added without condition. Its shape is `gmeow:putForwardByClaim-domain-shape`, in the
  authoring namespace. It bounds every subject of the property, whether or not the subject is
  typed.
- `foreign_domain_tests.rs` covers both families with this exact shape. It uses
  `g:run logic:domain rdf:Statement` with a functional record, and `g:token logic:range
  rdf:Statement` with an inverse-functional record. It asserts that no derived shape IRI lies
  outside the authoring namespaces. It then validates `ex:two g:run 1, 2` and gets exactly one
  `sh:MaxCountConstraintComponent`, on `ex:two`.
- I read the test; I did not run it.

## (2) The merge changed nothing of the branch, and nothing on main conflicts with it

- **Branch files.** All nine files are byte-identical at 5b14a825d and 5c31fadbe
  (`git diff --quiet`, exit 0). The branch's own changes are exactly what was reviewed at
  5b14a825d.
- **Correction text merged from main.** The change that makes a correction not suppress the
  corrected claim is now on main:
  - `gmeow:correctsClaim` is no longer a sub-property of `gmeow:supersedes`.
  - Its useWhen says plain disagreement is recorded as a `gmeow:ClaimEvaluation` that
    `gmeow:evaluates` the claim.
  - The standpoint guide (`docs.md:140-175`) and `docs/claim-lifecycle.md:43` add the
    two-pattern table.
  - The guide states: "No data sets `gmeow:displayable false` because a claim is disputed,
    corrected or evaluated; what a particular reader sees of a disputed claim is decided at
    projection".
- **How the branch fits.** The branch text agrees with all of this. Its guide says an
  evaluation never suppresses, ranks or replaces the evaluated claim, gives no ground for
  `displayable false`, and that display is reader policy decided at projection. The merged example
  `correction-and-disagreement.ttl` evaluates a `gmeow:ClaimToken`, the same pattern the branch
  extends to reified statements through the bridge.
- **One related point.** Main's standpoint docs tell users to name the reifier of a ranking
  statement so that the lifecycle relations can reach it. That is consistent with the branch's
  rule of one named reifier per act.
- **The one line that touches the same subject.** The retained ClaimEvaluation howToUse sentence
  "Supersede by minting a new evaluation and suppressing the old with gmeow:displayable false
  (Principle 10)" is about an evaluator replacing its own evaluation. Main's guide classes a
  standpoint changing its own position as succession or retraction, "neither is a dispute", and
  says Principle 10 names the cases that do prompt suppression. So there is no conflict in
  meaning. It remains the open display-policy item already noted as optional (earlier F3).
- **The counter-example's source shape.**
  - Since the branch base, main changed `frontend.rs` in three ways: the namespace guard, plus
    two changes that exclude reification-obligation declarations by identity.
  - The obligation exclusion applies only to class shapes whose class carries an obligation
    declaration. `gmeow:ClaimEvaluation` carries none.
  - The ClaimEvaluation axioms (`allValuesFrom gmeow:ClaimToken` on `gmeow:evaluates`, with the
    class-scoped ClosureEntry) are unchanged.
  - So `gmeow:ClaimEvaluation-shape` is still the shape that reports
    `shacl.ClassConstraintComponent` for the counter-example. The slicetest harness is unchanged
    since the branch base.

## (3) Slice-quality floors for the epistemics slice

The floors are the seven `gmeow:afc-epistemics-*` records in
`slices/core/slice-quality-rubric/module.ttl`; the branch does not touch that file. "Before" is
origin/main c0a3734f8. "After" is 5c31fadbe.

| Axis | Floor | Before | After | Verdict |
|---|---|---|---|---|
| axisProseQuality | 0.7912087912087912 | 72/91 = 0.791209 (reproduces the floor exactly) | 74/93 = 0.795699 | holds |
| axisTranslationCoverage | 0.4967948717948718 | 212 expected, fr 55, zh 55: 0.506289 | 218 expected, fr 61, zh 61: 0.519878 | holds |
| axisDocMaturity | 0.583300 | 7/12 = 0.5833 (reproduces the floor) | 7/12 = 0.5833 | holds, no margin |
| axisDocumentation | 1.0 | 1.0 (docs.md prose 12,860 chars, over 200) | 1.0 (17,239 chars) | holds |
| axisShapeMigration | 0.0 | at least 0 | at least 0 | holds trivially; branch does not touch `shapes.ttl` |
| axisGmnGlyphOptimality | 1.0 | 1.0 (main gated) | 1.0 by reading | holds: no glyph candidate or denotation in `slices/grounding/lang/module.ttl` names `putForwardByClaim` |
| axisGmn1Coverage | 0.9403973509933775 | main gated | not computed | not verifiable by reading; see below |

Details:

- **Prose quality.** The two new checks pass:
  - The `gmeow:putForwardByClaim` definition carries boundary cues ("not", "not a property of
    every reifier").
  - Its `skos:example` is a worked triple: `ex:skyStatement rdf:reifies <<( … )>> ;
    gmeow:putForwardByClaim ex:aliceSaysBlue .`
  - `gmeow:putForwardByClaimFunctionality` is a `logic:NamedIndividual`, outside the TBox the
    axis checks.
  - The changed `ClaimEvaluation` and `evaluates` literals are usage notes, which this axis does
    not check.
  - The 19 failing checks are the same terms before and after, all predating the branch (for
    example `producesClaim`, `Proposition`, `knowerAgent`).
- **Translation coverage.**
  - Six new expected pairs: label, definition and scopeNote of each new term. All six are covered
    in fr and zh.
  - The examples are excluded as notation.
  - The usage notes (useWhen, avoidWhen, howToUse) are not localizable predicates for this axis.
  - The main score (0.5063) is above the committed floor and does not reproduce it exactly. I
    infer the floor was set at an earlier commit and is raise-only. The mirror itself was
    validated against the inquiry floor (evidence file).
- **Doc maturity, dimension by dimension.**
  - Covered: Definition, Label, Alignment, LinkageCoverage, LossLedgerRow, CompetencyRationale
    and WorkedInstance.
  - CompetencyRationale is witnessed by `gmeow:DoxasticState` in the affect slice's competency
    file, which carries `cqRationale`.
  - WorkedInstance is witnessed by, for example, `gmeow:accepts` in examples.
  - Missed: UsageAdvice (21 terms), Example (21), ScopeNote (43), FixturePair (no epistemics
    term appears in both a `conformance-fixtures/` and a `counter-examples/` file) and
    RealizedState (no realized-state table in docs.md).
  - The two new terms carry label, definition, usage advice, example and scopeNote, so the miss
    counts are unchanged (21, 43 and 21 before and after; terms 73 to 75).
- **GMN-1 coverage.** Only the codec measures it. The example adds a `rdf:reifies <<( … )>>`
  triple term and `https://example.org/reified-evaluation/` IRIs. Triple-term examples already
  exist in slices whose floor is 1.0 and which pass on main: affect (`background-moods.ttl`,
  `word-tone-readings.ttl`) and places (`qualitative-placement.ttl`). No regression is expected,
  but it is not measured.

No floor fails, so this section has no REQUIRED finding.

## (4) Ruling on the functional form

**How the repository consumes the characteristic.**

- **Validation.** `derive_validation_shapes` gives a property-scoped `sh:maxCount 1`. A
  reifier linked to two tokens is reported.
- **OWL projection.** The characteristic is projected to `owl:FunctionalProperty`
  (`reason/mod.rs` lowering table, `owl_char!("functionalProperty", …)`).
- **Reasoning.** The native reasoner does not assume unique names (`dl.tests.rs` 1615-1618,
  2379, 3472). A functional property with two named values derives equality between them,
  through the rule `dl:functional-equality` (`schema.rs` 665-670; `equality_tests.rs` 112). It
  does not report a clash unless the two values are provably different.
- **Effect.** In the reasoned closure, a reifier wrongly linked to two claim tokens would make the
  two assertion acts one individual. The validation cap reports the same data. Any reasoning-only
  consumer would see the merge silently.

**Precedent.**

- The inquiry slice landed this week with "one reason per record is a validation constraint, not
  a functional axiom, because a functional axiom would have a reasoner infer two reasons on one
  record to be the same individual". There the values are a closed set of reason individuals,
  and merging two of them collapses distinct vocabulary members.
- In this slice, `gmeow:evaluates` (ClaimEvaluation to ClaimToken, an Activity) is functional by
  the same record form. The maintainer's decision for this issue explicitly kept it so
  ("`gmeow:evaluates` stays functional"). `knowerAgent`, `knownProposition` and the other
  relator roles are functional in the same way.
- The bridge has the same shape as `evaluates`: one subject, at most one assertion act.

**Ruling.** Acceptable as authored, and not a required change:

- AGENTS.md section 2 lists `logic:functionalProperty` among the declarative checks a slice
  authors in `logic:`.
- The slice's own `evaluates` precedent, kept by the maintainer's decision for this very issue,
  uses the same form with the same equality consequence.
- The merge arises only from data that breaks the stated precondition, and validation reports
  that data.

The inquiry decision is a distinct case: a closed set of value individuals, which a merge would
corrupt.

**Recommendation (OPTIONAL).** If the maintainer wants a breach never to merge two acts in a
reasoned closure, convert the bound to the uniqueness `logic:Constraint` form. Apply it to
`gmeow:evaluates` and `gmeow:putForwardByClaim` together, so the two sibling bridges keep one
semantics. That revisits the decision for `evaluates`, so it is the maintainer's call, not this
branch's. Short of that, one sentence in the bridge's scopeNote would make the consequence
explicit: a second token on one reifier is rejected by validation and, under reasoning, would be
identified with the first.

## Findings

No REQUIRED findings.

OPTIONAL:

- **O1 (functional semantics).** See (4): the bound is functional with equality semantics under
  reasoning. Keep it consistent with `evaluates`, or change both by maintainer decision.
- **O2 (inherited record boilerplate).** The record's useWhen says "the native coherence gate must
  enforce that gmeow:putForwardByClaim names exactly one value". Under no unique-name assumption,
  reasoning identifies two named values rather than reporting them, and functional means at most
  one. The validation cap is what reports a second value. The text is shared with the
  neighbouring characteristic records (18 in this module), so it is a repository-wide wording
  defect, not one this branch introduced.
- **O3 (DocMaturity has no margin).** The epistemics slice sits exactly on its floor (7/12).
  - Any future epistemics term that misses a dimension the slice still covers (definition or
    label) would drop it to 6/12 and fail.
  - Adding a pair of fixtures for one epistemics term would raise it to 8/12: one
    `tests/conformance-fixtures/` file plus a counter-example naming the same term (FixturePair).
- **Carried, unchanged.** Earlier O1 and O2 from the 5b14a825d review (wording of the minted
  reifier as "shared", and "behind a reifier"). Earlier F3 to F5 (the displayable sentence, which
  belongs to the display work; closed-world-only refusal; `rdf:Statement` typing).

## Not verified (no execution evidence obtainable in this session)

- The two example-conformance cells (the conforming example with no findings; the
  counter-example raising only `shacl.ClassConstraintComponent` from
  `gmeow:ClaimEvaluation-shape`).
- The competency question's single row, and the two structural cells, under the real harness.
- PurRDF parsing of the triple-term example and counter-example. The author's `rapper` run used a
  placeholder.
- `foreign_domain_tests.rs` passing, and the absence of `rdf:Statement-shape` in generated SHACL,
  ShEx and model projections.
- Every slice-quality value above. They come from shell mirrors. ProseQuality and DocMaturity
  reproduce the committed floors exactly at origin/main; TranslationCoverage was validated on
  another slice. No gate was run.
- axisGmn1Coverage.
- `make validate` annotation completeness, `make reason-verify`, and the native coherence gate over
  the merged ontology.
