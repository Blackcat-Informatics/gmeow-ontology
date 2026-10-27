# Issue #2014: Use complete native JSON for grammar observations and correspondence records

Source: https://github.com/Blackcat-Informatics/gmeow-ontology/issues/2014
State: open

## Body

Move the complete grammar-observation publication and authenticated reader to PurRDF's native JSON owner. This includes the full nested correspondence family used by each emission, not only the outer observation record. Principles 4 and 7.

Frozen scope: Grammar and Formalism; three grammar observation records; `Correspondence`, `LawClaimIr`, `RecoveryCaseIr`, `CorrespondenceCaveat`, `AxisEvidence`; their seven closed unit enums; and the four recursive `LegPath` variants. Add the missing native Grammar record and four Formalism variants at their existing owner; reuse the native RuleExpr, IngestDiagnostic, Formula, PreservationKind and both checked NumericLiteral codecs.

Acceptance:

- Implement native `ToJson`/`FromJson` for the complete eighteen-type closure. Preserve field names, declaration-order compact bytes, explicit nulls, requiredness, raw booleans, vector order, recursive legs and exact two-element leg pairs. Accept unknown record fields, reject duplicate known fields and malformed enum/row/type/unsigned values through existing native record helpers. Typed maps retain the last duplicate value only after validating every occurrence.
- Preserve full RDF literal identity and the existing collection policies. Caveat comments hydrate as a normalized, sorted/deduplicated, nonempty literal set. Loss evidence uses the same admission/set policy but may be empty. Reuse the existing literal owner for admission and preserve each existing wire: the collection serializer respects an explicit datatype, while the single-literal serializer uses its effective datatype. Never silently repair a malformed explicit datatype during encoding. Do not change NumericLiteral checks or `AxisEvidence::content_key()`.
- Construct decoded records directly. Do not run correspondence, recovery or axis constructors that would sort, validate or repair additional raw fields beyond existing Serde admission. Retain shared Serde implementations for the genuine LogicProgram and other remaining owners. Retire Serde only from the three private grammar-channel records.
- Wire the actual `lang_projection` grammar publisher and the authenticated `contract_fixtures` grammar reader through native JSON. Preserve complete action/selector authentication, all corpus assertions and the other two real Serde channels. No fallback to Serde in the migrated grammar path. Provide one actual publication method usable by the existing authored grammar control without driving the corpus producer.
- Extend `native_reparse_is_bound_to_the_actual_emitted_bytes` through the actual channel codec and retain its tamper/absent-result assertions. Add bounded complete-family wire and admission controls covering literals, all enum values, nested legs, both grammar outcomes, nullable/default boundaries and raw evidence fields. Reuse the two existing NumericLiteral controls. No generic JSON/RDF conformance suite, constructor mirror tests or corpus production from tests.
- Compile affected production owners/callers with warnings denied, run the selected authored controls and normal hooks, and review independently. Authenticated corpus/browser/installed/release acceptance remains unverified under the standing no-local-producer/corpus/sync/full-gate exception. CI is waived, not recorded as passing.

Remaining contracts: #1776, #1777, #1336 and #1962, including the separate GMN gate/pack channels, complete prepared operators, other binary owners and full production acceptance. This child closes only the complete grammar-observation owner.


## Comments (0)
