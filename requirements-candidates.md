# Requirement candidates — issue #1717

Lines from the issue body and comments that matched a lexical rule, copied
verbatim with the rule that fired. This is a CANDIDATE list: rules both
over-match (a "should" in passing) and under-match (a requirement stated
without a keyword). The issue-analyzer keeps or drops each row, adds what
the rules missed, and accounts for every silent comment. Code fences are
skipped.

| ID | Source | Line | Rules | Text |
|---|---|---|---|---|
| B1 | body | 24 | checkbox | - [ ] **finance** — monetary amounts, rates and prices carry `math:Quantity` dimensions; interest/return computations reference `math:` operations. |
| B2 | body | 25 | checkbox | - [ ] **risk** — risk scores/likelihoods ground in the probability layer (`Prior`/`Posterior` where Bayesian; never bare floats). |
| B3 | body | 26 | checkbox | - [ ] **quality** — quality metrics as dimensioned quantities; aggregate scores with declared estimators. |
| B4 | body | 27 | checkbox | - [ ] **evidence / trust** — strength/confidence values ride the probability-vs-confidence never-conflate rule (P19): confidences stay `logic:confidence`, probabilities require a model. |
| B5 | body | 28 | checkbox | - [ ] **model-serving** — tensors/metrics ground in the linear-algebra layer (`TensorComputationGraph`, `math:LossFunction`). |
| B6 | body | 29 | checkbox | - [x] ~~**diagnostics** — extend the 3-ref toehold to graded-severity quantities.~~ **Struck: already grounded.** |
| B7 | body | 43 | list-under-requirement-heading | - No inherently quantitative slice sits at `math:` 0. |
| B8 | body | 44 | list-under-requirement-heading | - Each grounded slice records `math:` in its manifest `gmeow:sliceDependsOn` set. |
| C1.1 | comment 1 (paudley, 2026-09-05T03:44:35Z) | 35 | modal | That is the intended semantics stated in prose, in `skos:definition`, where nothing can enforce it. Turning it into an EL-safe `logic:` axiom in `module.ttl` — and recording `math:` in the manifest's `gmeow:sliceDependsOn` — is exactly the grounding this issue asks for, with the modelling decision already made and documented. No new ontology design is required, which is not true of finance (currency-as-dimension) or model-serving. |
| C2.1 | comment 2 (paudley, 2026-09-05T17:06:44Z) | 5 | modal | > `quality` is the best first cluster — its commitment is already written, just not axiomatised … Turning it into an EL-safe `logic:` axiom in `module.ttl` — and recording `math:` in the manifest's `gmeow:sliceDependsOn` — is exactly the grounding this issue asks for, **with the modelling decision already made and documented. No new ontology design is required.** |
| C2.2 | comment 2 (paudley, 2026-09-05T17:06:44Z) | 49 | scope-out | - **(c) Declare it out of scope** — accept that a quality *verdict* is not inherently dimensioned and that the grounding belongs to the `observations` stack it already reuses, which **is** grounded. Then `quality` legitimately sits at `math:` low and the acceptance criterion "no inherently quantitative slice sits at `math:` 0" needs its premise re-examined for this slice. |
| C2.3 | comment 2 (paudley, 2026-09-05T17:06:44Z) | 55 | modal | - The suggested order **quality → finance → model-serving** should be dropped. `quality` is *harder* than I claimed because its grounding is conditional on a dimension, not universal on a class. |

## Comments that matched no rule

None.
