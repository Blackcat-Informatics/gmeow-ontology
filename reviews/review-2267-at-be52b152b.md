BLOCKED at be52b152b

# Delivery review of issue 2267 (finality vocabulary, memory-records) at be52b152b

## Scope

- Worktree `/home/paudley/Active/gmeow-ontology/.worktrees/2267-marks-on-an-agent-s-important-events`.
  `git rev-parse HEAD` = `be52b152b99cb7fbb655ffa347975edbe670292f`, as briefed; `git status` clean.
  The head is one commit ahead of `origin/paudley/2267-…` (62d745418), so it is unpushed.
- Merge base with `origin/main` is `279ce7e40`; `origin/main` is 3 commits ahead and touches neither the
  slice nor the golden. A trial `git merge-tree` of `origin/main` and HEAD shows no conflict.
- Whole branch `origin/main...HEAD`: 9 files, +1289/-25. Eight are under `slices/core/memory-records`;
  the ninth is `crates/docs/tests/fixtures/reviewed_coverage_golden.json`.
- Contract read verbatim from `stagectl issue 2267`: the body, the delivery map, and both comments. The
  binding item is the maintainer's 2026-11-15T00:32:16Z split decision and its three acceptance items.
  I found no recorded ruling that narrows a watermark to one producer's marks. The earlier review's
  "binding decision applied" line is a claim from that review; it is not in the issue, and
  `.stage/STATUS.md` records only "attribution and watermark scope" as review findings.
- Read-only. Run: `git`, `stagectl issue`, `rapper -c` on four Turtle files (all exit 0),
  `prose-check` on `docs.md` and `design/MEMORY-RECORDS.md` (exit 0), and inline read-only greps and
  perl. No make, cargo, producer, gate, reasoner or test run.

## What I read at be52b152b

- `slices/core/memory-records/module.ttl:1176-1216` (the tracePacketAt, TracePacket and ledger
  siblings), `1218-1417` (all added lines), and the `ClosureEntry` index of the whole file.
- `examples/trace-finality.ttl:1-68` (whole file).
- `tests/structural.ttl:1-168` and `tests/competency.ttl:1-95` (both new files, read whole).
- `docs.md` and `design/MEMORY-RECORDS.md`: the full branch diff (docs around 283-399, design 197-287).
- `i18n/fr.po` and `i18n/zh.po`: every entry changed in `62d745418..be52b152b`. Mechanically, all
  79 English literals of the 11 new subjects have a non-fuzzy, non-empty entry with an identical
  `msgid` in both catalogs. The golden holds 158 keys for those subjects (79 per catalog).
- `crates/slicetest/src/exec.rs:600-780` (structural execution, teeth check, example discovery),
  `crates/slicetest/src/dsl.rs:100-140`, `crates/slicetest/src/native_query.rs:110-140`,
  `crates/logic-compile/src/projections/reader_view.rs:1-80`. The reader view is additive, so the
  `logic:` patterns in the structural ASKs match the module as authored.
- `slices/grounding/logic/design/LOGIC-VALIDATION.md:195-240` (closure and required-path doctrine).
- Precedents: union-domain properties with global closed-world entries (environment-records, dreaming,
  query-program), the `rdf:reifies <<( … )>>` rule-body form (refinement-cues), and the
  `logic:characteristicSort logic:functionalProperty` form.
- Pins: `crates/docs/tests/lint_regression.rs:172-195` and its snapshot,
  `crates/docs/tests/po_parser_parity.rs` (golden consumer), `.config/nextest.toml:40-80`, and a
  search of `crates/` and `governance/` for files that name memory-records.
- Stage evidence: `.stage/issue-2267/r3-checks.sh`, `r3-checks.log` (0 bytes), `r3-commit.log`,
  `r2-checks.log`, and both earlier reviews.

Not read: unchanged catalog entries; the slice's other examples and terms; `generated/`; the PurRDF
SPARQL parser; the `logic:termDistinct` lowering; slice-quality rubric code.

## Earlier findings (review at 62d745418)

| Finding | State at be52b152b | Basis |
|---|---|---|
| R1: conformance file cannot run, because the module has no ontology header | Resolved by withdrawal | `tests/example-conformance.ttl` and the three counter-examples are deleted. No reference remains in the slice. The declarer requirement is still pinned: `structural.ttl:51-56` matches each `wasAttributedTo` restriction and its class-local closed-world entry (`module.ttl:1354,1360,1365,1374-1376`), so dropping any one fails that `must` ASK |
| R2: whole-ledger or producer-as-declarer wording | Done as asked (`examples/trace-finality.ttl:40-41`, `competency.ttl:13,24,43,53`, `module.ttl:1226,1239,1244` and both catalogs, `docs.md:384`, design 234 and 270-271). But the requested replacement, "that producer's frontier", removes the issue's frontier law from the shipped text. See new R1 | Diff `62d745418..be52b152b` |
| O1: French useWhen grammar | Fixed | New msgstr reads correctly |
| O2: conformance cells test existence only | Moot | File withdrawn |
| O3: `cqWatermarkDeclared` omits the declarer | Still open | `competency.ttl:18-22` |
| O4: maximum-cardinality sentence in the design note | Still open | See O2 below |
| O5: sortal | Withdrawn by me | `TraceMark` and `TracePacketLedger` use the same `SubKind` under `InformationObject` pattern |
| O6: "Until that comparison exists" | Acceptable | The ruling asks the design note to state the deferred laws and why. The sentence cites no tracker |

## REQUIRED findings

### R1. The frontier law named in the ruling has been replaced by a different law, and the watermark's meaning and discard condition no longer match the issue

The issue says the watermark makes marking final ("no mark will name it") and that a later-marking
derivation states a committed point "which the watermark never passes". The ruling moves this law to
the comparison issue: "every applicable late-producer frontier before advancement". So the frontiers
that gate a watermark belong to **other** producers, and there are several of them.

The branch ships a different model:

- `module.ttl:1225-1226`: a watermark finalizes only "the one producer named through
  traceFinalityProducer", and "nothing here checks … that the named producer's own frontier has been
  reached".
- `module.ttl:1239`: "A producer's watermark should not pass that producer's frontier".
- `design/MEMORY-RECORDS.md:270-271` lists the deferred law as "a watermark does not pass its
  producer's frontier". `docs.md:384` says the same.
- The phrase "every applicable late-producer frontier" is gone from the slice. A grep for "applicable"
  finds only the unrelated anchor-profile sentences.

Why this is a defect:

1. Acceptance item 2 asks the design note to state which laws move to the comparison issue. The note
   names a different law: one frontier of the same producer, not every applicable frontier of the
   late producers.
2. Under these definitions, the watermark and the frontier say the same thing about one producer. A
   watermark says "that producer will mark no such packet" (`module.ttl:1225`); a frontier says the
   point "up to which [that producer] has finished marking" (`module.ttl:1238`). The issue's
   distinction between them, a frontier as a gate that another producer sets on the watermark, has
   no expression left. The worked example shows the gap. `ex:cut` (memory service, packet one at
   09:00) and `ex:lateFrontier` (late deriver, point at 09:07) are built for the issue's reading, where
   the cut must not pass the late frontier. Under the shipped text the two records are unrelated. The
   late deriver has no watermark, so by `module.ttl:1226` the recorder can never treat this ledger as
   final.
3. The ruling preserves the issue's discard eligibility: an unmarked packet at or below the watermark,
   once old enough. The branch adds a condition the issue never stated, "only where the recorder holds
   a watermark for every producer it knows of" (`module.ttl:1230`, `docs.md:393`, design 284-286, and
   both catalogs). That changes the preserved condition rather than preserving it.

The comparison issue would have to redefine `TraceWatermark` and `TraceMarkFrontier` to author the law
the maintainer assigned to it.

This arose from the review loop. The first review offered a per-producer scope as option (a); the
second asked for "that producer's frontier". No operator ruling chose it. Smallest correction (the
primary decides, or records an operator ruling verbatim if a per-producer scope is wanted):

- Restore the issue's model. A watermark declares marking final for the ledger's packets at or below
  its cut. `traceFinalityProducer` on a watermark names the producer it belongs to, meaning the
  producer that issues it, as the acceptance wording "name the ledger and producer they belong to"
  reads. A frontier is a late producer's committed point, and the watermark must not pass any
  applicable frontier.
- State the deferred law as "a watermark does not pass any applicable late-producer frontier" in
  `module.ttl:1226,1239`, `docs.md:384` and design 270-271.
- Remove the added "holds a watermark for every producer it knows of" condition from 1226, 1230, 1279,
  `docs.md:359-363,393` and design 245-249,284-286.
- Update both catalogs; the golden keys do not change.
- If the operator instead rules for per-producer watermarks, the design note must still list the
  ruling's frontier law verbatim, and the frontier's definition must say what it adds beyond a
  watermark.

### R2. Nothing has executed the new slice-spec cells at any head, and this head's own check log is empty

- `.stage/issue-2267/r3-checks.log` is 0 bytes. The script `r3-checks.sh` exists, but it recorded
  nothing at be52b152b. The only recorded evidence at this head is the commit hook output in
  `r3-commit.log`: hygiene hooks and the issue-reference lint pass.
- No head has a slice-spec run, `make check`, or hosted CI result for the 9 structural assertions and
  3 competency questions. The previous blocking defect (the unrunnable conformance file) was found
  only by reading code, and these cells have never been observed to pass.
- None of the five `mustNot` assertions (`structural.ttl:60-152`) declares `gmeow:saFailWitness`, so
  even a green run shows only that the banned pattern is absent. It does not show that the patterns
  can match.

Correction: before PR or merge, run the gate path once (`make check`, or hosted CI on the PR).
Record the command, exit status and the slice-spec verdict for memory-records under
`.stage/issue-2267/`. If the run is red, fix it in the slice.

## OPTIONAL findings

- **O1. A give-up reads as if it must name packets, but `givenUpPacket` is optional.**
  `module.ttl:1252` ("and names the packets whose marks are given up"), design 253-254 and
  `docs.md:375` read as a requirement, while `module.ttl:1321` says "Optional and nonfunctional".
  Either say "may name", or add the required path. The issue requires a give-up "before the watermark
  may pass their packet", which suggests packets are part of a give-up.
- **O2. The design note's single-value paragraph (design 259-262)** says a maximum qualified
  cardinality on an object property "would make a second value the same individual". The same slice
  puts `logic:maxQualifiedCardinality 1` on `hasReferenceFrame` (`module.ttl:1201`), and
  LOGIC-VALIDATION treats cardinality as a reported obligation. Drop the clause or scope it to OWL
  Direct semantics.
- **O3. `cqWatermarkDeclared` does not return the declarer** (`competency.ttl:18-22`), now that the
  declarer is required and distinct from the producer.
- **O4. `saTraceFinalityNoComparison` guards only constraints that formalize the nine finality
  terms.** A time comparison authored with `logic:formalizes gmeow:tracePacketAt` would pass it.
  Acceptance item 3 holds by reading at this head: the two constraints use only `rdf:type`, the
  limited role and `logic:termDistinct` (`module.ttl:1391-1396,1411-1416`).

## What holds

- **Acceptance item 1 (structure).** All three classes and six roles carry label, definition,
  isDefinedBy, scope note, inline example, useWhen, avoidWhen and howToUse, with fr and zh entries.
  `examples/trace-finality.ttl` has one worked instance each. Required paths use `allValuesFrom` plus
  class-local `ClosedWorldClosure` entries, as `LOGIC-VALIDATION.md:220-229` prescribes:
  - watermark: ledger, producer and cut;
  - frontier: ledger, producer and point;
  - give-up: reason;
  - all three: `wasAttributedTo`.

  Requiring `wasAttributedTo` is outside the literal acceptance list. It follows the sibling
  `TraceMark` and Principle 9, and I do not count it as unauthorized expansion.
- **Item 3 (no comparison).** True by reading; see O4.
- **No hand-authored shapes, no `owl:` in the module, and no unqualified cardinality.**
- **Example validity by reading.** Each packet carries the required time and frame. `ex:pointKey` is a
  `TraceAnchor`, so it needs no frame. Each record satisfies its required paths.
- **Structural and competency cells by hand.**
  - `must` cells: `saTraceFinalityVocabulary` holds over the reader view, because
    `logic:subClassOf`, `logic:domain` and `logic:range` are authored and the view is additive.
    `RequiredRoles` holds and fails if any entry is dropped or its closure is opened. `SingleValue`
    matches the authored formula tree. `Example` holds over module plus examples, and none of the
    other example files introduces a finality record.
  - `mustNot` cells are false at this head.
  - The three competency questions each return exactly the one expected row.
- **Catalogs and golden.** 79 of 79 literals match the `msgid` in both languages. The golden carries
  79 keys per catalog for the new subjects. Keys are subject and predicate pairs, so the text changes
  in this commit need no golden change. The earlier review's full derivation at 62d745418 still
  applies.
- **Pins.** No slicetest, slice-quality, governance or ratchet file names memory-records except the
  golden. The `coverage_ratchet_baseline` insta snapshot has not changed since 2026-08-27, across 458
  later module changes on main, so it does not track live slice content. I did not establish why.
- **Process wording.** No issue or PR number, and no tracker or follow-up wording, in the added lines.
- **Syntax and prose.** `rapper -c` exits 0 on `module.ttl`, the example and both test files.
  `prose-check` exits 0 on `docs.md` and the design note. I could not run `prose-check` on the module
  literals or the catalogs: it needs a regular file, and this review writes no files other than
  itself.

## Not verified

- No slice-spec, validator, reasoner, producer, gate or CI run (see R2).
- The derived SHACL for the new closures and the lowering of `logic:termDistinct` under `not/exists`.
- Whether the coverage-ratchet snapshot runs on the default nextest profile.
- Translation fidelity of unchanged entries.

VERDICT: BLOCKED
