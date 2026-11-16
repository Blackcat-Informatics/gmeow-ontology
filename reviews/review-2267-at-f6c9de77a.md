PASS at f6c9de77a

# Delivery review of issue 2267 (finality vocabulary, memory-records) at f6c9de77a

## Scope

- Worktree `/home/paudley/Active/gmeow-ontology/.worktrees/2267-marks-on-an-agent-s-important-events`.
  `git rev-parse HEAD` = `f6c9de77a5a3d764b9389232917dc0ced44410c8`, as briefed. `git status` is clean.
  The head equals `origin/paudley/2267-…`.
- HEAD is a merge commit. Its parents are `fc3a7119f` (the last reviewed head) and `e32f90c96`
  (`origin/main` as fetched locally). `git merge-base HEAD origin/main` = `e32f90c96`. I did not fetch, so a
  newer main is not shown.
- `git diff --name-only origin/main HEAD` lists 15 paths: 14 under `slices/core/memory-records` and
  `crates/docs/tests/fixtures/reviewed_coverage_golden.json`. No other path differs from main.
- The contract is `stagectl issue 2267` read verbatim: the body, the delivery map, the two later sections
  and both comments. The binding ruling is the maintainer's comment of 2026-11-15T00:32:16Z.
- The coder's notes under `.stage/issue-2267/` are claims. I read `r5-hand-eval.md` and
  `r5-golden-refresh.log` and used `r5-full-regen.json` only as a cross-check of my own computation.
- Read-only. Commands: `git` (read-only), `stagectl issue`, `rapper -c`, `prose-check`, `jq`, `grep`,
  `diff`, `comm`. No make, cargo, producer, gate, reasoner or test run. Nothing was written except this file.

## What I read at f6c9de77a

- `slices/core/memory-records/module.ttl`:
  - 1136-1437, read in full: the ledger and packet, all finality terms, the closures and restrictions,
    and all three finality constraints;
  - 570-598 and 1080-1093, the slice's existing multi-variable constraint forms;
  - a grep for `failureClass`, `logic:severity` and `violation*` in the slice (none present).
- `slices/core/inhabitation/module.ttl:1505-1530`: the precedent for an atomic consequent over
  bound variables.
- In the same slice:
  - `docs.md:270-420`;
  - `design/MEMORY-RECORDS.md:190-296`;
  - `examples/trace-finality.ttl` (whole);
  - `tests/structural.ttl` (whole);
  - `tests/counter-examples/finality-cut-outside-ledger.ttl` and `finality-constraint-compares-time.ttl`
    (whole).
- The `fc3a7119f..HEAD` diff of the module, tests, example, docs, design note and both catalogs.
- Catalogs:
  - every `+msgstr` of `fc3a7119f..HEAD` in `fr.po` and `zh.po`, read for meaning;
  - a mechanical `grep -cF "msgid <literal>"` for each of the 87 English literals that the branch adds
    or changes against `origin/main`, in both languages;
  - a reverse check that no replaced literal survives as a `msgid`;
  - a check for `fuzzy` flags and empty `msgstr`.
- Harness: `crates/slicetest/src/exec.rs:590-727`.
- Golden and its readers:
  - `crates/docs-model/src/reviewed_coverage.rs:1-162`;
  - `ontology_catalogs.rs:207-249`;
  - `i18n_compile.rs:493-496,767-787`;
  - `crates/docs/tests/po_parser_parity.rs:1-80`;
  - `.config/nextest.toml:55-79`.
- Pins:
  - `git grep` of the slice name and the finality term names under `crates`, `governance`, `.config`,
    `bench` and `tests`;
  - `crates/logic-compile/src/frontend/functional_carrier_ledger.txt` (header, line count);
  - the slice-quality floor golden TSVs;
  - the slice manifest;
  - `.deficiencies`.
- Previous review `review-2267-at-fc3a7119f.md` (whole).

Not read:
- the slice's other two examples (only grepped: neither declares a finality record);
- other subjects' catalog entries;
- PurRDF's evaluation of the lowered constraint;
- the lowering code for `logic:Constraint`;
- `translation_has_integrity`;
- `generated/`;
- the epistemics content that the merge brought in (only shown byte-equal to main).

## Earlier finding

| Finding (review at fc3a7119f) | State at f6c9de77a | Basis |
|---|---|---|
| R1: the text gave a false reason (no typed comparison) why the cut's ledger membership was unchecked | **Resolved, by authoring the check** | The membership clause is gone from every "without a typed comparison" list: `module.ttl:1226`, `docs.md:393-396`, design 285-289 and the `watermarkPacket` scope note at 1293. Each place now says that a constraint checks membership and that the check needs no comparison. A grep of the slice's Turtle, Markdown and both catalogs finds no remaining claim that membership is unchecked |
| Merge conflict in the golden | **Resolved** | See section 6 |

The earlier optional findings, as changed in this round:

- **O1, frontier "declares nothing final".** Resolved: "declares no ledger-wide finality and makes no
  packet eligible for discard" (1238, `docs.md:373-374`, design 248-249).
- **O2.** Resolved: `docs.md:288`, "a watermark may advance".
- **O3, give-up packets read as required.** Mostly resolved: the definition at 1251 and the howToUse at
  1256 now say "where the producer names/states them". The useWhen at 1254 still says "for named
  packets"; see O6 below.
- **O4.** Resolved: the frontier note (1239) and the give-up note (1252) now each say that time-then-digest
  order is unchecked.
- **O5, functional-axiom rationale.** Resolved: it is now scoped to the OWL projection (1384, 1404,
  `docs.md:384-386`, design 261-263, `structural.ttl:185`).
- **O6, the no-comparison ban covers only constraints that formalize finality terms.** Unchanged and
  still optional.
- **O7, the example ledger's attribution.** Resolved: the ledger is attributed to `ex:memoryService`
  (example 10, 37).
- **O8, translations.** French: "au niveau duquel ou en dessous duquel". Chinese: 之上 became 之后. See O7
  below.

## 1. Was authoring the membership check right?

Yes. It is inside the issue's contract and not an expansion.

- The issue (item 3) says: "The service records each packet it sees, with the time that orders it
  (`TracePacket`, `tracePacket`, `tracePacketAt`). Packets are ordered by that time, then by digest. A
  `TraceWatermark` says that marking is final for every packet at or below **one packet in that order**."
  The order is over the packets the service records in its ledger, so the cut is one of those packets.
  The branch's own `watermarkPacket` already said "The recorded TracePacket …" and "Avoid naming a packet
  that was not recorded" (1292, 1296).
- The ruling moves exactly "the typed time and digest comparison primitive, and the order laws that need
  it (actual time-then-digest order, the inclusive never-backward watermark, every applicable
  late-producer frontier before advancement)". Membership is none of these. What stays is "the
  structural vocabulary for finality, with no order law", with "axioms for what is structurally required".
  A join of three roles is structural and needs no comparison.
- Acceptance item 3 ("No constraint in this change compares times or digests") still holds. The new
  constraint uses `rdf:type`, `traceFinalityLedger`, `watermarkPacket` and `tracePacket` only.
- My previous review called this option "a scope addition". On the issue's own words ("one packet in that
  order") that was too strong. The option is implied by the issue and not excluded by the ruling.

## 2. `gmeow:TraceWatermarkLedgerMembershipConstraint` (`module.ttl:1418-1436`)

Node by node:

- Root (1431): `logic:quantifiedVariable` index 0 `memberWatermark`, 1 `memberLedger`, 2 `memberPacket`;
  then `logic:forall` into an implication node.
- Antecedent (1432-1435): `logic:and` of three atoms:
  - `rdf:type(memberWatermark, gmeow:TraceWatermark)`, with arg 0 the variable and arg 1 `termIri`;
  - `gmeow:traceFinalityLedger(memberWatermark, memberLedger)`;
  - `gmeow:watermarkPacket(memberWatermark, memberPacket)`.
- Consequent (1436): the atom `gmeow:tracePacket(memberLedger, memberPacket)`.

Encoded formula:

`∀w ∀l ∀p. (TraceWatermark(w) ∧ traceFinalityLedger(w, l) ∧ watermarkPacket(w, p)) → tracePacket(l, p)`

- **Intended rule.** Yes. The argument order matches `tracePacket`'s `logic:domain TracePacketLedger` and
  `logic:range TracePacket` (1173-1174), and its use in the example (`ex:ledger gmeow:tracePacket
  ex:packetOne`).
- **Form.**
  - It matches the slice's existing procedural constraints: multi-variable `forall`, positive
    conjunctive antecedent with a type guard, and range-restricted variables (570-598, 1089-1093).
  - The consequent is an atom over bound variables rather than an `exists`. `inhabitation/module.ttl:1519-1524`
    is the same form in a shipped constraint.
- **EL-safe or procedural.** Procedural is right. A cross-node equality of the ledger reached through two
  paths cannot be an EL-safe `logic:` axiom without role chains and nominals.
- **Failure class.** None of the slice's other 20 constraints carries `logic:severity` or a failure class,
  so the default applies. No finding.
- **Coat.** It has `rdfs:isDefinedBy`, `graphBoxRole`, label, definition, scopeNote, example, useWhen,
  avoidWhen, howToUse, `logic:formalizes gmeow:watermarkPacket` and `logic:message`. That is the same coat
  as the two sibling constraints, and it is fully translated (section 5).

Worked by hand:

- **`examples/trace-finality.ttl`, must conform.**
  - The only `TraceWatermark` is `ex:cut` (42-46): (w, l, p) = (`ex:cut`, `ex:ledger`, `ex:packetOne`).
  - `ex:ledger gmeow:tracePacket ex:packetOne` is asserted (36), so the consequent holds.
  - No other binding exists, and no other example file declares a watermark. **Conforms.**
- **`finality-cut-outside-ledger.ttl`, must violate.**
  - (w, l, p) = (`ce:cutOutsideLedger`, `ce:ledger`, `ce:unlistedPacket`) satisfies the antecedent.
  - `ce:ledger` lists only `ce:listedPacket`, so `tracePacket(ce:ledger, ce:unlistedPacket)` is absent.
    **Violates.**
  - The fixture's other finality constraint (two distinct cuts) does not fire. Its required paths are
    present: ledger, producer, cut and declarer.
  - It is not a clean "only this rule" witness against the full closed-world projection, because
    `ce:unlistedPacket`, `ce:listedPacket` and `ce:producer` are untyped. The derived class checks
    (`watermarkPacket`/`tracePacket` → `TracePacket`, `traceFinalityProducer`/`wasAttributedTo` →
    `Agent`) would also report it.
  - Nothing binds this fixture to a conformance run (the slice has no `tests/example-conformance.ttl`).
    It is only the fail witness of an ASK, and for that ASK it trips exactly the intended ban. See O4.

## 3. The structural assertions

- **`saTraceFinalityNoComparison`** (`structural.ttl:60-73`).
  - **Widening.** Two relations were added: `gmeow:traceFinalityLedger` and `gmeow:tracePacket`. They are
    exactly the relations of the new constraint that were not already allowed (`rdf:type` and
    `watermarkPacket` were), so this is the narrowest correct widening.
  - **Primary run, module alone.** Three constraints formalize finality terms. Their relations are
    {type, `watermarkPacket`, `termDistinct`}, {type, `frontierPoint`, `termDistinct`} and {type,
    `traceFinalityLedger`, `watermarkPacket`, `tracePacket`}. All are allowed, so the ASK is false.
  - **Fail witness.** `ce:watermarkTimeComparison` reaches `logic:termLessEqual` through `forall` and
    then `consequent`. That relation is not allowed, so the ASK is true and the ban still has teeth.
  - **Rationale.** It is still true in substance: the allowed relations are structural links or an
    identity test, and none is a comparison. The wording "the finality roles being limited" fits
    `traceFinalityLedger` poorly here, since it is a binding; see O3.
- **`saTraceFinalityLedgerMembership`** (`must`, `scopeModule`, 148-164).
  - It holds over the module: the root `forall`, the antecedent `and` with three distinct conjuncts on
    type, ledger and cut, and the consequent on `tracePacket`.
  - It can fail. It fails if the constraint is renamed, its `formalizes` changes, the consequent is
    removed or re-related, or any of the three conjuncts is removed.
  - It does not pin argument wiring: a reversed `tracePacket(p, l)` or a fresh variable would pass. See O5.
- **`saTraceFinalityCutListed`** (`mustNot`, `scopeModuleAndExamples`, fail witness, 166-174).
  - **Primary run.** The module plus all three example files gives one watermark, whose cut is listed, so
    the ASK is false. It can fail: an example watermark whose cut is unlisted makes it true.
  - **Teeth run, by `exec.rs:673-709`.** The harness unions the module and the witness only (no
    examples), applies the same projection, and requires the ASK to hold. The witness gives
    `ce:cutOutsideLedger` with no `ce:ledger tracePacket ce:unlistedPacket`, so the ASK is true and the
    ban is tripped. The fixture's header describes this correctly.
- **The other bans are unaffected.**
  - `NoFormulaOutsideConstraint`: the new `traceFinalityLedger` and `watermarkPacket` atoms are
    reachable from the integrity root through `forall`, `antecedent` and `and`.
  - `DerivesNothing`, `NoRuleBody` and `NotFunctional`: the change adds no matching triples.
  - `saTraceFinalityExample` is unchanged by the ledger re-attribution.

## 4. Text

Each changed sentence was checked against the issue and the ruling.

| Statement | Verdict |
|---|---|
| A constraint checks that the cut is a member of the declared ledger, "a plain join that compares no time or digest" (1226, 1230, 1293, 1423-1428; `docs.md:362-366`; design 263-266) | True |
| It checks listing only, not that the ledger lists every packet it observed or that the cut is at or below another packet (1424; docs 365-366; design 265-266) | True. It matches `TracePacketLedger`/`tracePacket`'s "no completeness" (1142, 1168) |
| Deferred and unchecked: order, never-backward, frontier respected, give-up first (1226, 1239, 1252; docs 388-395; design 276-289) | True to the ruling's list. The reason given (a typed comparison is needed) is now true of every listed item |
| "Listing of the cut packet in its ledger is not among these laws: it needs no comparison" (docs 395-396; design 287-289) | True |
| A frontier "declares no ledger-wide finality and makes no packet eligible for discard" | True. The issue gives discard eligibility only through the watermark |
| A give-up names packets "where the producer names them" | Consistent with the ruling's acceptance item 1 (a give-up names its reason). No overstatement |
| Functional-axiom rationale "in the OWL projection" | True, and now scoped |
| Example: "The memory service's ledger lists the three packets it saw" | Matches "The service records each packet it sees" |
| Design 271-274: "Another rejects any worked watermark whose ledger does not list its cut. Each of those bans carries a fail-witness fixture" | True (six bans, six witnesses) |
| Design 267-270: the ban allows "the type test, the finality roles, the ledger-membership link (tracePacket) and term distinctness" | Slightly broad: only three of the six finality roles are allowed. See O3 |
| `docs.md:290`: "The vocabulary computes no digest, performs no join, …" (older text, not changed) | Now sits beside "a plain join" at 364. It means the recorder's trace join. See O2 |
| Constraint scopeNote (1424): "A ledger that is silent about a packet fails a watermark naming that packet; the declarer lists the packet or names another cut." | The remedy can read as telling a watermark declarer to add a ledger row it did not observe. See O1 |

I found no remaining text that attributes something to the wrong cause, and no new overstatement against
the issue.

## 5. Catalogs

- **msgid equality.** I checked with `grep -cF "msgid <literal>"`, run now at this head. Each of the 87
  English literals that the branch adds or changes in `module.ttl` against `origin/main` has exactly one
  match in `fr.po` and exactly one in `zh.po`. That includes all eight literals of the new constraint and
  the eleven changed in this round.
- No literal replaced in `fc3a7119f..HEAD` survives as a `msgid`. Neither catalog has a `fuzzy` flag. The
  only empty `msgstr` is the header.
- **Meaning, French.** All 19 new or changed entries are faithful. This includes the new constraint's
  label, definition, scope note ("Elle ne vérifie que l'appartenance …"), example, useWhen, avoidWhen,
  howToUse and message, and the corrected `watermarkPacket` definition ("au niveau duquel ou en dessous
  duquel").
- **Meaning, Chinese.** All 19 are faithful:
  - the scope note "一项约束会检查截断数据包是否为所声明账本的成员，这是一个不比较任何时间或摘要的简单连接";
  - the constraint's "它只检查成员关系 …";
  - the message "必须是其所声明 traceFinalityLedger 的 tracePacket 成员";
  - the OWL-projection clause "而不是像函数性公理的 OWL 投影那样被推断为同一个数据包".
- No mistranslation. One wording is ambiguous; see O7.

## 6. The merge and the golden

- **Every key of `origin/main`'s golden is present at HEAD.** A jq set difference over all catalogs gives
  0 missing entries. The lists of all 148 of main's catalogs are identical at HEAD.
- **Only this slice's catalogs differ.** HEAD has 150 catalogs. The only two that are not identical to
  main are `slices/core/memory-records/i18n/fr.po` and `zh.po`, which main did not have.
- **The two lists match the code.**
  - Keys are `msgctxt` split at `|` with the predicate expanded (`reviewed_coverage.rs:140-150`). The
    filter is `counts_as_reviewed_coverage` (`i18n_compile.rs:493-496`).
  - The sorted, de-duplicated `msgctxt` set of each head catalog (512 per language) equals the golden list
    exactly, and the golden lists are sorted.
  - Every entry is non-fuzzy with a non-empty `msgstr`. I did not apply `translation_has_integrity`, but
    the coder's full regeneration (`r5-full-regen.json`) gives the identical 512-key lists for both files.
- **The rest of the merge is main's, unaltered.** Every non-slice path in `fc3a7119f..HEAD` is
  byte-identical to `origin/main` (`git diff --quiet origin/main HEAD -- <path>` for each):
  `functional_carrier_ledger.txt`, `dsl.tests.rs` and the 9 epistemics files.
- **Pre-existing observation, not this branch's defect.**
  - `po_parser_parity.rs:50-73` asserts that the golden equals the producer's observation of every
    discovered catalog. That test is on the default nextest lane.
  - The coder's own full regeneration from the same code covers 190 catalogs. It differs from the
    committed golden in 70 of the shared catalogs and has 40 catalogs that main's golden lacks.
  - Main's golden therefore already diverges from what the code computes. This branch neither causes nor
    widens the divergence, and it follows main's own hand-union convention (main's `9f445d310` added only
    epistemics keys).
  - Whether that test passes at main or at this head is not shown.

## 7. Pins, floors, prose, syntax

- **Pins.**
  - Outside the slice and the golden, no file in `crates`, `governance`, `.config`, `bench` or `tests`
    names the slice, its finality terms or its constraint names. The one hit is an unrelated test name in
    `conformance_agentic.rs`.
  - The branch adds no functional carrier, so `functional_carrier_ledger.txt` is unaffected.
  - No `*.snap` or slicetest count pin names the slice.
- **Floors.** Neither `migrated-axis-floors.golden.tsv` nor `migrated-tier-floors.golden.tsv` names
  memory-records. The manifest declares only `sliceTier tierCore`.
- **Ledger.** `.deficiencies` has no entries. The slice diff contains no issue or PR number and no
  follow-up wording.
- **Prose.** `prose-check` exits 0 on `docs.md` and the design note, on all added lines of
  `fc3a7119f..HEAD` in the module, tests, example and catalogs (stdin), and on the HEAD commit message.
- **Syntax.** `rapper -c` exits 0 on `module.ttl`, `examples/trace-finality.ttl`, `tests/structural.ttl`
  and the new fixture.

## REQUIRED findings

None.

## OPTIONAL findings

- **O1. The constraint's remedy can invite a fabricated ledger row** (`module.ttl:1424`, and the matching
  fr/zh msgstr). "The declarer lists the packet or names another cut": the watermark's declarer need not
  be the ledger's source. `tracePacket` is "only for the source's actual observed-packet membership" and
  says to avoid "synthesizing missing rows" (1170-1171). Suggest: "the ledger's source records the
  membership it actually observed, or the declarer names a cut the ledger lists."
- **O2. `docs.md:290`, "performs no join".** This older sentence means the recorder's trace join, but it
  now sits beside "a plain join of three roles" (364). Suggest "performs no trace join".
- **O3. The allowed relations are described more broadly than they are.**
  - Design 267-270 says the ban allows "the finality roles". It allows only `traceFinalityLedger`,
    `watermarkPacket` and `frontierPoint`, so a constraint on `traceFinalityProducer` would trip it.
  - The rationale at `structural.ttl:73` ("the finality roles being limited") fits the ledger binding
    poorly.
  - Suggest naming the three roles.
- **O4. Nothing executes the constraint itself.**
  - `saTraceFinalityCutListed` is a SPARQL restatement of the rule over the worked examples. Its
    "conforming and violating controls" (rationale; commit message) test that ASK, not the
    `logic:Constraint` or its lowering.
  - The fixture is also not a clean single-rule counter-example: its untyped `ce:unlistedPacket`,
    `ce:listedPacket` and `ce:producer` would trip the derived class checks too.
  - If an executed control is wanted, bind a typed fixture through an example-conformance record that
    expects this constraint's violation. Otherwise state in the rationale that the control is for the ASK.
- **O5. `saTraceFinalityLedgerMembership` does not pin argument wiring.** A reversed consequent
  `tracePacket(p, l)`, or a variable not shared with the antecedent, would still pass. Binding
  `logic:termIndex`/`logic:termVariable` on the consequent and the ledger and cut conjuncts would close
  this. `saTraceFinalitySingleValue` has the same gap.
- **O6. The give-up useWhen still says "for named packets"** (`module.ttl:1254`), against "where the
  producer names them" in the definition and howToUse.
- **O7. Chinese `watermarkPacket` definition.** "该水位线声明标记已终结的、位于其处或其之前的已记录 TracePacket" can
  be read as describing any finalized packet rather than the boundary. The trailing clause
  disambiguates it. Suggest "该水位线声明：在此已记录 TracePacket 处及其之前，标记已终结；包含该数据包本身，而不包含其之后的数据包。"
- **O8 (carried over, the earlier O6).** `saTraceFinalityNoComparison` guards only constraints that
  formalize the nine finality terms.

## Open state

- The branch is pushed. No PR, merge or execution is shown here.
- `origin/main` is as last fetched locally (`e32f90c96`), and the branch contains it. A later main is not
  checked.

## Not shown

- No execution at this head:
  - the slice-spec run of the 11 structural cells, the 6 fail-witness teeth checks and the 3 competency
    cells;
  - the lowering of the new constraint to SPARQL and its evaluation on the example (conforms) and the
    fixture (violates);
  - validator, annotation gate, `make reason-verify`, `make check` and CI.
- `translation_has_integrity` on the new entries. It was inferred from the coder's regeneration output,
  not run by me.
- The pass or fail of `corpus_reviewed_coverage_matches_frozen_golden` at main or at this head, given that
  the committed golden diverges from a full regeneration (section 6).
- Whether any repository-wide example validation runs the slice's constraints over
  `examples/trace-finality.ttl`.
- PurRDF's parse of `finality-role-in-rule-body.ttl` (unchanged since the last review).
