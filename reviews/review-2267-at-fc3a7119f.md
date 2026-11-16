BLOCKED at fc3a7119f

# Delivery review of issue 2267 (finality vocabulary, memory-records) at fc3a7119f

## Scope

- Worktree `/home/paudley/Active/gmeow-ontology/.worktrees/2267-marks-on-an-agent-s-important-events`.
  `git rev-parse HEAD` = `fc3a7119f18512605dcbe19e7177174f0a3d5eda`, as briefed. `git status` is clean,
  and the head equals `origin/paudley/2267-…`.
- The branch's merge base is `96f9fd6cd`. Against it, the branch changes 14 files: 13 under
  `slices/core/memory-records` and `crates/docs/tests/fixtures/reviewed_coverage_golden.json`.
  `be52b152b..HEAD` also contains the main-side math changes from merge `b06f24ed4`. They are not
  part of this delivery.
- **`origin/main` moved during this review** to `e32f90c96` (epistemics, which extends the same
  golden). `git merge-tree HEAD origin/main` reports a content conflict in
  `reviewed_coverage_golden.json`. See "Open state" below. This is not a finding against the work at
  this head.
- I read the contract verbatim from `stagectl issue 2267`: the body, the delivery map, the three
  later sections, and both comments. The binding ruling is the maintainer's split decision of
  2026-11-15T00:32:16Z and its three acceptance items. I read nothing in `.stage/issue-2267/`
  except as claims, and none of it is used as evidence here.
- The review was read-only. Commands run: `git` (read-only, including `merge-tree`),
  `stagectl issue`, `rapper -c`, `prose-check`, `jq`, `grep`, `comm`. No make, cargo, producer, gate,
  reasoner or test run.
- One deviation from the limits: one `rapper` call sent stderr to `/tmp/rapper.err`. I deleted the
  file in the next call. Nothing else was written outside this review file.

## What I read at fc3a7119f

- `slices/core/memory-records/module.ttl:1170-1417`: the tracePacketAt/TracePacket context, all 9
  finality terms, the closures, the restrictions and both constraints.
- `docs.md`: the full branch diff, 280-410 in full, and the unchanged finality mentions at 158-168
  and 222-230.
- `design/MEMORY-RECORDS.md`: the full branch diff, 195-289 in full, and the unchanged mentions at
  76-86 and 155-167.
- `examples/trace-finality.ttl:1-71`, `tests/competency.ttl:1-98` and `tests/structural.ttl:1-173`,
  read whole.
- All five `tests/counter-examples/finality-*.ttl`, read whole.
- `i18n/fr.po` and `i18n/zh.po`:
  - every `+msgstr` in `be52b152b..HEAD` (12 entries per language), read for meaning;
  - a mechanical set comparison of all finality-subject msgids against the module literals.
- Harness:
  - `crates/slicetest/src/exec.rs:592-727` (structural cell and fail-witness teeth check);
  - `dsl.rs:105-127`;
  - `native_query.rs:98-122`;
  - `crates/logic-compile/src/projections/reader_view.rs:1-110` and grep of the rest (term-level and
    additive; it keeps reifiers in the reifier table and does not compile formulas);
  - `crates/slicetest/src/repository.rs:355-375` (fail-witness files are spec inputs).
- Repo-static: `crates/validate/src/repo_static.rs:260-345,416-441,598-700` (fixture handling).
- Precedents:
  - the rule-body form at `slices/core/refinement-cues/module.ttl:1114` and
    `slices/core/recall-context/module.ttl:922`;
  - the same ASK form at `slices/core/inquiry/tests/structural.ttl:388-398`;
  - the math fail-witness at `slices/grounding/math/tests/structural.ttl:1515-1530`;
  - the functional form at `slices/core/accounts/module.ttl:318-328`.
- `/home/paudley/.claude/skills/rdf-work/SKILL.md:35-60` (triple-term syntax).
- Golden: `crates/docs/tests/po_parser_parity.rs:15-75`, `crates/docs-model/src/reviewed_coverage.rs:134-162`,
  `ontology_catalogs.rs:207-240`, `i18n_compile.rs:493-496`; key-set diff of the golden at merge base,
  head and current `origin/main`.
- Pins: `git grep memory-records` outside the slice and the golden finds only two unrelated slice docs
  and one manifest. No count pin in `crates/slicetest/src/dsl.tests.rs` (it pins epistemics only) and
  no slice-quality floor names the slice.
- Earlier reviews: `review-2267-at-be52b152b.md` in full, `review-2267-at-a65417f4a.md:130-160`.

Not read: catalog entries for other subjects; the slice's other examples; `generated/`; PurRDF's
Turtle parser and the SPARQL evaluation of `rdf:reifies` patterns (only `sparql-eval` grep); the
slice-quality rubric code; the `translation_has_integrity` rules.

## Earlier findings

| Finding (review at be52b152b) | State at fc3a7119f | Basis |
|---|---|---|
| R1: the watermark/frontier model drifted from the issue | **Resolved** | See below |
| R2a: no `mustNot` ban had a fail witness | **Resolved by reading** | All five bans now declare `saFailWitness` (`structural.ttl:62,77,94,107,150`). Worked by hand below |
| R2b: nothing executed | Not shown | Out of scope for findings per the brief |
| O1: a give-up reads as if it must name packets | **Partly fixed** | The scope note (`module.ttl:1252`), `docs.md:375` and design 254 now say "may name". The definition (1251, "given up for named packets") and howToUse (1256, "name every packet") still read as required. See O3 |
| O2: the functional-axiom rationale is unscoped | Open | See O5 |
| O3: `cqWatermarkDeclared` omits the declarer | **Resolved** | `competency.ttl:18-23,35,43` |
| O4: the no-comparison guard covers only constraints that formalize finality terms | Open | See O6 |

Details of R1:

- **Watermark.** It is final for every packet of the ledger at or below the cut, and "no mark will
  name such a packet" (`module.ttl:1225`, `docs.md:356-358`, design 245-246). No per-producer
  narrowing remains, and a grep for "one producer", "whose marks it finalizes", "every producer it
  knows" and "that producer's frontier" finds nothing in the module, docs, design, tests or either
  catalog.
- **Frontier.** It is a late producer's committed point that a watermark must not pass
  (1238-1239, 1279; `docs.md:365-371`; design 247-250).
- **Deferred law.** It is stated as "every applicable late-producer frontier is respected before a
  watermark advances" (design 273-274, `docs.md:383-385`, `module.ttl:1226`).
- **Discard condition.** The added "holds a watermark for every producer" condition is gone. The
  condition is back to unmarked, at or below the cut, and old enough (1230, `docs.md:392-395`,
  design 284-288).
- **Example.** `ex:cut` at packet one (09:00) and `ex:lateFrontier` at 09:07 now illustrate the
  issue's relation (`examples/trace-finality.ttl:40-61`).

## 1. Text against the issue

The issue says:

- the order is time, then digest;
- the watermark makes marking final at or below one packet ("no mark will name it") and never moves
  backward;
- a recorder may then discard an unmarked packet at or below it, once that packet is old enough;
- a late derivation's committed point is never passed by the watermark;
- lost in-memory marks are given up explicitly, with a reason, before the watermark passes their
  packet.

The ruling lists the deferred laws as time-then-digest order, the inclusive never-backward
watermark, and every applicable late-producer frontier before advancement.

| Branch statement | Against the issue |
|---|---|
| The watermark is final for every packet at or below the cut, inclusive (`module.ttl:1225,1292`; `docs.md:356-358`; design 245-246) | True to the issue |
| The order is time, then digest (1226; `docs.md:383`; design 271-272) | True |
| The watermark never moves backward, stated as an unchecked law (1226; `docs.md:383`; design 272) | True |
| The watermark must not pass any applicable late-producer frontier (1225-1226, 1239, 1279; docs 359-360, 370, 384; design 247, 273, 282) | True. It uses the issue's word "applicable" and leaves applicability undecided (example 51-52) |
| Lost marks are given up before the cut passes their packet (1226, 1252; docs 385; design 274) | True |
| Discard: unmarked, at or below the cut, and old enough "under the recorder's own policy"; nothing recorded or deleted (1230; docs 392-395; design 284-288) | True. "The recorder's own policy" fills a point where the issue is silent about who judges "old enough". It is not a new rule |
| A give-up does not mark packets or make them eligible for discard (1252, 1321) | Consistent: eligibility still needs the watermark |
| A give-up is "for example on a restart" (1251) | Broadens "on a restart". This fits "general memory terms" in the issue body |
| A frontier is the point up to which the late producer "has finished marking" (1238) | The issue is silent on what "committed point" means. This reading is the one that makes "the watermark never passes it" coherent. Not an invention |
| A frontier "declares nothing final" (1238; docs 369; design 248-249) | The issue is silent. This contradicts "has finished marking" in the same definition. See O1 |
| "A producer that has committed nothing states no frontier; absence is not a frontier at the start of the order" (1239) | The issue is silent. This forbids fabricating a record; it does not decide whether such a producer still bounds a watermark. Not an order law |
| The watermark belongs to a producer; the declarer is separate (`wasAttributedTo`) and may be the same agent | The ruling requires the producer. Requiring the declarer is Principle 9 and follows the sibling `TraceMark`. Not an invented law |
| Each cut is a separate watermark record ("A further cut is a separate watermark"; "Several watermarks may coexist") | The issue is silent about record identity. This modelling choice does not change "never moves backward", which stays unchecked |
| **Membership of the cut in its ledger is unchecked "consequently" on the missing typed comparison** (1226; docs 386-388; design 278-281) | **Not true. See R1** |
| The example ledger is the recorder's, attributed to `ex:recorder` (example 9-10, 36-38) | The issue says "The service records each packet it sees". See O7 |

## 2. The five bans and their fail witnesses

The harness (`exec.rs:664-709`) parses module and fixture, unions them (blank nodes kept apart),
applies the additive reader view, and requires a `mustNot` ASK to hold there. The primary run is over
the module alone through the same view.

| Ban | Module and fixture | Module alone |
|---|---|---|
| `saTraceFinalityNoComparison` (60-73) | `ce:watermarkTimeComparison` is a `logic:Constraint` that formalizes `TraceWatermark`. Its root, through `forall` and then `consequent`, reaches a formula with `logic:relation logic:termLessEqual`. That relation is not in the allow-list, so the ASK is **true** | The only constraints that formalize finality terms are 1378 and 1398. Their relations are `rdf:type`, the role and `logic:termDistinct`, all allow-listed, so the ASK is **false** |
| `saTraceFinalityDerivesNothing` (75-90) | `gmeow:watermarkPacket logic:subPropertyOf ce:finalizedPacket` binds `?p ?link ?q`, so the ASK is **true** | No sub-property, inverse, equivalent or chain triple names any of the six roles (grep of the module), so the ASK is **false** |
| `saTraceFinalityNoRuleBody` (92-103) | `ce:finalizedFromWatermark a logic:Rule ; logic:body [ rdf:reifies <<( ce:someCut gmeow:watermarkPacket ce:somePacket )>> ]` binds `?atom` and `?p = watermarkPacket`, so the ASK is **true**, if the engine matches `rdf:reifies` against a parsed triple term as the inquiry ASK and the standpoint and evidence competency queries rely on | The module has no `logic:Rule`, `logic:body` or `logic:negatedBody` (grep), so the ASK is **false** |
| `saTraceFinalityNoFormulaOutsideConstraint` (105-118) | `ce:frontierPointLaw logic:relation gmeow:frontierPoint` is reachable from no constraint root, so the correlated `FILTER NOT EXISTS` passes and the ASK is **true** | The only formulas on the roles (1394-1395 and 1414-1415) are reachable from the integrity roots through `forall`, `consequent`, `not`, `exists` and `and`, so the ASK is **false** |
| `saTraceFinalityNotFunctional` (148-157) | `ce:watermarkPacketFunctionality logic:characterizes gmeow:watermarkPacket ; logic:characteristicSort logic:functionalProperty` gives the ASK **true** | No `characterizes` names a finality role, so the ASK is **false** |

**Triple-term syntax.** `[ rdf:reifies <<( s p o )>> ]` puts a triple term in object position of
`rdf:reifies`. That is the form the skill prescribes (SKILL.md:54-56) and the form the shipped
`refinement-cues` and `recall-context` rule bodies use. `rapper -c` rejects this fixture at line 14
("syntax error at '<'"). It rejects `refinement-cues/module.ttl` the same way at line 1112, because
rapper has no Turtle 1.2. So rapper gives no evidence either way. The other four fixtures, the
module, the example and both test files pass `rapper -c` (exit 0).

**Other checks.**

- The reader view (`reader_view.rs`) renames terms only and keeps reifiers. It compiles no formula,
  so the deliberately unsafe formula in the comparison fixture cannot make the projection fail.
- Repo-static applies only the SHACL compute, migrated-`sh:select` and alignment checks to
  registered witnesses. None of these fixtures contains such a construct.
- No fixture contains a shape, `owl:` or `rdfs:` subsumption.

## 3. Competency questions, worked by hand on `examples/trace-finality.ttl`

- **`cqWatermarkDeclared`.** Only `tf:cut` is a `TraceWatermark`. It has one value for each of
  ledger, producer, `wasAttributedTo` and packet (43-47). The query returns exactly one row:
  `(tf:cut, tf:ledger, tf:memoryService, tf:memoryService, tf:packetOne)`. That equals `ex:wdCut`,
  including the new `declarer` cell, and the shape now declares five required IRI columns.
- **`cqFrontierCommitted`.** It returns one row: `(tf:lateFrontier, tf:lateDeriver, tf:pointKey)`.
  That equals `ex:fcLate`.
- **`cqMarksGivenUp`.** It returns one row: `(tf:lost, tf:packetTwo, "Marks held only in memory were
  lost when the service restarted."@en)`. That equals `ex:guLost`.
- **`saTraceFinalityExample`** (159-172) still holds over the module and examples. No other example
  file declares a finality record.

## 4. Catalogs

- **msgid equality.** Each of the 12 English literals changed in `be52b152b..HEAD` has exactly one
  `msgid` match by `grep -F` in each catalog. A set comparison of every `msgid` under a finality-subject
  `msgctxt` against every module literal at 1220-1416 shows no difference in either language. Neither
  catalog has a `fuzzy` flag or an empty `msgstr` other than the header.
- **Meaning of the 24 changed translations.** Each says what its source says.
  - The watermark: final at or below the cut, belongs to its producer, and must not pass any
    applicable late-producer frontier ("frontière applicable d'un producteur qui marque plus tard";
    "适用的事后标记生产者前沿").
  - The unchecked-law lists match their sources.
  - The discard condition is rendered as eligibility, not permission ("n'est admissible à
    l'élimination que sous ces conditions"; "仅在这些条件下才具备被丢弃的资格").
  - The frontier definition, the producer definition, its scope note and its useWhen are faithful.
  - I found no mistranslation. Two awkward renderings carried over from the previous text are under
    O8.
- **Golden.** Keys are `subject|predicate` (`reviewed_coverage.rs:140-150`). This round adds and
  removes no subject or predicate, so it needs no golden change.
  - Each catalog has 504 `msgctxt` entries and 504 golden keys.
  - The merge-base golden had no `memory-records` entry at all, although the slice already shipped
    425 entries per catalog. The branch's golden adds the whole slice.
  - Integration with current main needs a union of the golden. See "Open state".

## 5. Whole branch

- **Acceptance item 1.**
  - All three records and six roles carry a definition, scope note, example, useWhen, avoidWhen and
    howToUse, each with fr and zh entries.
  - There is one worked instance of each record.
  - Required paths are `allValuesFrom` plus class-local `ClosedWorldClosure` (1350-1376):
    - a give-up: its reason;
    - a watermark and a frontier: ledger and producer;
    - in addition: the watermark's cut, the frontier's point, and every declarer.
- **Acceptance item 2.** The design note lists the deferred laws and gives a reason for the deferral
  (271-282). It cites no tracker. Its "why" misattributes one item; see R1.
- **Acceptance item 3.** No constraint in the slice compares a time or a digest. The only `logic:term*`
  relations in the module are `termDistinct`, a boolean `termEqual`, `termHasLang`, `termRegex` and
  `termSameComponent`. There is no `termLess` or `termGreater`.
- **Rules.** No hand-authored shape, no `owl:` in the module, no unqualified cardinality. There is no
  issue or PR number and no process wording. "Deferred laws" is the wording the ruling asks for.
- **Pins and floors.** None in `crates/`, `governance/` or `.config/` names the slice except the
  golden. No slice-quality floor names it. The fail-witness files feed the spec cache key
  (`repository.rs:360-365`).
- **Prose.** `prose-check` exits 0 on `docs.md`, the design note, the module literals at 1220-1416
  (stdin), the changed msgstrs (stdin), and the comments and rationales of the tests, the example and
  the fixtures (stdin).
- **Scope.** No unauthorized expansion. The required declarer goes beyond the literal acceptance
  list, as before, and I accept it under Principle 9.

## REQUIRED findings

### R1. The text gives a false reason why ledger membership of the cut is unchecked

Locations:

- `docs.md:386-388`: "Checking them needs a typed comparison of times and digests, which no
  constraint here makes … Consequently nothing verifies that a cut belongs to its ledger, …"
- `design/MEMORY-RECORDS.md:278-281`: "Until that comparison exists, … Consequently nothing verifies
  that the cut belongs to its ledger, …"
- `module.ttl:1226` (and its fr/zh entries) puts "that the cut packet is a member of the ledger" in
  the list that begins "Without a typed time and digest comparison nothing here checks …".

Why this is a defect:

- Membership is a join with no comparison: `traceFinalityLedger(w, l) ∧ watermarkPacket(w, p) →
  tracePacket(l, p)`. A typed time or digest comparison is neither needed for it nor would supply it.
- The membership check is absent for other reasons. The branch chose not to author it, and its own
  `saTraceFinalityNoComparison` allow-list would reject a constraint that used `traceFinalityLedger`
  or `tracePacket`. The guide also says a ledger need not list every packet it observed
  (`docs.md:298-301`).
- Acceptance item 2 asks the design note to state the deferred laws "and why". As written, the note
  hands the comparison work a law that is not a comparison law, so that law has no owner.
- `docs/GATE-AND-PIPELINE.md` treats a false claim in a comment or help text as a defect.

Smallest correction:

- Remove membership from the "Consequently" sentences in `docs.md:388` and design 280-281.
- In `module.ttl:1226`, move it out of the "Without a typed time and digest comparison" list.
- State the true position once, in each of the three places. For example: "Nothing checks that the cut
  is listed in the declared ledger; this is not authored", together with its actual reason. Do not
  invent the reason; the primary decides it.
- Update the fr/zh `msgid` and `msgstr` for the changed module literal. The golden keys do not change.
- If the maintainer wants membership enforced instead, author it as a `logic:Constraint` and widen the
  no-comparison allow-list to the two relations. That would be a scope addition, not a correction.

## OPTIONAL findings

- **O1. A frontier "declares nothing final" (`module.ttl:1238`, `docs.md:369`, design 248-249).** The
  same definition says the late producer "has finished marking" up to the point, which is finality for
  that producer's marks. "Declares no ledger-wide finality and makes no packet eligible for discard"
  would say what is meant without the contradiction.
- **O2. `docs.md:287-288`, "or a recorder may advance a frontier".** The paragraph was rewritten in
  this branch. In the new model a frontier is a late producer's point, and what advances under the
  issue's law is the watermark. Suggest "a watermark may advance".
- **O3. The give-up definition and howToUse still read as if packets are required.** "Given up for
  named packets" (1251) and "name every packet whose marks are given up" (1256) sit beside the
  optional `givenUpPacket` (1321) and "may name" (1252). Either soften both, or make the packet
  required. The issue's "before the watermark may pass their packet" argues for requiring it.
- **O4. "Each term says plainly … nothing checks time-then-digest order or that a watermark never
  moves backward."** The watermark says this (1226). The frontier (1239) and the give-up (1252) each
  say only that their own relation to the watermark is unchecked. I read "each term" as the three
  records and their notes as meeting the intent, as the first review did (its O8). If the maintainer
  means it literally, add one clause to each of the two scope notes and their catalog entries.
- **O5. The functional-axiom rationale** (design 260-263, `docs.md:379-381`, scope notes 1293, 1307,
  1384, 1404). It says a functional axiom "would make a second value the same individual". That holds
  for an OWL Direct projection. In this repository a `PropertyCharacteristicAssertion` is also the
  carrier a native coherence gate enforces (`accounts/module.ttl:325`). Scope the sentence to the
  OWL projection.
- **O6. `saTraceFinalityNoComparison` only guards constraints that formalize the nine finality terms.**
  A time comparison authored with `logic:formalizes gmeow:tracePacketAt` would pass it. Acceptance
  item 3 holds by reading at this head.
- **O7. The worked example attributes the ledger to the recorder** (`examples/trace-finality.ttl:9-10,
  38`). The issue says "The service records each packet it sees". Attributing `ex:ledger` to
  `ex:memoryService` would match the issue's scenario.
- **O8. Translation clarity, carried over and not mistranslations.** French `watermarkPacket`
  definition: "à partir duquel et en dessous duquel" would read better as "au niveau duquel ou en
  dessous duquel". Chinese `watermarkPacket` definition: "位于其处及其之前 … 其之上" mixes
  before and above. Elsewhere the catalog pairs 之前 with 之后.

## Open state

- **Integration.** `origin/main` is now `e32f90c96`. It extends the same one-line golden with 17
  epistemics keys per catalog. `git merge-tree --write-tree HEAD origin/main` reports
  `CONFLICT (content)` in `crates/docs/tests/fixtures/reviewed_coverage_golden.json`. The branch must
  merge main and keep both sides' keys: the 17 epistemics keys and the 504 memory-records keys per
  catalog. The worked key-set diff confirms the two sets are disjoint.
- **Delivery.** The branch is pushed, but no PR, merge or execution is shown here.

## Not shown

- No execution at any head:
  - the slice-spec run of the 9 structural cells, the 5 fail-witness teeth checks and the 3
    competency cells;
  - validator, annotation gate, reasoner, `make check` and CI.
- That PurRDF's Turtle 1.2 parser accepts `finality-role-in-rule-body.ttl`, and that its SPARQL
  engine matches `?atom rdf:reifies <<( ?s ?p ?o )>>` against that parse after the reader view. This
  is inferred from the shipped precedents only.
- The derived SHACL for the new closures, and the lowering of the two `termDistinct` constraints.
- Whether every memory-records catalog entry passes `translation_has_integrity`, which decides the
  504-key golden count. I counted entries; I did not apply the rule.
- Translation fidelity of entries for other subjects.

VERDICT: BLOCKED
