Split out of #1439, whose first checklist item — the `gmeow:Quantity` ≡ `math:Quantity` reconciliation — is complete and verified. This carries the remaining six clusters so the reconciliation can close without dragging an open sweep behind it.

Child of #1433.

## Why

The `.goals` MAXIMAL GROUNDING mandate says the three grounding slices ground all other slices. Measured against `main` @ c2b03aba6, `math:` references in each slice's `module.ttl`:

| slice | `math:` refs | note |
| --- | --- | --- |
| affect | **38** | the precedent — nearest-prototype on math geometry |
| diagnostics | 3 | already grounded via `gmeow:gradeOrdinal` — see struck item below |
| quality | 1 | effectively ungrounded |
| finance | **0** | |
| risk | **0** | |
| evidence | **0** | |
| trust | **0** | |
| model-serving | **0** | |

Numbers in those slices are ungrounded scalars.

## Work (one PR per cluster is fine)

- [ ] **finance** — monetary amounts, rates and prices carry `math:Quantity` dimensions; interest/return computations reference `math:` operations.
- [ ] **risk** — risk scores/likelihoods ground in the probability layer (`Prior`/`Posterior` where Bayesian; never bare floats).
- [ ] **quality** — quality metrics as dimensioned quantities; aggregate scores with declared estimators.
- [ ] **evidence / trust** — strength/confidence values ride the probability-vs-confidence never-conflate rule (P19): confidences stay `logic:confidence`, probabilities require a model.
- [ ] **model-serving** — tensors/metrics ground in the linear-algebra layer (`TensorComputationGraph`, `math:LossFunction`).
- [x] ~~**diagnostics** — extend the 3-ref toehold to graded-severity quantities.~~ **Struck: already grounded.**
  `gmeow:gradeOrdinal` embeds each truth-order chain into (ℤ, ≤) — severityInfo 0, Note 1,
  Warning 2, Error 3 — with the one-way bridge `math:integers math:formalizes gmeow:gradeOrdinal`
  declared in the math slice, and the term states it is "the Principle-19 MATH grounding of the
  truth ordering" and "NOT a parallel gmeow:Grade subtree". Severity is ORDINAL, not dimensioned;
  pushing it toward `math:Quantity` would conflate an order-embedding with a magnitude. The low
  reference count measures how compactly it was done, not absence.

## Dependencies

Blocked by the probability and statistics layers **only where a cluster needs them** — risk and evidence. finance, quality, model-serving and diagnostics are not blocked and can start now.

## Acceptance

- No inherently quantitative slice sits at `math:` 0.
- Each grounded slice records `math:` in its manifest `gmeow:sliceDependsOn` set.

## The pattern to follow

`affect` (38 refs) is the worked precedent. `observations` is the smaller one, and is now complete end to end: it declares `slices/math` in `gmeow:sliceDependsOn`, consumes `math:Quantity`, and its structural tests *assert the legacy `gmeow:` quantity vocabulary is absent* rather than merely not using it — see `slices/core/observations/tests/structural.ttl:136`. Grounding a slice is not finished until the retirement is guarded that way.

## Complete delivery decomposition

The original requirements remain unchanged above. Diagnostics remains already grounded. The remaining clusters map exactly once to these independently accepted deliveries; all block this parent, which stays open until every contract and production acceptance passes.

- [ ] #1893 — quality
- [ ] #1894 — finance
- [ ] #1895 — risk
- [ ] #1896 — evidence-trust
- [ ] #1897 — model-serving

Delivery order starts with quality: explicit quantitative/aggregate subtypes avoid the categorical-result and multidimensional pairing defects described in the existing comments. Other clusters retain their real prerequisites; no requirement is dropped or reclassified as delivered. Principles 4, 7 and 19.
