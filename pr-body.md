Competency rationale literal validation now returns `Diag` with a private typed source instead of a string error. Both model callbacks use the same native datatype/component validation; invalid direction tokens also become typed before Serde presentation. The native IRI failure remains in the cause chain until the generic Serde error boundary.

The four wire fields, nullable facets, original lexical bytes and native admission order are unchanged. The existing malformed control now checks the native cause and invalid datatype serialization; its previous malformed inputs and the exact UTF-8/null wire control are retained. Independent source review passes. Principles 4 and 7.

The acceptance/feedback query-directory read also has its precise audit annotation at the construction site. Its actual callers, required disk reads and catalog/profile checks are unchanged. The existing seal requires an adjacent comment; the earlier bundle annotation does not cover this separate read. Both fixes share the same source-policy qualification and delivery.

Strict actual host callers, both authored transport controls, helper hygiene and independent review pass. The actual read-only crate check confirms both repaired findings are gone: 13 findings became 11, all unchanged from the prior baseline. The remaining source-span binding and ten stale browser attestations still fail the whole gate. Normal signed commit and push hooks pass. CI is waived under the standing user instruction. No optimized producer, corpus construction, synchronization or full gate. Generic Serde errors may preserve only display text; no end-to-end cause guarantee is claimed.

Closes #2011.

Closes #2012.

Remaining contract: #1761, including its other source-policy findings, shipped browser attestations and full gate acceptance.
