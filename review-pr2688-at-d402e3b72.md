VERDICT: PASS d402e3b72b4218893c99da1be7dc51070401867e

# Review of PR 2688 at d402e3b72 (issue 2685): the merge of main only

Scope: the single merge commit `d402e3b72` (parents `98bf53bad`, the head passed in `review-pr2688-at-98bf53bad.md`, and main `d8af1cd0a`). HEAD of `.worktrees/2685-vocabulary-a-transitive-partial-order` and `gh pr view 2688` both equal `d402e3b72`. Nothing was built; `msgfmt -c -o /dev/null` and the catalog comparison script were the only programs run (both read-only).

## The standpoint side of the merge is exactly main's sibling term

`git diff 98bf53bad d402e3b72 -- slices/core/standpoint` is 4 files, +73/-0, the same shape as main's `git diff f71169a78 ef77c8870 -- slices/core/standpoint` (`ef77c8870` is an ancestor of the merge).

- `module.ttl` and `docs.md`: the added lines are byte-identical to main's added lines (compared as line sets with `-`/`+` markers).
- `fr.po` and `zh.po`: the 24 added lines are identical to main's except where the blank separator line sits. Main put it before its six entries and the merge puts it after them, because the block now sits before the `moreEntrenchedThan` entries rather than after the previous ones. Reading the hunk at `fr.po:460-487` shows six complete entries with blank lines on both sides.
- Removed lines in the standpoint slice: 0. So none of this branch's `moreEntrenchedThan` material (term, irreflexivity record, three constraints, docs anchor, catalog entries) changed.

Cross-check from the other side: for each of the branch's six files (`constraint_shapes.rs`, `constraint_shapes.tests.rs`, and the four standpoint files), the changed lines of `git diff d8af1cd0a d402e3b72` (the branch relative to new main) equal those of `git diff f71169a78 98bf53bad` (the branch relative to its old base). In both diffs those are the only files the branch changes, so the merge dropped nothing of either side.

## Conflict markers, duplicates, catalogs

- No `<<<<<<<`, `=======` or `>>>>>>>` line in `docs.md`, `module.ttl`, `fr.po` or `zh.po`.
- No duplicate `msgctxt`+`msgid` pair in either catalog (awk pairing, 0 duplicates). `msgfmt -c` accepts both. Its only warnings are three missing header fields, which come from the header and not from this merge.
- Comparison script (outside the repository), both catalogs against `module.ttl`: 147 entries each (141 before plus main's 6), 28 branch literals matched, 0 problems, and 0 catalog `msgid` without an English literal.

## The two terms in `docs.md`

The terms table lists `madeWithinContextOf` ("no characteristic", never a sharpening) and `moreEntrenchedThan` ("irreflexive", a ranker's dated partial ordering, never derived from `sharpens`) on adjacent rows. The anchors follow in that order. Neither text mentions the other term. They relate different things: a reading standpoint to its frame, and a ranker's ordering of frames. Both refuse a `sharpens` link and both decline to be an order across parties. Nothing in one contradicts the other. The `madeWithinContextOf` anchor's "It is not transitive, symmetric or irreflexive" wording was optional finding O1 in the 2689 review, and it is main's text, not this merge's.

## Open point: does the merged tree change the `all_axioms_project` count?

No, by reading. The count stays at 24.

- `crates/pipeline/src/stages/constraint_shapes.rs` projects only `logic:PropertyCharacteristicAssertion` (irreflexive or acyclic sort), `RelatumDistinctnessAssertion`, named `AllDisjointClasses` carrying `logic:formalizes`, `ConditionalRangeAssertion`, `RoleCompositionExclusionAssertion` and `MediatedPropertyRequirementAssertion`. It does not read `logic:Constraint` or `logic:integrity` (grep finds neither). The branch's three `logic:Constraint` records are therefore lowered by `logic-compile`, not by this stage, and do not enter the count. The branch's one entry in the count is `MoreEntrenchedThanIrreflexivity`, already in the 24 and in the anchor list (`constraint_shapes.tests.rs:117-157`).
- Main's `madeWithinContextOf` declares no characteristic and no assertion record.
- Every line the merge adds under `slices/` and `ontology/` was searched for `Assertion`, `AllDisjoint`, `characteristicSort`, `irreflexive` and `acyclic`. The only match is a prose sentence in `docs.md`. The provenance additions that came in with main (`gmeow:Approval` and its properties) are `logic:Kind`, `logic:ObjectProperty`, `logic:DatatypeProperty`, `logic:Restriction` and `logic:ClosureEntry`, none of which this stage reads. The `git grep` total of the matching record spellings in slice modules is 937 at both `98bf53bad` and `d402e3b72`.
- Neither `constraint_shapes.rs` nor its test changes in the merge.

The test itself needs a fixture made by the producer and was not run, so only a producer run plus `make nextest` confirms 24.

## Findings

REQUIRED: none.
OPTIONAL: none new. The OPTIONAL list in `review-pr2688-at-98bf53bad.md` still stands.

## Limits (only a producer or hosted run can show)

`all_axioms_project` and every other gate on the merged tree; that both standpoint terms reason consistently together; the hosted generation jobs.

## Coverage

Read or computed at `d402e3b72`:
- `git log -1` of the merge (parents and message) and `gh pr view 2688` head.
- `git diff 98bf53bad d402e3b72 --stat` (18 files, all main's).
- Line-set comparisons of the four standpoint files against `git diff f71169a78 ef77c8870`.
- `fr.po` merge hunk 460-487 in full.
- Conflict-marker grep; duplicate-pair awk; `msgfmt -c` on both catalogs; the comparison script.
- `slices/core/standpoint/docs.md` 10-115.
- `crates/pipeline/src/stages/constraint_shapes.rs` 1-130 and a grep for `Constraint`/`integrity`; `constraint_shapes.tests.rs` 117-157.
- Grep of the merge's added slice and ontology lines for projecting record types; a type tally of the added provenance and logic lines.
- Six-file before-and-after comparison of the branch's own change against old and new main.

Not read: main's non-standpoint changes beyond the record-type grep (frontend Rust, `meta_findings`, `PIPELINE_SPINE.md`, provenance prose); the `zh.po` merge hunk beyond the comparison and duplicate checks.
