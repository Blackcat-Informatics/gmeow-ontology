Source inspection narrows the CI failure: doc_render.rs is a live module declared
by docs-print/src/lib.rs and called by render.rs. It must not be deleted.

In crates/xtask/src/producer.rs, resolve_source_selection normalizes package IDs
through metadata_package_ids / explicit_package_id. The compiler-artifact and
build-script-executed branches of build_producer instead use raw Cargo package_id
strings with workspace path substitution only. Thus a Cargo ID whose fragment is
only the version cannot match the explicit name@version selected unit identity.
The artifact filter silently omits the selected package, then the final read seal
correctly refuses its source. This is a source-supported cause to reproduce with
synthetic Cargo metadata/messages; the remote compiler-artifact transcript itself
was not uploaded, so that exact raw message remains unavailable.

Normalize artifact and generated-input owners using the same exact metadata map,
reject unknown package IDs, and preserve the selected-unit and compiler-read seals.
Regression coverage must include implicit workspace identities, generated owners,
explicit IDs, unknown/contradictory IDs and unselected artifacts. No CPU or
environment fingerprint is needed to test message identity binding.
