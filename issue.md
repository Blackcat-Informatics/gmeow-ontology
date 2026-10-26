# Issue #1956: Retire Serde JSON from grounding, medium and prepared policy channels

State: CLOSED   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, cross-cutting, dependencies, rust, complexity-hard

## Body

Retire Serde JSON from the complete grounding-catalog, medium-axis and prepared MCP action-policy transport paths. This is an independently shippable part of #1776; that parent retains workspace dependency removal, all other JSON owners, committed-artifact qualification and the full gate.

The current producer, caches and authenticated readers duplicate these observation records. Prepared action policy also crosses a separately featured MCP ingress. Move each path to its canonical typed native JSON owner, with one DTO shared by producer and reader where appropriate.

Acceptance:

- Grounding and medium caches, final publishers and authenticated readers use `purrdf_lex::json` directly. Remove their duplicate Serde DTOs.
- Prepared policy uses native JSON for both cache/publication and MCP ingress, while preserving the ingress byte bound, decode-versus-validation diagnostic context and lazy dataset hydration. Do not add a direct JSON dependency to the reasoning-only MCP feature.
- Preserve field and collection order, optional/null distinctions, source identity and authentication, exact tuple arity, complete recorded diagnostic causes, strict unsigned counts/schema versions and policy unknown-field refusal.
- Numeric confidence reuses the existing exact RDF literal owner and checked numeric constructor. Preserve the separate boolean direction protocol used by policy literals.
- Existing confidence and policy controls retain their assertions. Authored wire-boundary controls pass without corpus production; affected production callers compile with warnings denied; normal hooks pass; independent source review has no blocking findings.

This unit does not claim authenticated corpus, installed/browser/WASM or workspace-wide acceptance. Those original parent requirements remain open. Principles 4, 6, 7 and 18.

Blocked by: none. Blocks: #1776.

Post-merge acceptance repair: the reasoning-only native MCP test profile was verified to fail with 62 references to core-only symbols and optional dependencies. Preserve all combined-consumer cases and their batching under the complete feature profile; keep pure storage checks available under their actual required features; move the authenticated candidate-library case into the native corpus batch without changing its assertions or fixture authority. Both shipped single-segment profiles must compile with warnings denied, and producer-free surface checks must exercise complete descriptor/handler assembly and segment routing. This adds no production dependency and does not claim authenticated RPC replay or browser execution.



## Comments (1)

### paudley — 2026-10-26T09:04:35Z

The post-merge reasoning-only MCP test build fails with 62 references to core-only symbols and optional dependencies. Reopening to repair the touched feature boundary. Preserve every combined-consumer case and its batching, separate the authenticated browser-library case from pure storage checks, and verify the shipped single-segment configurations with producer-free surface checks. Production feature boundaries and corpus authentication remain mandatory.

