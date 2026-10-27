# Issue #2003: Use native JSON for complete affect worked-envelope observations

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, cross-cutting, logic, dependencies, rust, complexity-hard

## Body

Move the complete affect worked-envelope observation channel to PurRDF native JSON under Principles 4 and 7, including its actual producer and authenticated typed consumer.

Acceptance:

- Implement both complete records: pipeline WorkedEnvelopeObservation (observation_present and envelope) and logic-compile WorkedEnvelope (intensity and dimensions). Wire record_worked_envelope and the existing authenticated correspondence_lower control through the native owner. Preserve the current channel path and fixture export registration.
- Preserve declaration-order compact bytes without a terminal newline. Intensity and both dimension cells remain raw strings; preserve every row, duplicate dimension identity and supplied order without normalization. Reuse the shared read_string_row<2> admission for exact two-string dimension rows.
- Require named records and every nonoptional boolean, string and collection field. Reject malformed or duplicate known fields while accepting unknown fields under the native default depth policy. Preserve false observation_present as observation data; hydration must not compute or validate affect geometry.
- Verify all actual consumers before removing the two records' transport-only Serde derives. The separate XML renderer consumes typed fields and remains unchanged. Do not add a generic bridge, redundant schema owner or legacy alias.
- Preserve source selection, original-observation presence evidence, geometry computation, XML emission and loss-ledger algorithms. Only serialization becomes infallible; existing source and geometry failures must still propagate before publication.
- Extend render_emits_both_vocabularies_and_an_envelope through native bytes without weakening any vocabulary, intensity, dimension or no-fabricated-value assertion. Add bounded complete-wire and malformed-admission coverage for both records. Keep the authenticated worked-example corpus control intact and outside local authored-case selection.
- Compile actual logic-compile, pipeline and developer CLI callers and run normal hooks. Selected runtime controls use explicit tiny authored data only. Tests must never construct the repository corpus. Corpus consumption requires its exact authenticated producer selector.

Remaining contracts: #1776, #1777, #1336 and #1962. The schema enrichment operation, complete grammar/correspondence family, other observation owners, workspace Serde retirement and actual corpus/release acceptance remain unfinished in their existing owners. The standing user exception prohibits local optimized producer, corpus/fixture production, synchronization and full-gate execution and authorizes sequential merges with CI waived. Report unverified acceptance accurately.


## Comments (0)

