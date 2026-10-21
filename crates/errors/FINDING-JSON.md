<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Native finding transport

`Finding` and `RelatedLabel` use native `purrdf_lex::json::record` codecs.
Fields retain their declaration order and complete provenance, graph identity,
labels, advice, remediation and guidance. Empty default vectors, absent optional
values and a false `anchor_non_trivial` are omitted. Present empty strings remain
present. Optional null values decode as absent; null vectors and null booleans
refuse. Required fields remain required. Known duplicate fields refuse; unknown
fields are ignored. Structs admit named objects, not positional arrays. Unit enums
retain their string and exact single-key null-object forms. Nested unsigned
coordinates require integer lexemes within their declared width, excluding `-0`,
fractions and exponents. Absolute graph IRIs use native `BaseIri` admission with
no resolution or normalization.

The validation cache reads and writes the native `CachedResult` record through
its existing authenticated action store. Its required `findings` array remains
present even when empty. Input/context selection, product authentication,
immutable publication and codec identity remain unchanged. A corrupt or malformed
selected result fails; it cannot become an empty verdict. Input-key and enclosing
action-receipt encodings retain their separate owners.

The shared CLI NDJSON reporter emits native records with `event` first, followed
by each variant's declared fields. Events are compact UTF-8 JSON plus exactly one
LF; embedded line breaks are escaped. Stage duration retains the complete `u128`
millisecond value. Findings retain the same full record. The reporter locks
stdout for each complete event; a sink error is a contextual hard failure at the
infallible `Reporter` method boundary, rather than a dropped event.

`Finding` retains Serde for genuine enclosing Report, SARIF and other consumers.
Report metadata, normalized report digest preimages and those renderers are not
migrated by this operation. No native-to-Serde bridge is used by the native cache
or NDJSON paths.
