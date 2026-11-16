PASS at 44c4e0d9a04a033546d3241e085432e4f05446a7 (after PR body change, re-read 2026-11-16 00:18 CST; the original verdict below was BLOCKED)

# Delivery review: reviewed-coverage golden refresh at 44c4e0d9a

The committed file is correct. The block is the PR text only: it closes the issue while one of the issue's three acceptance criteria is not done and not mentioned. No change to the commit is needed.

## Scope

- Worktree `/home/paudley/Active/gmeow-ontology/.worktrees/2836-reviewed-coverage-golden-on-main-is-out`, branch `paudley/2836-reviewed-coverage-golden-on-main-is-out`.
- HEAD is 44c4e0d9a04a033546d3241e085432e4f05446a7. It equals `origin/paudley/2836-...` and the PR head (`gh pr view 2840 --json headRefOid`).
- The diff is one commit on base 0e4e85952. It changes only `crates/docs/tests/fixtures/reviewed_coverage_golden.json` (+1/-1 lines, a single-line JSON file).
- Requirement: issue 2836, read verbatim with `gh issue view`. The issue has no comments.

## Evidence (all read-only; no cargo, make, producer or gate was run)

1. **What the refresh target runs.** The Makefile target `maint-refresh-reviewed-coverage-golden` (Makefile:1322-1323) runs `cargo run -p gmeow-docs-model --example refresh-reviewed-coverage-golden`. The example (`crates/docs-model/examples/refresh-reviewed-coverage-golden.rs`) calls `reviewed_coverage::reviewed_coverage_map(repo_root())` and writes `purrdf_lex::json::record::to_vec(&map)` plus a newline to `golden_path()`.
   - `reviewed_coverage_map` (`crates/docs-model/src/reviewed_coverage.rs:154-162`) iterates `ontology_catalogs::discover(root)`.
   - `discover` (`crates/docs-model/src/ontology_catalogs.rs:207-249`) reads:
     - `slices/<group>/<slice>/i18n/*.po`
     - each such slice's `manifest.ttl`, which must declare exactly one `gmeow:Slice` owner or discovery refuses
     - the DSL units' `dsl/mappings/vocabulary.ttl` and `dsl/tests/vocabulary.ttl` (parsed to admit the owner), `dsl/mappings/catalog-families.ttl` (existence) and `dsl/{mappings,tests}/i18n/*.po`
   - It reads nothing else: no generated tree, no producer, no network.
2. **Determinism.** Sources are sorted by path (`ontology_catalogs.rs:247`, `i18n_compile.rs:785`). The map is a `BTreeMap` and each value comes from a `BTreeSet` (`reviewed_coverage.rs:140-151`). Keys are root-relative (`relative_key`). There is no time, host or absolute-path data. Checks on the committed file:
   - `jq -c .` reproduces it byte for byte.
   - Every array is sorted and unique, and the keys are sorted.
   - There are no `/home` or backslash bytes.
3. **Same code path as the consumer.** `corpus_reviewed_coverage_matches_frozen_golden` (`crates/docs/tests/po_parser_parity.rs:50-67`) compares the golden with `reviewed_coverage` from the authenticated `stage-conformance` artifact `pipeline/reviewed-translation-observation.json`. That artifact is written by `crates/pipeline/src/stages/conformance/reviewed_translations.rs:15-27` through `gmeow_docs::reviewed_coverage::observe(input.root)`.
   - `observe` (`reviewed_coverage.rs:87-118`) and `reviewed_coverage_map` use the same `discover`, the same `parse_po(text, false)`, the same language resolution (`language_from_po` then the file-stem fallback), the same `reviewed_keys` filter (`counts_as_reviewed_coverage` = not fuzzy, non-empty, passes the integrity guard; then `split_once('|')` and `expand_predicate`) and the same `relative_key`.
   - The only differences:
     - `observe` also parses the auxiliary catalogs (`tests/fixtures/i18n/{fr,zh}.po`; `i18n/ontology-docs-templates.*.po` does not exist). These are kept out of `reviewed_coverage`.
     - The order of the language parse and the entry parse differs; both propagate errors.
   - Neither difference can make the two maps differ when `observe` succeeds. The synthetic unit test at `reviewed_coverage.tests.rs:6-38` pins this equality.
   - Conclusion: the refresh example and the producer compute the same map by the same functions. I found no divergence in discovery, filtering or key spelling.
4. **Catalog count.** All three listings give the same 190 paths as the golden's key set (`diff` empty for each):
   - `ls slices/*/*/i18n/*.po dsl/mappings/i18n/*.po dsl/tests/i18n/*.po`
   - `git ls-files` with the same patterns
   - `find slices dsl -name '*.po'`
5. **Content against the catalogs, whole corpus.** For every one of the 190 catalogs I compared two sets:
   - Set A, from the catalog: the distinct msgctxt values, with a leading-space-tolerant parse and the `expand_predicate` CURIE expansions for rdfs, skos, dcterms/dct and gmeow.
   - Set B: the golden's list for that catalog.

   Exactly 6 entries differ, all present in the catalog and absent from the golden, and each is one the integrity guard (`i18n.rs:53-93`) rejects:
   - `gts/i18n/{fr,zh}.po` `evidenceCorpusStale|skos:example`: msgstr equals the English msgid.
   - `pipeline/i18n/{fr,zh}.po` `flowReadsHandle|skos:example`: same.
   - `math/i18n/{fr,zh}.po` `RHat|rdfs:label`: msgstr "R-hat" equals msgid.

   No catalog has an empty msgstr outside its header, and no catalog has a fuzzy flag. Nothing in the golden is missing from its catalog.
6. **Twelve hand-checked added keys.** Each has a msgctxt present once, no fuzzy flag and a real translation:
   - math/zh `commensurableWith|useWhen`
   - logic/fr `characteristicSort|scopeNote`
   - ai/fr `chunkSpanSourceContext|definition`
   - gts/zh `MediumDictionaryDigestFormConstraint|label`
   - slice-quality-rubric/zh `qualityRecordFingerprint|howToUse`
   - dsl/mappings/fr `opDate|howToUse`
   - dreaming/zh `wakingOverrideText|label`
   - query-program/fr `AppliedOrderMatchedWitnessLaw|useWhen`
   - civil-dating/fr `civilDateFrame|definition`
   - sources/zh `SourceSnapshot|example`
   - documents/zh `epubTocPath|example`
   - reference-navigation/fr `bibliographicPublicationDateText|useWhen`

   The checked catalogs are a mix of changed and newly added ones.
7. **Six hand-checked removed keys.** Each is absent from its catalog:
   - slice-quality-rubric fr and zh `pcc-epistemics-bfo|label` (the subject is gone from both)
   - slice-quality-rubric/zh `reloc-organization-to-temporal-week-cycle|example`
   - logic/fr `candidateScopeFunctionality|useWhen`
   - logic/zh `ContextJournalPositionConstraint|logic:message` (the term remains, its message entry is gone)
   - affect/zh `modelIdentifierFunctionality|howToUse`

   In total, 128 key pairs were removed, across affect, slice-quality-rubric and logic only, and 35484 were added.
8. **Size of the move.** On main's first-parent history the last full refresh of the golden was 106ddaad8 (2026-10-21: 148 catalogs, 28027 keys). e32f90c96 and 0e4e85952 then made partial edits (148/28061 and 150/29085). Each of the 20 slices behind the 40 added catalogs was added on main between 2026-10-29 and 2026-11-15, for example agency-records 77168fdb6, dreaming 838fc37b2, query-program 4e868854c and evidence 125fc8168. That explains the change from 150 to 190 catalogs and 29085 to 64441 keys. Of the 148 catalogs present in both versions, 70 changed counts.
9. **Main since base.** After `git fetch origin main`, origin/main is bae740871, five commits past 0e4e85952. `git diff 0e4e85952 origin/main` touches no `.po`, no `manifest.ttl`, no DSL vocabulary, no `reviewed_coverage`, `i18n_compile`, `i18n` or `ontology_catalogs` source, and not the golden; its slice changes are confined to `slices/core/quality` (docs, examples, tests). The golden is not stale at origin/main as of this review.
10. **Logs** (`.stage/issue-2836/`):
    - `refresh.log` names pwd, HEAD 0e4e85952, a clean status, the date, `CMD: make maint-refresh-reviewed-coverage-golden`, the cargo-wrapper mtime and sha256 before and after (equal), the example's output line, and `EXIT=0`. The status comes from `run.sh`'s unpiped `$?` under `flock`.
    - `commit.log` shows the staged golden only, all hooks run (no bypass), commit 44c4e0d9a, and `EXIT=0`.
    - `old.json` is byte-identical to the golden at 0e4e85952.
11. **Text.**
    - The commit message (`msg.txt`) has no issue or PR number. Its two factual sentences are true (counts verified with jq).
    - `prose-check` passed on `msg.txt` and `pr-body.md` (exit 0).
    - The live PR body equals `pr-body.md` apart from a trailing newline.
    - PR claims checked and true:
      - 40 catalogs added and none removed.
      - The run was done under the build lock.
      - The equality test was not run, and cannot run without the producer artifact.
      - Hosted runs are red before the rust shards: main CI runs 43786412742 (bc4560473) and 43774087248 (e32f90c96) failed at `generation`, `fixture-prefix` and `quality`, with `rust` skipped.
      - `main` has not changed a translation catalog since the base.

## Findings

### REQUIRED

1. **The PR closes the issue with acceptance criterion 3 unmet and unlisted.** PR 2840 body, last line: "Closes #2836." Issue 2836's acceptance has three boxes. The third is: "The open branches that touched the file merge `main` and regenerate, so each carries only its own keys."
   - That work can only happen after this PR lands. The PR body neither does it nor lists it under "Not shown", yet merging will auto-close the issue.
   - At this review, `origin/paudley/2727-displayable-is-not-scoped-to-a` and `origin/paudley/1896-ground-evidence-and-trust-strengths` carry their own edits to the golden. Only `origin/main` was fetched, so other remote refs may be stale; the issue says three branches.
   - Smallest correction: edit the PR body (no commit change). Replace `Closes #2836` with a non-closing reference, and add a line under "Not shown" saying that criterion 3 is outstanding, naming those branches. Then close the issue only after each branch has merged `main` and regenerated. The alternative is to keep the closing keyword and complete criterion 3 before the merge, which the criterion's wording ("merge `main`") makes impossible.

### OPTIONAL

1. **PR body wording, "What changed".** The parenthetical "(reading each slice `manifest.ttl` only to name the catalog owner)" understates what discovery reads.
   - Discovery also parses `dsl/mappings/vocabulary.ttl` and `dsl/tests/vocabulary.ttl` to admit the DSL owners, and checks that `dsl/mappings/catalog-families.ttl` exists.
   - The manifest parse also refuses a slice that does not declare exactly one owner.
   - The conclusion, that the target runs no producer or pipeline stage, is true.
2. **Spot-check method.** `check.md`'s six spot checks test "non-empty, not fuzzy" but not the integrity guard, which is the third part of `counts_as_reviewed_coverage`. The whole-corpus comparison in Evidence 5 covers this; the check list could say so.
3. **Catalog hygiene seen while reviewing (outside this issue's contract; reported, not required here).**
   - `slices/grounding/math/i18n/{fr,zh}.po` each hold 74 contexts twice, once with a CURIE predicate and once with the full IRI. An example is `math/BasisBlade|rdfs:label` at fr.po:2277 and again with the full IRI at fr.po:9928.
   - `slices/grounding/lang/i18n/fr.po:187` has a msgctxt line with a leading space.
   - Three example or label entries copy the English text and are excluded by the integrity guard.
   - None of these changes the golden's correctness.

## What I read

- Issue 2836 (no comments) and PR 2840 body, files and checks.
- The worktree's `AGENTS.md` and the rust-work skill.
- Makefile:1322-1323.
- `crates/docs-model/examples/refresh-reviewed-coverage-golden.rs`
- `crates/docs-model/src/reviewed_coverage.rs`, `reviewed_coverage/paths.rs` and `reviewed_coverage.tests.rs`
- `crates/docs-model/src/ontology_catalogs.rs`, in full
- `crates/docs-model/src/i18n_compile.rs`: `expand_predicate`, `parse_po`, `language_from_po`, `counts_as_reviewed_coverage`, `directory_entries`, `slice_directories`
- `crates/docs-model/src/i18n.rs:53-146`, the integrity guard
- `crates/docs/tests/po_parser_parity.rs`
- `crates/pipeline/src/stages/conformance/reviewed_translations.rs`, `conformance.rs` around lines 890-915 and 1040-1075, and `fixture.rs:957-975`
- All 190 `.po` catalogs, by mechanical msgctxt extraction
- `.stage/issue-2836/{refresh.log, commit.log, check.md, msg.txt, pr-body.md, run.sh, old.json}`
- Git history of the golden and of the added catalogs
- origin/main at bae740871

## What I did not read or run

- The internals of `purrdf_lex::json::record` (only its output form).
- The `is_english_prose_token`, `ascii_words` and `is_technical_invariant` bodies: the six guard exclusions were inferred from msgid equal to msgstr, not executed.
- The pipeline's `StageInput.root` construction: I assumed it is the checkout root.
- Branch refs other than origin/main were not refetched.
- No cargo, make, producer, gate or test was run, by instruction.

## What nobody has shown

- Neither locally nor in CI has `corpus_reviewed_coverage_matches_frozen_golden` compared this golden with a producer's `stage-conformance` observation. On main the `rust` job is skipped because the producer jobs fail first.
- Byte identity of the committed blob to the example's output rests on the log sequence: a clean tree before the run, then only the golden staged at commit. My corpus comparison reconstructs the content with shell parsing, not the Rust parser, so it confirms the key sets, not the exact Rust execution.
- Acceptance criterion 3, regenerating the golden on the other branches, has not been done.

## Re-read after PR body change (2026-11-16 00:18 CST, from `date`)

Head is unchanged at 44c4e0d9a04a033546d3241e085432e4f05446a7: the local HEAD, `origin/paudley/2836-...` and the PR's `headRefOid` all agree. I re-read the live body with `gh pr view 2840 --json body,closingIssuesReferences`.

- **REQUIRED 1 is resolved.**
  - `closingIssuesReferences` is `[]`.
  - The body has no closing keyword (close/closes/closed, fix/fixes/fixed, resolve/resolves/resolved) before `#2836`. It now reads "Refs #2836; this does not close it", and the phrase "close it" carries no issue reference, so GitHub does not link it.
  - A new section, "Still open on the issue", states criterion 3 and names `paudley/2727-displayable-is-not-scoped-to-a` and `paudley/1896-ground-evidence-and-trust-strengths`.
  - I checked this against live refs: `git ls-remote --heads origin` gives 2727 at 1d39b88d5 and 1896 at c803c4792, which equal the local remote-tracking refs. Of every live remote head ahead of `origin/main` (bae740871), only those two and this branch change the golden (`git diff --name-only origin/main...<sha>`). None of the open PRs (2839, 2835, 2832) changes the golden or a `.po`.
  - The criterion-3 list is therefore accurate and complete.
- **Verdict at this head: PASS.** The commit was already judged correct, and nothing in the PR body now auto-closes the issue or omits an acceptance criterion.

### OPTIONAL (text accuracy; none blocks)

1. **"the only open branches that change a translation catalog" is not quite true.** The live remote branch `paudley/1761-classify-extracted-test-modules-outside` (7420921ac, 2026-10-29, no PR) changes `slices/core/pipeline/i18n/{fr,zh}.po`. It does not change the golden, so the criterion-3 list is unaffected. Smallest fix: say "the only branches that change this file", or drop the catalog clause.
2. **The discovery sentence was added, not corrected.** "What changed" still says "(reading each slice `manifest.ttl` only to name the catalog owner)", and the new sentence in "Still open on the issue" contradicts that "only". Smallest fix: delete the parenthetical in "What changed".
3. **"Three open branches each rewrote this file independently" ("Why")** now sits beside "Two such branches exist now". Smallest fix: drop "open", or say that one of the three has since landed.

### What I did not do on this re-read

- I ran no cargo, make or producer command.
- I re-fetched no branch: I compared live heads by `ls-remote`.
- I edited nothing except this file and scratch scripts under `.stage/issue-2836/review-scratch/`.
