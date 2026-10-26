Retire Serde JSON from the complete grounding-catalog, medium-axis and prepared MCP action-policy transport paths. This is an independently shippable part of #1776; that parent retains workspace dependency removal, all other JSON owners, committed-artifact qualification and the full gate.

The current producer, caches and authenticated readers duplicate these observation records. Prepared action policy also crosses a separately featured MCP ingress. Move each path to its canonical typed native JSON owner, with one DTO shared by producer and reader where appropriate.

Acceptance:

- Grounding and medium caches, final publishers and authenticated readers use `purrdf_lex::json` directly. Remove their duplicate Serde DTOs.
- Prepared policy uses native JSON for both cache/publication and MCP ingress, while preserving the ingress byte bound, decode-versus-validation diagnostic context and lazy dataset hydration. Do not add a direct JSON dependency to the reasoning-only MCP feature.
- Preserve field and collection order, optional/null distinctions, source identity and authentication, exact tuple arity, complete recorded diagnostic causes, strict unsigned counts/schema versions and policy unknown-field refusal.
- Numeric confidence reuses the existing exact RDF literal owner and checked numeric constructor. Preserve the separate boolean direction protocol used by policy literals.
- Existing confidence and policy controls retain their assertions. Authored wire-boundary controls pass without corpus production; affected production callers compile with warnings denied; normal hooks pass; independent source review has no blocking findings.

This unit does not claim authenticated corpus, installed/browser/WASM or workspace-wide acceptance. Those original parent requirements remain open. Principles 4, 6, 7 and 18.
