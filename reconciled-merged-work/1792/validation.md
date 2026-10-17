# Acceptance index

## Current main reconciliation

The implementation and benchmark lane are already present on main fb73513b8,
introduced in eca80ae00 and integrated by PR 1802. This worktree has fast-forwarded
to current main and is clean. No duplicate implementation PR is needed. Main's
newer checked PurRDF calls, boxed intrinsic witnesses and producer-obstruction
evidence are retained. The old development copy is preserved in
pre-sync-unstaged.patch, pre-sync-staged.patch and the exact stash named in
pre-sync-stash.txt. It was not reapplied over the integrated implementation.
The first stash attempt refused the intent-to-add source_support file and the
merge refused all edits; no merge occurred. Removing only that index marker let
the scoped preservation succeed before the fast-forward.

Current native/case-split regressions passed all 21 selected cases in
raw/main-native-regressions.log. Current-main native RDF rule/statement and
production-profile GTS controls also passed all ten selected cases in
raw/main-native-rule-regressions.log. Neither execution produced the corpus.
The refreshed helpers gate failed in raw/main-helper-hygiene.log: actual duplicate
string/diagnostic/default-shim groups, stale debt groups and fixed-hasher/frame-le/
hash-domain overages are owned by issue 1834. No ceiling was raised. Source
implementation is merged; full qualification remains NOT MET.
The earlier benchmark measurements below remain historical measurements of their
recorded profile/lock/source, not a new speed claim for the updated dependency API.
The source-leaf walk, checked proof-position index and immutable batch lookup are
present in production callers. Shared gate qualification still depends on the
generation/fixture repairs in PR 1830; no full-gate pass is asserted.

## Historical development evidence

Current source: immutable per-batch proof lookup plus the existing native evidence
owner's checked append-only proof positions. Public ledger serialization and the
reasoning/admission verdict policy are unchanged. The following records describe
the original isolated development snapshot, before the main reconciliation above.

| Criterion | Evidence | Current result |
| --- | --- | --- |
| Baseline, same synthetic class operation, normal bench profile | raw/class-evidence-baseline-v2.log; baseline-harness.diff | 1250: 8.6553s [7.5030,10.149]; 2500: 45.919s [40.800,50.819]; 5000: 295.94s [267.86,324.71] |
| Candidate, same harness/profile/lock | raw/class-evidence-candidate-small.log | 1250: 0.50117s [0.48589,0.51528]; 2500: 1.3018s [1.1920,1.4129]; 5000: 1.6917s [1.6213,1.7584] |
| Complete size series | raw/class-evidence-candidate-large.log; old baseline terminated successfully before candidate 5000 began | PASS for the synthetic benchmark contract |
| Preserve source/proof/scope behavior | raw/source-lookup-focused.log, nextest 23926bfa-7b41-4b49-87c1-baaa5030d028 | 20 passed, 2197 skipped, 0.056s execution |
| Native original source completeness | Every descriptor requires an original onProperty proof; actual class outcome must engage and its retained proof validate | Executed in both timing series |
| Lint | raw/source-lookup-lint-v2.log | Passed on the final benchmark filter parameter too: raw/source-lookup-lint-final.log |
| Helper ownership | raw/source-lookup-helpers.log | FAILED: old base has 713 fixed-hasher/27 list copies vs ceilings 705/19; integrate actual shared-home repairs, never raise ceilings |
| Corpus purity | Old base still contains known forbidden authored-corpus tests | Integration of combined repair required; no false acceptance |
| Independent review, full qualification, PR and merge | No qualifying current evidence | Open |

Ratios of the completed point estimates are about 17.3x, 35.3x and 174.9x. These describe
this synthetic class operation, including proof validation, not the complete
pipeline. Each case uses ten Criterion samples. The baseline completed successfully, and its complete source/profile/harness log
is retained. The candidate's 1250/2500 filter wrote distinct case directories
while baseline 5000 ran; candidate 5000 began only after the baseline terminated. No resource,
compiler, profile, feature or lock override was used.

Commands: make bench-class-evidence for baseline; make bench-class-evidence-build
for candidate compilation; make bench-class-evidence
CLASS_EVIDENCE_FILTER='class_restrictions/descriptors_(1250|2500)$' for the smaller candidate cases; make bench-class-evidence
CLASS_EVIDENCE_FILTER='class_restrictions/descriptors_5000$' for the final case. The filter is a first-class Make input; its default remains the
complete class_restrictions group.

The initial baseline attempt refused a missing explicit default-world selection
before timing. That is a failed harness attempt (raw/class-evidence-baseline.log),
not a measurement. The corrected settled harness was compiled with unchanged
origin/main core before the lookup implementation was applied. Historical pending
status text is superseded by this index; raw logs remain attributable.
