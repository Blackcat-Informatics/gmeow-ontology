# Focused delivery review

All JSON parsing in query-wasm, mcp-core-wasm and mcp-wasm uses the native owner. This includes the refresh-witness-mcp example found by compilation after removal of the old dev edge. Normal Cargo updates only the three dependency edges in Cargo.lock. No production runtime dependency or public API changes.

The helper extraction preserves actual witness assertions and their authenticated callers. Frozen attestations, full/segmented engine comparisons, routing/verdict checks and judgment RDF parsing remain. Independent review found no blocker, including the later example correction. Native numeric equality admits 1.0 as equal to request id1; unchanged exact frame/attestation comparisons still enforce actual wire bytes. The maintainer example writes the original frame and never reserializes the native JSON value.

Initial compile failed on the missed example, terminal EXIT2, no tests ran; corrected source then executed19 pure controls and all passed. Source-only producer recipe before/after comparison passed. Actual authenticated witnesses and browser parity are separate unrun acceptance. No corpus or attestation producer was run.
