# gmeow-docs

`gmeow-docs` is the Rust-owned documentation core for the GMEOW ontology. It
builds a single typed `DocsModel` directly from the native slice catalog: the
slices, their manifest metadata, their artifacts (by digest, never by embedded
bytes), the vocabulary terms parsed out of each `module.ttl`, and the
cross-slice dependency edges from the ownership analyzer.

The model is PyO3-free and fully deterministic — every collection is sorted by a
stable key so the serialized model is byte-reproducible. Python bindings live in
`src/py.rs` and are folded into the unified `gmeow_native` cdylib (#630) as the
`gmeow_native.docs` submodule.

Renderers (Markdown / HTML), linkage diagrams, lint, and bundle wiring are built
on top of this model in later tasks of #853; this crate owns the model itself.
