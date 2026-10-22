<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Native projection-profile inventory

The mappings producer records `pipeline/projection-profile-inventory.json`
through the native JSON owner after checking its observed source inventory.
The existing consumer first authenticates the selected mappings artifact, then
decodes the same native records. Its artifact name, fixture selection and
receipt/product commitments are unchanged; there is no source-read fallback.

The compact UTF-8 envelope writes `declared` then `measured`, without a trailing
LF. Each declared profile writes `iri`, `source`, then `cell_minimum`. Declaration
vectors retain order and repetitions. Measured counts emit in lexical key order.
String values are unchanged, with native JSON escaping; the codec does not
normalize source paths or identifiers.

All record fields are required. Missing/null/wrong-type fields and duplicate
known record members refuse; unknown record members remain ignored. Records admit
named objects, not positional arrays. Empty collections, empty strings and zero
counts remain present. Source loading and `check()` retain their existing policy;
the codec does not invent additional inventory constraints.

The measured object is a map, with distinct semantics from record members:
repeated keys retain their last value. Every occurrence must nevertheless contain
a valid count, so an invalid earlier value cannot be hidden by a later valid one.
The native BTreeMap decoder owns this behavior; its values use the same strict
count admission as `cell_minimum`. Counts require unsigned integer tokens within
the host's usize width. Negatives, `-0`, fractions, exponents and overflow refuse,
with native JSON pointers identifying nested rows or escaped map keys.

The production check still refuses missing declared profiles, undeclared measured
profiles and counts below their declared floor. Counts above the floor remain
valid. No canonical source parsing or fixture production is performed by the
authored JSON/plain-model controls. Other pipeline JSON owners remain separate.
