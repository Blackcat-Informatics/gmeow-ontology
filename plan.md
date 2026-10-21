# Browser witness JSON ownership

Migrate the dev-only JSON readers in query-wasm, mcp-core-wasm and mcp-wasm to purrdf_lex::json. Scope is exactly their three manifests and three witness integration tests. Preserve all actual requests, engine calls, source/bundle authentication, frozen attestations and byte parity comparisons. Remove each crate's dev-only serde_json dependency; root owns lock resolution.

Use native record::read_document for existing free-form Value semantics. Separate query decoding from file I/O and extract existing MCP envelope/payload assertions into actual witness helpers. Tiny independently authored JSON inputs exercise those same helpers without snapshots, model/corpus work or an alternate decoder. No new production behavior or runtime dependencies.

Root owns formatting, builds, tests, gates and delivery. This is one bounded continuation; full JSON migration and real browser/bundle acceptance remain open.

Root compile review expanded the source boundary to the existing MCP attestation refresh example, the sole remaining caller of the removed dependency. Migrate its two reads in the same unit, preserve the producer path and byte publication, compile it but never execute it. Normal Cargo resolves Cargo.lock.
