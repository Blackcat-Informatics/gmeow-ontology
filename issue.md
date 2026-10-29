# Issue #2056: Preserve complete scoped witness receipts with native JSON

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, logic, dependencies, rust

## Body

Move the complete scoped-witness receipt JSON writer and intrinsic reader to native PurRDF JSON under parent #1776. The actual RDF publisher is the pipeline witness projection; full reasoning-result RDF hydration separately uses native-execution CBOR and remains unchanged. Principles 4 and 7.

Acceptance:

- Complete all nine wire owners: WitnessDerivation, WitnessScope, WitnessOrigin, WitnessHead, WitnessStatement, WitnessPosition, SelectedLogicalWorld, LogicalGraph and DomainProfile. Reuse existing native graph/profile writers and the complete retained RDF term adapter for statements and frontier values. Hydrate private selected-world fields only inside their owning module. No partial wrapper, Serde fallback, new RDF codec or generic framework.
- Preserve the canonical gmeow-witness-v2 prefix, complete field/member order, enum tags, unsigned byte arrays, ordinal, all heads/positions/premises and exact RDF term values. Require exact 32-byte commitments and checked usize admission using native array/integer primitives; share the necessary small schema adapter rather than duplicate it.
- Preserve validation before encoding and after decoding, unsupported-version refusal, intrinsic address/proof checks, selected-domain membership and current publisher head/premise binding. Hydration is observational and does not establish current source or execution authority. Native parse/record failures retain the Physical category, operation context and original typed cause through fresh diagnostics.
- Use a declared native named-record profile: all members required, known duplicates, unknown members and positional struct arrays refused. No missing array becomes an empty proof and no missing authority is invented.
- Correct the existing pipeline witness receipt control to select the actual v2 prefix instead of its stale v1 filter. Keep its nonvacuous source/domain counts and all original assertions. Do not add a legacy fallback.
- Before encoder replacement, capture complete actual published wire strings and witness IDs for both Rule and NonemptyDomain origins from that existing authored pipeline control. Freeze every original byte and ID. Identity algorithms use native framed term keys/SHA-1 and typed Debug/BLAKE3, not Serde JSON; leave them untouched.
- Native contracts include production source identity, so source edits legitimately change freshly generated contracts and witness IDs. Replay historical typed receipts and their actual IDs byte-for-byte, then separately validate current generated receipts. Never substitute historical contracts into execution, recalculate expectations from the replacement, or claim old contracts establish current authority.
- Extend only existing controls for bounded selected-format/required-field/digest/ordinal and semantic refusals. Reuse existing library receipt integrity, selected-domain integrity, actual pipeline publication and wrong-result/missing-head/tampered-position controls. No new test function or generic RDF/JSON compliance matrix.
- Qualify actual errors/logic and downstream pipeline/validation/developer CLI consumers with warnings denied, the selected existing authored controls, independent source/trust and combined review, formatting, helpers, static recipe and normal signed hooks. Original and final runs use the same selected consumer graph. Charge readiness from 15:05 UTC October 29 and land by 23:05 UTC, including qualification and publication.

Shared Serde/frontier support remains necessary for real native-execution CBOR and stays under the parent. No production JSON RDF-import caller exists for from_wire; preserve that distinction. No local optimized producer, canonical corpus construction, sync or full gate. Authored controls do not establish authenticated corpus, installed/browser/release or performance acceptance. CI remains explicitly waived; every remaining parent requirement stays open. Preserve the sibling CBOR work.


## Comments (0)

