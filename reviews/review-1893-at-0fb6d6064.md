BLOCKED at 0fb6d6064

# Delta review: issue 1893 unit at 0fb6d6064 (pull request 2827)

## Scope

- Worktree `.worktrees/1893-ground-quantitative-quality-assessments`, branch
  `paudley/1893-ground-quantitative-quality-assessments`. `git rev-parse HEAD` =
  0fb6d60640597f12e2c446989cabb48ed2d7d03d. `git status --porcelain` printed nothing, so the tree is clean.
- Delta `909446ec8..0fb6d6064`: one commit, one file (`slices/core/quality/tests/structural.ttl`, 39 insertions,
  10 deletions). The only hunk is inside `ex:saAggregateConstraintNamesEveryRequiredRelation`: its
  `gmeow:saPattern` (lines 135-181) and `gmeow:saRationale` (line 182). No other assertion, file or module
  term changed. `module.ttl` is unchanged in the delta.
- Read-only. No make, cargo, producer, gate, reasoning, test or SPARQL engine was run. `.deficiencies` has no
  entries below its marker.

## What I read

- `slices/core/quality/module.ttl` 150-193 (the whole file is 193 lines; the constraint starts at 167 and the
  `logic:integrity` tree is 180-193).
- `slices/core/quality/tests/structural.ttl` 1-193, plus `git show 909446ec8:` of the same assertion (135-154).
- `git diff 909446ec8..0fb6d6064` and the commit message.
- `.stage/reviews/review-1893-at-909446ec8.md` (the earlier PASS).
- `.stage/issue-1893/r3-hand-eval.md`, `r3-rapper.log`, `r3-prefix.log`, `r3-prefix.sh`, the tail of
  `r3-lint.log`, and `pr-body.md`.
- `stagectl issue 1893` (body and both comments).
- `crates/slicetest/src/exec.rs` 690-727 (`run_ask`) and `crates/slicetest/src/native_query.rs` 165-185.
- The SPARQL parser locked by `Cargo.lock` (`purrdf-sparql-algebra` 4.0.0, from the cargo registry):
  `src/parser.rs` 428-470 and 1350-1366 (`expect_eof`).

Not read: the rest of the branch (covered by the earlier PASS), `r3-new.ttl`, `r3-commit.log`, `r3-push.log`,
the PurRDF lexer, the forge PR page and its comments.

## REQUIRED finding

**R1. The new pattern does not parse: it has a stray closing brace.**
`slices/core/quality/tests/structural.ttl` 180-181. The query opens one group (`ASK {` at 139) and closes two:
line 180 is `}` and line 181 is `}""" ;`. Counting braces over 135-181 gives `{}}`; the same count over the
previous head's assertion gives `{}`. The commit added the line-180 `}` and kept the old closing brace.

Effect: the locked parser's `parse_query` calls `expect_eof()` after the query form (`parser.rs` 447), which
refuses any remaining token with "unexpected trailing token" (1350-1366). `run_ask` (`exec.rs` 714-719) turns
that into a hard `SparqlEval` error. The assertion therefore never evaluates to TRUE or FALSE. It fails the
quality slice specification with a parse error at the first producer pass. Every protection the rationale
claims is void until that is fixed. No error is swallowed, so nothing passes silently; it simply cannot pass.

Why the evidence missed it: `rapper -c` checks only the Turtle around the literal, and `r3-prefix.sh` checks only
prefixes. Both logs also record HEAD 909446ec8, so they ran before the commit and do not attest 0fb6d6064. No
evidence file parses the SPARQL. `r3-hand-eval.md` says "Result: the ASK is true". That claim is false of the
committed text, because the text is not a query.

Smallest correction: delete line 180 (the lone `}`), so that line 179's triple is followed by `}""" ;`. No
other change is needed. The evaluation below shows that the corrected pattern meets the delta's goal.

## The formula tree as encoded (module.ttl 180-193)

The tree is one anonymous node structure. Every variable is a plain string literal in `logic:termVariable`, and
every index is an `xsd:integer` in `logic:termIndex`. Each node is typed `logic:Formula`.

```
L  (180)  quantifiedVariable {idx 0 "this", idx 1 "result"}
          forall F0
F0        antecedent A0 = and {
             (180) rdf:type        [0 var "this", 1 iri gmeow:AggregateQualityAssessment]
             (181) gmeow:observationResult [0 var "this", 1 var "result"] }
          consequent O = and { E1, T1, E2, T2, P }

E1 (181-182) antecedent and { rdf:type[0 "this",1 AggregateQualityAssessment],
                               (182) observationResult[0 "this",1 "result"] }
             consequent W1: quantifiedVariable {idx 0 "value"}
                            exists R1 = math:estimateValue [0 var "result", 1 var "value"]

T1 (183-185) quantifiedVariable {idx 0 "valueTyped"}
             forall F1: antecedent and {
                          and { rdf:type[0 "this",1 AggregateQualityAssessment],
                                (184) observationResult[0 "this",1 "result"] },
                          (185) G1 = math:estimateValue [0 var "result", 1 var "valueTyped"] }
                        consequent C1 = rdf:type [0 var "valueTyped", 1 iri math:Quantity]

E2 (186-187) antecedent and { rdf:type[...this, AggregateQualityAssessment], observationResult[this,result] }
             consequent W2: quantifiedVariable {idx 0 "estimator"}
                            exists R2 = math:estimator [0 var "result", 1 var "estimator"]

T2 (188-190) quantifiedVariable {idx 0 "estimatorTyped"}
             forall F2: antecedent and {
                          and { rdf:type[...this, AggregateQualityAssessment], (189) observationResult[this,result] },
                          (190) G2 = math:estimator [0 var "result", 1 var "estimatorTyped"] }
                        consequent C2 = rdf:type [0 var "estimatorTyped", 1 iri math:Estimator]

P  (191-193) antecedent and { rdf:type[...this, AggregateQualityAssessment], (192) observationResult[this,result] }
             consequent Q = or {
               D1 (192): quantifiedVariable {idx 0 "estimand"}
                         exists R3 = math:estimatesEstimand [0 var "result", 1 var "estimand"]
               D2 (193): quantifiedVariable {idx 0 "parameter"}
                         exists R4 = math:estimatedParameter [0 var "result", 1 var "parameter"] }
```

`r3-hand-eval.md` describes the same structure. Its location "continuation lines to 213" is wrong: the module
ends at 193.

## Evaluation (of the pattern with line 180 removed; as committed every row is a parse error)

Bindings on the module as authored: `?law`=L; `?result`="result" (L's index-1 quantified variable);
`?obligations`=O; `?valueExists`=E1, `?valueWitness`=W1, `?value`="value", `?valueRelation`=R1;
`?valueTyped`=T1, `?typedValue`="valueTyped", `?valueObligation`=F1, `?valueGuard`=G1 (the nested `and` member
has no `l:relation`, so it cannot bind), `?valueType`=C1; `?estimatorExists`=E2, `?estimatorWitness`=W2,
`?estimator`="estimator", `?estimatorRelation`=R2; `?estimatorTyped`=T2, `?typedEstimator`="estimatorTyped",
`?estimatorObligation`=F2, `?estimatorGuard`=G2, `?estimatorType`=C2; `?targetPresent`=P, `?estimandCase`=D1,
`?estimand`="estimand", `?estimandRelation`=R3, `?parameterCase`=D2, `?parameter`="parameter",
`?parameterRelation`=R4. Every argument blank node matches on index and term. The integer literals `0` and `1`
in the pattern are the same terms as the module's integers. Result: TRUE.

The five roles match only their own conjuncts. E needs `consequent` and then `exists`; T needs a top-level
`forall`; P needs `consequent` and then `or`. The two E roles differ by relation, and the two T roles differ by
guard relation and class.

| Single mutation | ASK | Reason |
|---|---|---|
| E1 removed | FALSE | no other conjunct's consequent has an `exists` on `math:estimateValue` (E2's is `math:estimator`; P's consequent is an `or`) |
| T1 removed | FALSE | the only other top-level `forall` is T2, whose guard is `math:estimator`, not `math:estimateValue` |
| E2 removed | FALSE | symmetric to E1 |
| T2 removed | FALSE | symmetric to T1 |
| P removed | FALSE | no other consequent is an `or` (removing one disjunct alone also fails) |
| T1 guard `estimateValue` changed to `estimator` | FALSE | `?valueGuard` must be a direct antecedent member with relation `math:estimateValue`; T1 has none now, and T2's guard is `estimator` |
| T2 guard `estimator` changed to `estimateValue` | FALSE | `?estimatorGuard` needs `math:estimator`; neither T1 nor T2 has it now |
| C1 or C2 argument 0 changed to another variable ("result", "this", "value") | FALSE | `?typedValue` / `?typedEstimator` is fixed by the conjunct's quantified variable and by the guard's index-1 argument, so it cannot equal the changed term |
| Guard argument 1 or the conjunct's quantified variable changed alone | FALSE | the three-way sharing of `?typedValue` / `?typedEstimator` breaks |
| R1, R2, R3, R4, G1 or G2 index-0 argument changed from "result" | FALSE | `?result` is fixed to "result" by L's index-1 quantified variable |
| An existence witness's index-1 variable differs from its quantified variable | FALSE | `?value`, `?estimator`, `?estimand` and `?parameter` are each shared between the two positions |
| `math:Quantity` and `math:Estimator` swapped (either one or both) | FALSE | C1 must name `math:Quantity` and C2 `math:Estimator`, each tied to its own guard |
| Conjunction members reordered, argument blank nodes listed in another order, or a variable renamed consistently | TRUE | `l:and`, `l:or` and `l:argument` are unordered object sets; arguments are matched by index; no variable name is a literal in the pattern |

No mutation in the requested list leaves the corrected pattern TRUE.

## Checks 4 and 5

- **Brittleness.** It is not brittle to the harmless changes above. It depends on two conventions: the result
  variable is the law's index-1 quantified variable, and the typing guard is a direct member of the antecedent
  conjunction. Flattening the nested antecedent keeps it TRUE. Exchanging the indices of the two outer
  variables would make it FALSE. That exchange is not obviously harmless, because index 0 is the focus variable.
- **Prefixes.** `g`, `l`, `m` and `rdf` are declared (135-138), and those four are used. `a` is the keyword.
- **Paths and syntax.** The property paths are `l:forall/l:consequent` (143), `l:antecedent/l:and` (152 and
  165) and `l:consequent/l:or` (171). Each is a plain sequence of IRIs. No blank-node property list appears
  inside a path. Blank-node property lists appear only as objects of simple IRI predicates, in the first
  position or after `;` or `,`, and their inner predicates are simple IRIs. The SPARQL 1.1 grammar allows that
  (ObjectListPath and ObjectList reach TriplesNode). The only syntax defect is R1.
- **Rationale (182).** Its wording matches the corrected pattern. It names the four relations whose first
  argument is the law's quantified result variable (true for both witnesses and guards). It says each witness
  is the second argument (true) and each typing conjunct is guarded, quantified and typed on one variable
  (true). Each claimed falsifier is borne out in the table. It makes no claim about the outer guard or the
  per-conjunct assessment guards, which stay unpinned (O5 from the earlier review, unchanged). While R1 stands,
  the rationale is not true of the committed assertion, because the assertion does not run.
- **`r3-hand-eval.md`.** Its structure and mutation reasoning are correct for the corrected pattern. It is wrong
  that the committed ASK is true (R1), and wrong that the tree runs to line 213. It does not separately work the
  class swap or the existence-subject mutation, which I added above.

## OPTIONAL findings

- **O1.** Add a SPARQL parse step to the evidence for any changed `saPattern`, beyond `rapper` and the prefix
  script. Run the evidence at the committed head.
- **O2.** `pr-body.md` still cites "PASS at `909446ec8`" and describes the older assertion. After the fix, update
  it to the new head and to what the pattern now pins.
- **O5 (carried).** The outer and per-conjunct assessment guards remain unpinned. The rationale makes no claim
  about them.

## Whole-branch coverage

This verdict cannot rest on the earlier PASS plus this delta as a PASS. The delta is confined to one assertion,
and the rest of the branch is unchanged since `review-1893-at-909446ec8.md`. But the delta makes that assertion
unparseable, so the quality slice specification fails at the head. Once line 180 is removed, and given no other
change, the whole-branch verdict can rest on the earlier PASS plus this review's evaluation. That earlier PASS
covered the unit as terms content only, not as closing the issue's execution requirements. The PR body calls it
"Part of" the issue and assigns execution to another issue. I did not re-judge that scope here.

## Not shown

- No SPARQL engine evaluated either version. The parse failure follows from reading the locked parser's
  end-of-input check and the brace count. The truth values come from evaluating by hand.
- No slice specification, producer, gate or reasoning run.
- The PurRDF lexer's handling of `}` was not read; I assumed it yields a token, which the grammar then refuses.

VERDICT: BLOCKED
