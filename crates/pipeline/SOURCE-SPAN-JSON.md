<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Native source-span transport

`SpanIndex::to_json_bytes` writes the source-load span blob using the native JSON
record owner. `StageProduct::span_index` reads that same blob through native
`FromJson`. The representation name, input selection, product/cache commitments
and scheduler drop-after-last-consumer policy are unchanged.

The compact UTF-8 object has `paths` followed by `entries`, with no trailing LF.
Each entry writes `subject`, `path_id`, `line`, `column`, then `byte_offset`.
Subjects sort lexically. A path enters the table when first encountered in that
subject order; paths are not independently sorted. Equal path strings share a
writer table slot. Path IDs must fit u32; encoding refuses an oversized reference
with context instead of truncating it. Line and byte offset retain all u64 values;
column retains all u32 values. Existing zero coordinates remain admitted.

Both records require all their fields. Missing/null/wrong-type fields and known
duplicate members refuse; unknown members remain ignored. Integer fields require
native unsigned integer tokens within their widths: negative values, `-0`,
fractions, exponents and overflow refuse. Records admit named objects; incidental
positional struct arrays are not an alternative transport.

Every decoded path ID must reference a table slot. Failures identify the entry's
`path_id` with a JSON pointer. Each table slot owns one shared `Arc<str>`: entries
referencing that slot share it, while separately listed equal path strings retain
separate slot allocations. Duplicate subjects in a decoded table retain their
last entry. This differs deliberately from the public live-index `insert` and
`merge` operations, which retain the first assertion. Re-encoding uses the live
index's sorted subjects and coalesces equal path strings, as the original writer
did; it does not preserve redundant input table rows.

An empty valid table is `{"paths":[],"entries":[]}`. An absent required blob or a
malformed selected table fails; neither becomes an empty index. Diagnostic
enrichment keeps the subject join key and physical source coordinates. This
transport does not migrate enclosing action receipts, reports or other JSON
owners in the pipeline crate.
