<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Card JSON and TOON wire contracts

`Card`, `CardEntailment`, `CardFixture`, `CardDiagnostic` and `CardLoss` implement
the native `purrdf_lex::json::record::ToJson` trait. Their Rust JSON capability is
native rather than Serde. They are output records, so this surface adds no
decoder. `card_json_schema()` returns a native JSON `Value`. The public String
and byte-returning documentation APIs keep their existing signatures.

Direct Card JSON preserves declaration order, including nested panel records.
Required `category` and `iri` always appear, even when empty. Optional strings
are omitted only when absent; `Some("")` remains an empty string. Vectors are
omitted only when empty, so a nonempty vector of empty strings or empty panel
records remains present. Each panel record writes every declared field,
including its own empty strings and empty premise vector. String Unicode and
escaping belong to the native writer.

Four output policies coexist:

| Surface | Ordering and layout |
| --- | --- |
| `card_json` / direct compact Card bytes | Declaration order; pretty two-space or compact respectively; no final newline. |
| `card_toon` | Recursively sorted object names before the existing TOON encoder; vector order retained; existing terminal newline and string quoting preserved. |
| MCP `doc_card` | Sorted envelope and nested Card names. `bytes` counts the original rendered payload's UTF-8 bytes; `tokens` counts its Unicode scalar values in groups of four rounded up. JSON payload measurement uses compact declaration-order Card bytes before sorting. Markdown uses the existing renderer. |
| Published `card.schema.json` | Recursively sorted schema names, two-space pretty JSON and exactly one final newline; `required` arrays retain their authored order. |

The same tier projection applies before machine output: Summary carries identity
and definition; Standard adds advisories; Full adds rich panels. This migration
does not change Markdown rendering, public field values, term resolution,
codebook or bundle authentication, or the meaning of the schema.

The schema remains the existing co-located authored contract, expressed through
native objects. It requires `category`/`iri`, rejects additional properties and
describes all rich-panel fields. The native schema validator consumes the schema
and Card bytes directly; no native-to-Serde translation is used.

The plain records have no numeric result fields. TOON's existing float-lexeme
test only decides whether a string such as `NaN` or `1e400` needs quotes; it does
not turn that string into a number or impose numeric constraints on the Card.
Other documentation model records, CachedSite receipts, minijinja NavItem inputs,
and unrelated MCP/pipeline JSON retain their separate ownership contracts.
