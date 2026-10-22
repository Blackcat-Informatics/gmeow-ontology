<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Glyph legend JSON

`glyph_legend_value` is the shared native JSON builder over an admitted glyph
registry. Each row has `glyph` then `tokenCost`. Rows retain the registry's
longest-match order: descending Unicode scalar count, then bytewise lexical
order. Duplicate executable tokens are represented once by the registry. Costs
come from the existing pinned table; no token cost is guessed, rounded or measured
while building a legend.

`glyph_legend_json` writes that same value as compact UTF-8 without a trailing
newline, preserving its public String API and existing pinned-inventory bytes.
The native JSON owner supplies escaping and integer spelling. Both exits remain
available without the `glyph-cost` feature and do not invoke a tokenizer or model.
An unpriced registry glyph returns the original typed `GmnUnpinnedGlyphCost`;
there is no zero price, omitted row or partial returned legend.

The MCP tool uses the same builder directly over its loaded dictionary's registry.
Its compact payload has `legend` then `ok`, preserving the existing sorted-envelope
byte order. It does not parse the String exit or maintain a second row generator.
An empty registry produces `[]` (or `{"legend":[],"ok":true}` at the MCP payload
boundary); the authenticated corpus contract independently requires the actual
shipped alphabet to be nonempty.

Dictionary admission, tool arguments, outer RPC serialization, source-artifact
authentication, browser forwarding and the pinned cost/BPE verification owner are
unchanged. Pure controls use only a small independently authored RDF registry;
agreement between selected source, bundle, wasm and browser outputs remains an
authenticated-artifact integration boundary.
