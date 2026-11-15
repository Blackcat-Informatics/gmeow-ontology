# Slice-quality floors: sources, inquiry and evidence

## Scope

This is a read-only check of committed slice-quality floors for three term changes that were never measured
against those floors. No gate, producer, cargo or reasoning command was run. Every command ran under
`flock /home/paudley/Active/gmeow-ontology/.stage/build.lock`.

| Slice | Before | After |
|---|---|---|
| sources (PR 2811, merged) | 1198be05c | origin/main 13396432e |
| inquiry (PR 2814, open) | merge base 862638ad8 | fb460b557 (worktree `.worktrees/2802-term-reasons-a-question-has-no-answer`) |
| evidence (issue 2800) | 1198be05c | fec97ae99, plus f807002e4 (current branch head, clean worktree) |

Floors are the `gmeow:AxisFloorCommitment` records `afc-<slice>-<axis>` in
`slices/core/slice-quality-rubric/module.ttl`. Those records are identical at 1198be05c and origin/main. For
these three slices, every axis with a floor has an explicit record, so no default applies. The only implicit
default is `axisGmn1Coverage = 1.0` on the three grounding slices (lang, logic and math). It is applied only
in `axis_floor_for`, `crates/gmeow-dev-cli/src/dev_slice_quality.rs` (around line 1774). Any other axis
without a record is unfloored and advisory. The gate passes when `measured + f64::EPSILON >= floor`
(`crates/slice-quality/src/gate.rs` 252-258).

## Evidence (shell mirrors, each validated against a committed floor)

The scratch scripts are in `.stage/reviews/`. Each mirror was checked by reproducing a committed floor
exactly:

- `scratch-prose-nt.sh` mirrors `prose_axis` (axes.rs 309-357) together with `prose.rs` and its cues. Check:
  evidence at 1198be05c gives 16/20 = 0.8, matching `afc-evidence-axisProseQuality`.
- `scratch-translation-nt.sh` mirrors `translation_axis` (axes.rs 1135-1447), `is_technical_invariant`,
  `translation_integrity_issue` and `counts_as_reviewed_coverage`. Check: inquiry at 81a79a09c, the commit that
  set the floor, gives 54 expected pairs and 6 reviewed per language. The score is (1 + 6/54 + 6/54)/3 =
  11/27 = 0.40740740740741, matching `afc-inquiry-axisTranslationCoverage`.
- `scratch-docmat-nt.sh` and `scratch-docmat-exists.sh` mirror the `DocMaturity` axis, which reads
  `docs-model` `term_coverage` and `slice_covered_dims` against the 12-dimension FULL intent. Check:
  evidence at 1198be05c gives 9/12 = 0.75, matching `afc-evidence-axisDocMaturity`.
- Documentation, shape migration and glyph optimality were read directly from `docs.md`, the `shapes.ttl`
  inventory and `slices/grounding/lang/module.ttl`.

Facts used by the DocMaturity mirror:

- A slice with no `gmeow:adoptionTarget` term always covers Alignment and LinkageCoverage. The reason is that
  every alignment cell carries a mapping set (model.rs 3923-3924).
- LossLedgerRow is always covered, because its presence equals its applicability.
- FixturePair, CompetencyRationale and WorkedInstance are existential. Adding terms cannot lose them.
- None of the three slices has a term with `gmeow:adoptionTarget`.

## Findings

### Floor table

| Slice | Axis | Floor | Before | After | Verdict | Terms to fix |
|---|---|---|---|---|---|---|
| sources | axisProseQuality | 0.8333333333333334 | 7/8 = 0.875 | 13/14 = 0.9286 | computed and holds | none required; `gmeow:sourceLocation` definition lacks a boundary cue (already at base) |
| sources | axisTranslationCoverage | 0.3333333 | 0.4222 (45 expected, 6/6) | 0.4902 (51 expected, 12/12) | computed and holds | none |
| sources | axisDocMaturity | 0.75 | 12/12 = 1.0 | 11/12 = 0.9167 | computed and holds | ScopeNote dimension lost: `gmeow:immutableAddress`, `gmeow:retrievedAt`, `gmeow:SourceSnapshot` have no `skos:scopeNote` (optional) |
| sources | axisDocumentation | 1.0 | 1.0 | 1.0 | computed and holds | none |
| sources | axisShapeMigration | 1.0 | 1.0 | 1.0 | computed and holds (no `shapes.ttl`) | none |
| sources | axisGmnGlyphOptimality | 1.0 | 1.0 | 1.0 | computed and holds (no candidate or denotation targets a sources term) | none |
| sources | axisGmn1Coverage | 1.0 | 1.0 (main gated) | not computed | not computable by reading | see GMN-1 note |
| inquiry | axisProseQuality | 0.8846153846153846 | 44/48 = 0.9167 | 55/60 = 0.9167 | computed and holds | at risk: new `gmeow:absenceQuestion` definition has no boundary cue |
| inquiry | axisTranslationCoverage | 0.40740740740740744 | 0.5359 (102 expected, 31/31) | 0.6619 (140 expected, 69/69) | computed and holds | none |
| inquiry | axisDocMaturity | 0.6667 | 9/12 = 0.75 | 9/12 = 0.75 | computed and holds | none (ScopeNote, FixturePair and RealizedState already missed at base) |
| inquiry | axisDocumentation | 1.0 | 1.0 | 1.0 | computed and holds | none |
| inquiry | axisShapeMigration | 1.0 | 1.0 | 1.0 | computed and holds (no `shapes.ttl`) | none |
| inquiry | axisGmnGlyphOptimality | 1.0 | 1.0 | 1.0 | computed and holds | none |
| inquiry | axisGmn1Coverage | 1.0 | 1.0 (main gated) | not computed | not computable by reading | see GMN-1 note |
| evidence | axisProseQuality | 0.8 | 16/20 = 0.8 | fec97ae99: 19/24 = 0.7917; f807002e4: 20/24 = 0.8333 | fails at fec97ae99; holds at f807002e4 | `gmeow:assertionCategory` definition (fixed at f807002e4) |
| evidence | axisTranslationCoverage | 0.3333333 | 0.3333 (no catalogs) | 0.4082 (89 expected, 10/10) | computed and holds | none |
| evidence | axisDocMaturity | 0.75 | 9/12 = 0.75 | 9/12 = 0.75 | computed and holds, with no margin | none; see at-risk note |
| evidence | axisDocumentation | 1.0 | 1.0 | 1.0 | computed and holds | none |
| evidence | axisShapeMigration | 1.0 | 1.0 | 1.0 | computed and holds (no `shapes.ttl`) | none |
| evidence | axisGmnGlyphOptimality | 1.0 | 1.0 | 1.0 | computed and holds | none |
| evidence | axisGmn1Coverage | 1.0 | 1.0 (main gated) | not computed | not computable by reading | see GMN-1 note |

### Evidence axisProseQuality: fails at fec97ae99, fixed at f807002e4

**REQUIRED at fec97ae99. Resolved at f807002e4 by the mirror.**

- The failing literal at fec97ae99 is the `skos:definition` of `gmeow:assertionCategory`. It carries no
  boundary cue (not, never, nor, cannot, rather than, as opposed to, instead of, unlike, distinct from).
- f807002e4 adds "It is not a rank, a trust level or a display flag, and it is distinct from
  gmeow:hasEvidenceClass, which grades one citation". The score becomes 20/24 = 0.8333, which holds.
- The fr and zh msgid entries for the edited literal match the module (18/18 in each catalog). Translation
  coverage is unchanged at 10/10 per language.
- The remaining prose misses predate this change: `coverageDepth`, `EvidenceSpan`, `sourceIndependence` and
  `sourceTier`, all with no boundary cue.

### Evidence axisDocMaturity: at the floor exactly

**OPTIONAL.** The evidence slice covers 9/12 both before and after: ScopeNote, FixturePair and RealizedState
are missed, and FixturePair and RealizedState were already missed at base. The score holds at exactly 0.75.

There is no margin. Any future evidence term that lacks a definition, label, usage advice or `skos:example`
loses one more dimension and fails the floor (8/12 = 0.6667).

### Sources axisDocMaturity

**OPTIONAL.** PR 2811 lowered the sources slice from 12/12 to 11/12. Three new terms have no `skos:scopeNote`.
The score still holds against 0.75. Adding a `skos:scopeNote` to `gmeow:immutableAddress`,
`gmeow:retrievedAt` and `gmeow:SourceSnapshot`, with fr and zh entries, restores 1.0.

### Inquiry axisProseQuality

**OPTIONAL.** The new `gmeow:absenceQuestion` definition has no boundary cue, and the score stays at 0.9167.
The floor allows at most 6 misses at the current 60 checks; there are 5 now. Two more misses would fail it.

Smallest truthful edit: add a sentence such as "It is not the question's answer and not a claim that the
property has no value", and update the fr and zh msgid and msgstr for that literal.

The earlier misses remain: `asks`, `tenureInquirer` and `tenureQuestion` have no boundary cue, and the
`QuestionTextLanguageViolation` example is not a worked triple.

### GMN-1 coverage: not computable by reading

The score is the GMN-1 codec's per-quad classification of module plus examples against the
`gmnDictV3` dictionary (`measure_coverage` in `crates/lang-bridge/src/gmn1_codec.rs`). Only running the codec
measures it. Bounded inference:

- The only IRIs new to that surface are under `https://example.org/assertion-category/`,
  `https://example.org/snapshot/` and `https://example.org/absence/`, plus the `rdf:` namespace.
- `https://example.org/` IRIs already appear in the examples of slices whose committed floor is 1.0 and that
  pass on main: affect and agency-records.
- A Turtle file carries no named-graph quads.

No regression is expected, but it is not measured.

## Verification limits

- No gate was run. Every value comes from a shell mirror or from reading the code.
- Each mirror reproduced one committed floor exactly (see Evidence), but none was compared against a gate
  run.
- The existential DocMaturity dimensions come from repository-wide `git grep` at the same revision:
  - FixturePair: a CURIE in both `tests/conformance-fixtures/` and `tests/counter-examples/`.
  - WorkedInstance: a CURIE in `examples/`.
  - CompetencyRationale: a `cellValueIri` in a competency file that carries `cqRationale`.
- The evidence and inquiry "before" DocMaturity values assume main passes its gate. The evidence mirror
  reproduces 0.75 exactly, which supports this.
- GMN-1 coverage was not computed.

Sources at origin/main and inquiry at fb460b557 hold every computable floor. Evidence holds every computable
floor at f807002e4, but at fec97ae99 it fails axisProseQuality (0.7917 < 0.8).
VERDICT: PASS
