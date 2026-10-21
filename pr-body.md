Affect ingestion still depended on serde-derived records while its native JSON owner and concrete callers were ready to share one implementation.

Move the five capture records/enums to purrdf_lex record parsing and writing, and wire actual CLI ingest/recover plus validate decode callers through them. Preserve known duplicate refusals, ignored unknown members, optional null/omission, threshold/map semantics, declared record order and finite typed numbers. The documented native schema requires object records and string enums; incidental positional-array/singleton-map forms now refuse. Typed f64 inputs normalize to native binary64 output. Recovery refuses nonfinite RDF scores/thresholds before JSON export, with subject and predicate in its error.

Principles 4 and 7: shared implementation and explicit schema admission. All38pure tests, strict producer-independent prebuild Clippy, formatting, combined helper gate and actual selected source-only recipe passed. Installed/embedded CLI, authenticated corpus conformance and full gate remain unrun under the campaign exception. No model execution or local corpus production.

Refs #1776
