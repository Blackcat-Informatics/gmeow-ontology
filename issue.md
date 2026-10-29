# Issue #2061: Preserve complete modal instruction interning with native JSON

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, logic, dependencies, rust

## Body

Move the complete finite-modal instruction interning key operation to native PurRDF JSON under parent #1776. Principles 4 and 7.

Acceptance:

- Complete all six private key owners: NodeId, Accessibility, Quantifier, TemporalOperator, AtomTerm and every one of the ten Instruction variants. Reuse the complete native compiler Term writer, including its original four-component RDF literal representation. No opaque wrapper, second term codec, parser, framework or public API.
- Preserve every original key byte: numeric child/axis indices, unit and external variant tags, declaration-order fields, ordered child vectors and the two-element Implies/Iff payloads. Do not substitute accessibility IRIs, normalize admitted terms or alter child-before-parent insertion, canonical formula identity, sharing, memoization, evaluation charging, proof rollback or assessment anchors.
- Mechanically extract the original key encoder and retain the actual lower/interner caller before capture. Extend only the existing shared_dag_work_is_charged_once_and_uncommitted_parent_proofs_are_removed control to capture a complete original-key atlas covering all ten variants, every unit tag, both atom forms and rich literal/string metadata. Also capture actual lowered instruction keys and addresses. Freeze the complete original bytes before replacement; compare through the actual native helper against literal references, without recalculating expectations from the replacement.
- Use the existing native compact String writer for this admitted finite in-memory key. Remove the unreachable serialization-only Malformed mapping while retaining every genuine semantic admission, unsupported-fragment, journal-limit, budget and cancellation refusal. Keep Program lowering's semantic Result API unchanged. No new error framework or fallibility churn.
- Remove obsolete Serde derives from these six types after confirming their only serializer was the replaced interner and that no reader/CBOR caller needs them. The physical plan remains a private execution DAG, never a second authored formula IR or external hydration authority.
- Qualify the existing DAG-sharing/proof-rollback, equivalent-operand/world-binder and nested-temporal shared-budget/memo controls on the same original/final producer-independent library graph. Keep every original assertion. Add no test function or generic term/JSON compliance matrix. Run warnings-denied actual logic, pipeline, validation and developer CLI consumers, formatting/prose, helpers, static recipe, independent review and normal signed hooks.
- Preserve the seventeen-site broader contextual/provenance identity operation and sibling native-execution CBOR work. Charge assessment from 18:30 UTC October 29 and land by 02:30 UTC October 30, including qualification and publication; never reset the clock. Parent #1776 stays open for every remaining requirement.

No local optimized producer, canonical corpus construction, sync or full gate. Authored models do not establish authenticated corpus, installed/browser/release or performance acceptance. Broader CI remains explicitly waived. Preserve held LinkML and protected donors.


## Comments (0)

