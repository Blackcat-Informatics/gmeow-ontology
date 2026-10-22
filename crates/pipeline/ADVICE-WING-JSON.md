<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Advice-wing observation JSON

The terminal advice observation uses native JSON records. The producer still
prepares its actual production shapes, validates the selected control, and emits
both advisory projections. The read-only consumer still authenticates the exact
artifact before decoding. This transport does not recompute an advisory, change
the claim's RDF confidence formatter, or replace its range authority.

`Observation::to_json_bytes` is fallible. It requires a finite binary64 confidence
for every advisory, including manually constructed records. A refusal identifies
the advisory index, code and `/advisories/{index}/confidence` in the primary
diagnostic and returns no bytes. It uses the native finite `Number` constructor;
it never substitutes JSON null for NaN or infinity. There is no infallible float
record exporter. The public confidence field remains `f64`.

The current selected producer carries the advisory owner's default confidence
`1.0`. Finite JSON output uses the native shortest round-trip binary64 spelling,
including `1.0`, signed `-0.0`, and the existing positional/exponent layout.
Decoding accepts native finite binary64 numbers and refuses overflow, null and
non-numbers. Arbitrary decimal lexemes are correctly rounded by the native owner;
bit-for-bit agreement with every legacy decimal parsing path is not promised.
Finite values outside the unit interval are not prohibited by this JSON codec;
the existing claim projection owns the advisory's semantic range contract.

Compact UTF-8 has no final newline. Record field order remains:

* Observation: `profile`, `shapes_digest`, `input_digest`, `shape_sources`,
  `retained_conforms`, `retained_findings`, `advisories`, `claims_nquads`.
* Retained finding: `focus`, `component`, `source_shape`, `severity`, `failure_class`.
* Advisory: `code`, `message`, `severity`, `suggestions`, `subject_iri`,
  `claim_code`, `claim_subject_iri`, `modality_iri`, `standpoint_iri`,
  `confidence`, `verdict_iri`, `advised_proposition`.

Vectors preserve order; maps and sets are sorted. Duplicate map keys remain
last-wins after every value is admitted. Optional fields write explicit null;
missing or null decodes to absence, while an empty string remains present.
Required fields and collections do not acquire defaults. Known duplicate record
fields refuse; unknown record fields remain ignored. The existing Severity owner
retains its string and exact singleton-null-object admission. Records require
named objects; incidental positional record arrays are outside this transport.
Malformed or trailing JSON refuses. Source digests, provenance and N-Quads remain
opaque strings, with their existing source owners unchanged.

The independent terminal `mcp-action-policy-statements.json` artifact uses the
native string-set codec directly at its publisher and authenticated reader.
Sorted compact strings, duplicate collapse, empty-set admission and the consumer's
separate nonempty assertion remain unchanged. The prepared policy record, cache,
source identity checks and hydration are separate owners.
