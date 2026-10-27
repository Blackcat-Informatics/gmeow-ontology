# Issue #1993: Use native JSON for complete class-source-admission observations

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, cross-cutting, logic, dependencies, rust, complexity-hard

## Body

Move the complete class-source-admission observation channel to PurRDF native JSON under Principles 4 and 7. Reuse the existing canonical records and dependency direction, the complete source-issue codec and the retained-term owner.

Acceptance:

- Implement native JSON for all five records: SourceAdmissionObservation, ClassAdmissionObservation, ClassAdmissionWorld, ClassAdmissionSourceWorld and RefutationPremise. Implement all three FragmentBoundary variants and all three FragmentFamily variants, reusing all eight RefutationSourceIssue variants and complete quoted TermValue identity.
- Wire the actual per-input class-admission cache through existing cached_native_inputs, its aggregate channel publisher, authenticated aggregate reader and frozen expected-record reader. Preserve exact source/profile inputs, operation identity, lazy parse sharing and cache-hit behavior. Share the existing parse initializer between Serde and native operations; do not add a cache framework or duplicate parsing paths.
- Preserve the exact 32-byte input digest with strict unsigned byte admission, strict u64 assertion counts, field and vector order, named-record admission, typed-map validation before last-wins replacement, Combined-set ordering/deduplication and the native default depth policy. Unknown fields remain tolerated; known duplicate fields, malformed variants, positional records and malformed shadowed values fail.
- Preserve all three graph fields as required-nullable through the existing term optional adapter. Plain refusal retains missing/null to None. Hydration preserves raw observation data and never executes source admission, reconstructs RDF or silently normalizes identities.
- Keep existing explicit validate calls, source selection, profile selection, digest checks, exact expected-record comparison, semantic separation and authenticated corpus assertions unchanged.
- Retain all shared Serde derives, attributes and variant order for genuine CBOR receipts, certificate and diagnostic records, mixed NativeCaseObservation execution JSON and broad CaseOutputs.verdicts. Those separate transports remain with their current owners; do not add a native-to-Serde bridge or expand into a full reasoning-result or CaseOutputs migration.
- Extend existing safe authored controls through the selected native cache and record boundary, add bounded full-wire/quoted-term/causal/admission coverage, and compile actual compiler, logic, conformance, pipeline and developer CLI callers. Run normal hooks. Tests use authored temporary inputs only; no corpus construction or authenticated corpus execution.

Remaining contracts: #1776, #1777, #1336 and #1962. Workspace Serde retirement, the separate mixed execution/verdict/CBOR operations and actual corpus/release acceptance remain unfinished in their existing owners. The standing user exception prohibits local optimized producer, corpus/fixture production, synchronization and full-gate execution and authorizes sequential merges with CI waived. Report unverified acceptance accurately.


## Comments (0)

