<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Primer observation JSON

The primer and held-out-task observation types use
`purrdf_lex::json::record::{ToJson, FromJson}`. The producer emits compact
UTF-8 JSON without a terminal newline. Its record members follow declaration
order; these typed records are not recursively sorted:

- Observation: `primer`, `tasks`.
- Primer: `rows`, `intro`, `elided_operators`, `total_operators`.
- Row: `sort_curie`, `body`, `source`.
- Task: `label`, `document`, `sigils`, `operator_glyphs`, `ast_error`.

Row and task arrays retain their stored order. The two task string sets serialize
in lexical order and deduplicate decoded array elements through their existing
`BTreeSet<String>` owners. An absent task error always serializes as
`"ast_error":null`; missing or null errors decode to `None`, while an empty
string remains a present error.

A row source has exactly one external Pascal-case variant member:
`{"Sigil":{"curie":"g:Claim"}}`,
`{"Operator":{"curie":"g:Sub","glyph":"⊑"}}`, or
`{"Repair":{"curie":"g:Err"}}`. The operator payload emits `curie` before
`glyph`. Unknown or multiple variant tags are refused.

Native object records replace the former Serde traits. Required fields must be
present with their declared types; known duplicate fields are refused and unknown
record fields are ignored. Counts require nonnegative integer tokens within the
target platform's `usize` range, without fractional/exponent or signed-zero
coercion. Records and variant payloads require objects: incidental positional
arrays formerly admitted by Serde are intentionally not an input format.
No extra ordering, count-consistency or nonempty-field constraints are imposed.

Consumers authenticate the selected artifact before native decoding. Decoding
never reconstructs a primer, dictionary or corpus, and does not change the
artifact's selector binding. The primer's rendering and provenance accessors
continue to use the stored rows.
