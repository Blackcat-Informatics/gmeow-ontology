# Recheck: PR 2803 (paudley/2745-term-carrier-index)

- Head reviewed: `651cb3ff3f7093dd254be543cd4a9fa6c4f4f6e0` (checked with `git rev-parse HEAD` in the worktree; it also matches the PR's `headRefOid`)
- Delta reviewed: `git diff 72d400ffb 651cb3ff3` (one commit, 6 files, +20/-11)
- Timestamp: Sat 14 Nov 2026 07:40:21 PM CST (`date`)
- Mode: read-only. No builds or test runs. The prebuilt `target/release/gmeow convert` was run once to re-parse
  module.ttl for the msgid byte check; its output went to my scratch directory
  `.stage/reviews/scan-2745/`.
- Earlier review: `review-2745-carrier-at-72d400ffb.md`. Its coverage stands for files unchanged since
  72d400ffb.

## What I read

| Item | Scope |
|---|---|
| Delta, all 6 files | every hunk |
| slices/grounding/logic/module.ttl | 15781-15792 (`logic:TermCarrier` definition), 17042-17050 (`TermCarrierValueConstraint`), 17157-17161 |
| crates/logic-compile/src/frontend/formula_reader.rs | 805-811, 952-1029 (from the earlier review; file unchanged) |
| crates/logic-compile/src/ir/constraint.rs | 742-765 (guard-atom collection; from the earlier review, file unchanged) |
| fr.po / zh.po | new howToUse entries; neighbouring howToUse entries; byte comparison of all 24 English literal/catalog pairs by script |
| crates/docs/tests/fixtures/reviewed_coverage_golden.json | presence of the new howToUse key (jq) |
| PR 2803 body | all |
| author logs `.stage/issue-2745-carrier/lint-2.log`, `msgfmt-{fr,zh}-2.log` | all (read as claims) |

## Answers

**(1) LOGIC-IR.md, lines 51-57.** The sentence now lists `termApplication`, `termIri`, `termLiteral` (with
`termLiteralDatatype`), `termSequenceMarker` and `termVariable`. These are exactly the five predicates in
`parse_term` (formula_reader.rs:805-811). `parse_term` rejects a datatype paired with any kind other than a
literal (846-855). The same five appear as the `logic:choicePredicate` values of
`logic:TermCarrierValueConstraint` (module.ttl:17045-17049).

The added sentence says a `logic:termApplication` value names a `logic:FunctionTerm`: one function symbol
applied to its own ordered `logic:argument` term-carriers. The code bears this out:
- `parse_function_term` requires exactly one IRI-valued `logic:functionSymbol` (996-1017).
- It reads the term's own `logic:argument` carriers through `parse_term_carriers`, so they are ordered and
  indexed (1020).
- A nullary application is rejected (1025-1028).

The rest of the paragraph is unchanged from the earlier review and true. Finding 1 of the earlier review is
resolved.

**(2) howToUse.** The new text, "Require one non-negative integer index on each logic:argument carrier, then
validate the sibling index set as 0 through n minus 1.", is consistent with the definition. That covers both
the objects of `logic:argument` and the exemption for triple-term carriers. Finding 2 of the earlier review is
resolved.

The new fr and zh entries use the context key
`https://blackcatinformatics.ca/logic/TermCarrierIndexConstraint|https://blackcatinformatics.ca/gmeow/howToUse`.
This is the same full-IRI form as the neighbouring `BoundVariableCarrierIndexConstraint` howToUse entry. Their
msgids match the module literal byte for byte; I re-parsed module.ttl and compared by script, and 22 of 24
pairs match. The 2 misses are the TermCarrierIndexConstraint avoidWhen in fr and zh. That literal is unchanged
and had no entry at the base either, as noted in the earlier review.

Both translations carry every clause:
- The French reads correctly: "Exiger un indice entier non négatif sur chaque porteur de logic:argument, puis
  valider l'ensemble des indices des porteurs frères comme allant de 0 à n moins 1."
- The Chinese keeps `logic:argument` and the "0 到 n 减 1" range, and renders "sibling" as 同级载体.

**(3) Test changes.**
- `tests/structural.ttl` now matches `logic:choicePredicate logic:termApplication, logic:termIri,
  logic:termLiteral, logic:termSequenceMarker, logic:termVariable` on `logic:TermCarrierValueConstraint`. As
  authored, the module lists exactly those five values (17045-17049), so this part of the ASK holds. The ASK
  matches a subset of triples, so it would still hold if a sixth predicate were added. This is the same
  property the base assertion had, not a regression.
- The rationale's "five exclusive term kinds" is true.
- The reworded `tests/example-conformance.ttl` comment says the positive case "jointly satisfies the
  ontology-authored constraints named in its rationale". That rationale (line 254) names FormulaConstructor,
  both implication guards, TermCarrierValue, TermCarrierIndex, BoundVariableCarrierIndex and LiteralDatatype.
  The earlier review established that the pass fixture satisfies each of them, so the comment is true. Earlier
  optional finding 3 is resolved.

**(4) Rest of the delta.** Nothing else changed. There are no issue or PR references in repository files, no
`owl:` triples and no shape authoring.

**PR body.**
- True at this head:
  - the problem statement; `logic:TermCarrier`'s definition (module.ttl:15781ff) does say a triple-term
    carrier needs no index
  - the scope of both constraints, and that a disjunctive guard is not accepted: `collect_guard_atoms` descends
    only into atoms, `∧` and `∃` (constraint.rs:746-757), so an `∨` guard yields no target
  - the test, LOGIC-IR.md and catalog bullets, and the Rust test bullet
  - the "Not shown" items, and "148 catalogs against 188"
- `make lint` and `msgfmt --check-format` logs exist with EXIT=0. They are timestamped 19:26, before the
  commit, and record HEAD 72d400ffb, so they ran against the working tree. I did not rerun them.
- One statement is not true; see the finding below.

## Findings

1. **OPTIONAL: PR 2803 body, "this branch adds 15 keys to that existing drift".** At this head the branch adds
   16 reviewed-coverage keys:
   - 7 in fr from the first commit: the six BoundVariableCarrierIndexConstraint literals plus the
     TermCarrierIndexConstraint useWhen
   - the same 7 in zh
   - 2 for the new TermCarrierIndexConstraint howToUse, one in fr and one in zh; that key is absent from the
     golden (jq count 0)

   The count is off by one in publication text, not in repository content. Correct it when the body is next
   edited.

No REQUIRED findings. Both REQUIRED findings from the review at 72d400ffb are resolved. What only a producer
pass can prove is unchanged from the earlier report.

VERDICT: PASS at 651cb3ff3f7093dd254be543cd4a9fa6c4f4f6e0
