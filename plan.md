# Private documentation search-index JSON

Migrate only SearchRecord and the actual search-index writer to the selected
native purrdf-lex JSON owner. Add the necessary direct dependency while retaining
Serde and serde_json for Card, NavItem and the remaining consumers. Preserve the
public search_index_json(&DocsModel) -> String API and the shared site-renderer
caller. Keep native builders at the output boundary, with no translation back
to Serde and no generic compatibility writer.

Preserve field declaration order, explicit definition:null, omission of only
empty facet vectors, populated advice/alignments/missing_coverage order, exact
(url,kind,id) record sorting, two-space pretty Unicode/string escaping and no
final newline. Keep source-document lookup, child URL and full-prose behavior.

Extract the existing sort/write boundary into a private adapter used by the
actual model path, permitting independent tiny record controls. Add an external
pure unit module with literal byte expectations and hand-constructed DocsModel
input; do not call SliceCatalog, fixture caches, corpus builders or producers.
Record a small wire contract. No edits to Card, NavItem, minijinja, lockfiles,
authentic fixtures, generated outputs or attestations.

Reuse current campaign governance and the previous current-source assessment;
Graphify was unavailable and narrow source navigation confirms the two production
entry points. No nested AGENTS files were found under docs/docs-model.
