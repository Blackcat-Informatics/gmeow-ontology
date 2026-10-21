<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMN validation verdict JSON

`GmnValidationVerdict` uses the native `purrdf_lex::json::record` traits. The
browser wrapper and pipeline's browser-verdict observation map use this same
record owner. Other language records and binary codebook encodings have their
own contracts.

The wire representation is a JSON object. Compact output writes members in
declaration order: `conformant`, then `detail` when present, then `failureClass`
when present. Absent optional strings are omitted, including for independently
constructed verdicts. Empty strings remain present. The writer preserves Unicode
and escapes JSON string content through the native owner. It adds no newline.

Examples:

```json
{"conformant":true}
```

```json
{"conformant":false,"detail":"uncovered term","failureClass":"https://blackcatinformatics.ca/lang/GmnUncoveredTerm"}
```

Decoding requires an object with a boolean `conformant`. Optional `detail` and
`failureClass` must be strings or null; missing and null both decode to `None`.
Known duplicate members are errors even if their values match or are null.
Unknown fields are ignored, including the Rust field spelling `failure_class`.
The public record does not add a new consistency rule between the boolean and
the presence of strings: the production codec owns their meaning. Malformed or
trailing JSON is refused by the native reader.

The admitted record form is an object. Incidental positional-array forms of the
previous derived decoder are not supported. Rust callers use native `ToJson` and
`FromJson` rather than Serde traits for this record. Both production callers use
the documented object representation.

The producer's outer observation map sorts source path keys. Each nested verdict
retains its declaration order; recursively sorting all JSON members would change
the wire contract. Consumers read authenticated observation bytes after source
selection. They do not rewrite those bytes or produce a missing codebook.
