# Issue #1971: Use native JSON for fragment registry observations

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github

## Body

Blocked by: none.
Blocks: #1776.

Move the complete native fragment registry observation channel to PurRDF's native JSON codecs. The current producer serializes the registry and recorded source refusals through Serde, and the authenticated corpus consumer decodes that same channel through Serde.

Acceptance:

1. Implement native named-record codecs on the existing NativeFragmentRegistry owner, preserving all eight inventory and characterization fields, declaration order, sorted sets/maps and exact strings. Remove its unused Serde ownership after checking actual consumers.
2. Wire the actual producer and authenticated reader through native JSON and the existing shared recorded-diagnostic outcome codec. Preserve both success and complete source-refusal evidence, with no generic parser, compatibility bridge or weaker fallback.
3. Preserve original-source observation and semantic refusal behavior, the channel identity and the consumer's authenticated read-only selection. Never construct a corpus from a test.
4. Qualify the boundary with independently authored small datasets, malformed named-record admission controls, affected production compilation, strict scoped diagnostics, independent review and normal hooks. Full produced-corpus, embedded consumer and installed acceptance remains owned by #1776 under the recorded campaign exception.

Principles 2, 3, 4, 7, 17 and 18 apply. This is a complete source channel migration, not completion of the parent dependency migration or corpus acceptance.


## Comments (0)

