<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Reviewed-coverage observation JSON

`ReviewedCoverageObservation` uses native JSON at its actual publisher and
authenticated consumer. `ReviewedCoverageOutcome` owns this channel's exact
single-member `{"Ok":observation}` or `{"Err":recorded-diagnostic}` envelope.
It retains original recorded causes and fingerprints; decoding never reparses a
catalog or converts an observation rejection into a live diagnostic or verdict.

The compact UTF-8 observation has `parsed_catalogs` then `reviewed_coverage`,
with no final newline. Both fields are required objects. Map keys are sorted;
reviewed-key vectors retain order and multiplicity. Empty maps and vectors remain
explicit. Counts require unsigned integer tokens within the host `usize` range:
negative zero, negatives, fractions, exponents, overflow and non-numbers refuse.
The native typed-map decoder admits every duplicate value before its last value
wins; a later valid entry cannot hide an invalid earlier count or vector.

Known duplicate record fields refuse; unknown record fields remain ignored.
Required null fields refuse. Named records require objects; incidental positional
record arrays are outside this producer-owned transport. Outcome objects require
exactly one known variant, with the proper typed payload. Empty, repeated, mixed
and unknown variants refuse. Native errors retain `Ok`/`Err`, field, map-key and
vector-index pointers. Malformed and trailing JSON refuse.

The reader still requires the exact selected stage-conformance artifact. Its
catalog assertions and equality comparison to the frozen golden remain unchanged.
The golden producer already writes a sorted compact string-vector map plus one
newline; that producer and its bytes are unchanged. The golden reader now uses
the same native JSON owner directly.

The independent documentation-coverage-dimension channel uses the native
string-set codec directly at its publisher and authenticated reader. Its original
RDF selection, dimension non-vacuity and coverage-key joins remain unchanged.
Broader documentation models, caches, selectors and report metadata have separate
owners.
