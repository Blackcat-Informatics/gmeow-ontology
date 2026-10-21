Finance and transcode introduced source-gate failures: duplicate profile diagnostics, two private RDF collection implementations, and JSON record macros without an authenticated source-input owner.

Both evaluators now use one diagnostic constructor. Finance uses the pinned strict RDF collection reader, and its synthetic list builder uses the shared collection builder. JSON macros are recognized only through a qualified external dependency alias proven against the canonical pinned package, exact revision, lock entry and selected Cargo dependency. Fake, mutable, patched and shadowed owners refuse; nested input ownership remains enforced.

Principles 4 and 7: one implementation owner and explicit, verifiable input identity. No debt ceiling or gate is weakened.

Validation:72focused build-input/native math/finance tests passed; full helpers-hygiene and actual source-only producer recipe passed. Strict prebuild Clippy passed. Production corpus, installed consumers and the full gate remain unrun under the campaign exception. The separate ONNX parity failure remains open.

Refs #1894, #1776
