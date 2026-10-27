# Issue #2016: Use complete native JSON for GMN gate observations

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

Move the complete GMN gate-observation publication and authenticated reader to PurRDF native JSON. The complete nested correspondence family already has native codecs; this delivery covers the five remaining gate records. Principles 4 and 7.

Frozen scope: the seven-field gate Observations and five-field ShippedWitness, plus Gmn1CodebookDigestMismatch, Gmn1CodebookDigestReport and Gmn1PackRootReport at their existing owner. Reuse native Correspondence/LegPath. Move the existing exact-width leg-pair reader to the correspondence owner for both real grammar/gate callers; preserve its messages and indexed error paths without a generic tuple framework.

Acceptance:

- Implement native ToJson/FromJson for all five records and wire the actual lang_projection gate publisher and authenticated contract_fixtures gate reader through them. Remove obsolete Serde from those five records after verifying actual users. Retain the real pack channel and all broader owner contracts.
- Preserve full declaration-order compact JSON bytes, no terminal newline, required records/vectors/booleans, raw strings and evidence order, unknown record fields, known-duplicate rejection, optional declared_root and leg_pair missing/null admission, exact leg-pair width and all nested correspondence policies. Validate every duplicate map occurrence before last-wins insertion. The usize checked field uses the existing strict unsigned admission, refusing negative zero, signed, fractional, exponent and overflow inputs.
- Hydrate reports and witnesses directly without recomputing hashes, classifying evidence, sorting raw fields or requiring a clean verdict. Preserve existing report calculations, failure classes, source paths, recorder behavior, complete action/selector authentication and every corpus assertion.
- Freeze and compare the complete original channel encoding using bounded authored values before retiring the original encoder. Add two focused authored controls for whole-channel byte/raw-evidence fidelity and admission/refusal. Tiny independently authored RDF may exercise the existing report functions directly; never call Recorder::finish, discover a corpus, or produce canonical fixtures from tests. Reuse the two existing grammar wire/admission cases to cover the moved shared leg reader.
- Compile actual affected owner/caller packages with warnings denied, run those four exact authored controls, complete helper/static source-recipe checks and normal hooks, and independently review the change. No canonical corpus/source compilation, optimized producer, synchronization or full gate under the standing exception. Authenticated corpus/browser/installed/release acceptance remains unverified; CI is waived, not passing.

Remaining contracts: #1776, #1777, #1336 and #1962, including the separate pack channel, prepared operators, other binary owners and full production acceptance. This child closes only the complete GMN gate-observation owner.


## Comments (0)

