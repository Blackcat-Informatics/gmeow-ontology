# Issue #1991: Use one native JSON owner for complete list-corpus observations

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

Move the complete list-corpus observation operation to PurRDF native JSON under Principles 4 and 7, with one canonical shared DTO instead of the duplicated publisher and authenticated-consumer declarations.

Acceptance:

- Define the complete twelve-field ListCorpusObservation and channel identity in the existing logic crate, re-exported from reason::refute. Pipeline already depends on logic; no new crate or dependency is needed. Replace both current DTO declarations, wire the actual publisher and authenticated reader, and retain every corpus assertion.
- Implement native JSON for all eight RefutationSourceIssue variants, including the three not currently collected by the list producer. Preserve explicit variant names, every string field and complete quoted TermValue owner/operand data, including blank scope, literal metadata and recursive statement identity, through the existing native term codec.
- Retain the enum's Serde derives, attributes and variant order for genuine RefutationCertificate JSON and ClassDiagnosticOutcome admission/CBOR consumers. The new observation DTO is JSON-only and needs no Serde. Preserve the existing wire envelope and native default depth policy.
- Preserve all twelve fields, u32/usize widths, diagnostics and defect order, typed map last-wins behavior and complete recorded causal refusals. Require named records and fields, tolerate unknown fields, refuse known duplicates, positional records, malformed outcomes and malformed shadowed map values. Preserve raw observations without resource inference, RDF-role normalization or rerunning admission.
- Keep catalog selection, projection, source filtering, native admission, list-defect collection and authentication unchanged. Corpus-success conditions remain in the authenticated consumer, not the data decoder.
- Extend the two existing authored three-statement controls through the actual publisher and native reader. Add bounded full-wire, all-variant, quoted-term, causal and admission coverage. Compile actual logic, pipeline and developer CLI callers and run normal hooks.

Remaining contracts: #1776, #1777, #1336 and #1962. Full certificate/class-diagnostic migration, workspace Serde retirement and actual corpus/release acceptance remain with their existing owners. The user retains the restriction against local optimized producer, corpus production, synchronization and full gate execution, and authorizes sequential merges with CI waived. Report unverified acceptance accurately.


## Comments (0)

