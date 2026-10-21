# Root source audit

Root inspected the five record implementations, original field declarations and omissions, both docs adapters, MCP cost/envelope path, feature-core dependency selection, existing test boundary changes and complete new controls. Independent review is retained separately.

A source-only Python audit extracted the original authored card_json_schema JSON macro from the base source, expanding only its existing string_array helper and Rust string continuation syntax. The new complete EXPECTED_SCHEMA literal is semantically equal to that original document and byte-equal to its recursively sorted two-space pretty JSON plus one LF. No Rust, corpus or producer executed for this comparison. The actual native publication adapter remains subject to its selected test.

Initial normal formatting exposed a giant schema expression. The implementer replaced it with readable named native Object locals without adding a framework or decoder, then refroze. The touched docs comment now correctly distinguishes shared record shape/values/tier from different direct and MCP member ordering. Root reran make fmt and git diff --check successfully after refreeze.

Normal Cargo resolution adds only the direct purrdf-lex edge to gmeow-mcp in Cargo.lock; the dependency pin and other packages are unchanged. Source-only producer identity, strict checks and actual test completion will be reported in validation.md.
