# Current validation

Scope: canonical value-validation work already integrated on main, plus the compound native violation-lowering repair and radial worked-example wording. Authoritative plan: plan.md. Principles 4, 7, 17 and 18.

| Criterion | Status and evidence |
| --- | --- |
| Initial full native measure law | FAILED: tests.log, UnsupportedConsequent before execution; retained as the regression's original observation |
| Boolean branches, membership and numeric ordering | PASS: tests-final.log, 152 selected cases. Original typed values retained; positive-binding checks and all-or-nothing residue retained |
| Native exact numeric/pole matrix | PASS: actual formula extraction, native lowering and integrity execution over 17 source values; all four comparison relations additionally exercised at negative/zero/positive boundaries. No corpus read or construction |
| Generated finite and signed-pole validation | PASS: both generated-shape finite-numeric tests in tests-final.log |
| Builtin consumers | PASS: selected native binding, canonical rule identity, counterfactual and reference resolver cases in the same 152-test run |
| Source lint | PASS: expanded repair, lint-final.log, exit 0 |
| Formatting | make fmt completed successfully, fmt.log |
| Strict producer-independent Clippy | PASS: make clippy-prebuild, clippy.log, exit 0; all producer-independent workspace targets and warnings denied |
| Source annotation/translation completeness | Inspected builtin coat and all seven paired French/Mandarin entries; no annotation edits in this patch |
| Radial worked-example status | Design, two examples, structural/competency/conformance cells inspected; all queries, expected rows and five actual flagship contracts preserved |
| Normal commit/push hooks | PASS: normal signed commit and push completed; commit.log and push.log. No hook bypass |
| Integration candidate | PASS: fetched main has no advancement beyond the branch's starting base; merge-tree completed cleanly. Candidate is the tested source tree, with only explanatory comments added after the final behavioral build |
| Whole-corpus/full gate | NOT RUN; local producer, fixture generation and full sync/gate not authorized |
| Hosted CI | Not a merge prerequisite under explicit user instruction: merge them one by one, ignoring CI |
| Independent review | Unavailable: no reviewer dispatcher exposed. Local reports are self-reviews, not independent approval |

All selected local checks passed. No whole-corpus/full-gate pass or independent approval is claimed. Publication awaits current PR feedback assessment and ghprsq.
