<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Documentation search-index JSON

`render::search_index_json(&DocsModel) -> String` and the site renderer share the
same native JSON writer. It emits a two-space pretty array with no trailing
newline. Records sort by `(url, kind, id)`; values within each facet retain the
order supplied by the model-to-record builder.

Each object writes `kind`, `id`, `label`, `definition`, and `url` in that order.
`definition` is always present: absent text is explicit JSON null, and present
empty text is an empty string. Nonempty facets follow in the fixed order
`advice`, `alignments`, `missing_coverage`. Only empty vectors are omitted; a
vector containing an empty string is present. Unicode and string escaping use
the native `purrdf_lex::json` owner, without a Serde conversion or recursive
object-name sort.

The index carries terms, slices, concerns, mapping sets, and slice child
documents. Top-level `docs.md` does not create another child record. Child
document URLs come from the shared source-to-page map, and their definition is
the complete source prose rather than a truncated summary.

This wire owner is private to search-index output. Card JSON and template inputs
have separate contracts. Tests use independent records and plain authored model
values; real-corpus search goldens and produced site parity require their existing
authenticated inputs.
