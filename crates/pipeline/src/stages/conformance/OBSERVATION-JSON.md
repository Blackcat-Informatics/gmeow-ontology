<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Native conformance observation channels

Signature queries, authored well-founded plans, frozen GMN vectors and gUFO source summaries use native JSON for their published channels and selected inner actions. Records are compact UTF-8 in declaration order without a trailing newline. Maps sort their keys; sets sort and deduplicate; vectors preserve order and repeated entries. Paths, digests, literal surfaces and quoted-triple surfaces are transported verbatim, without RDF interpretation.

Records admit objects, refuse missing required fields, null required fields and repeated known fields, and ignore unknown fields. Incidental positional record arrays are not admitted. Optional injection digests admit missing or null and emit explicit null when absent. Unsigned counts require unsigned integer tokens and checked host width: signed zero, negatives, fractions, exponents and overflow refuse. Zero signature rows are valid.

Results are exact singleton `Ok`/`Err` objects. Recorded failures retain their complete diagnostic roots and causal nodes. Positive-vector successful per-claim and idempotence witnesses require `Ok:null`; negative-vector successes carry a string or null. gUFO terms use exact singleton `Iri`, `Literal`, `BlankNode`, or `Triple` objects with string payloads; each subject/object pair has exactly two array elements. Typed maps validate every duplicate value before last-wins replacement.

Both selected cache codecs use one byte-action core, preserving operation/input/build identity, action coordination and the existing codec identity. A decoded hit that is malformed fails terminally; it does not rerun its initializer. Live initializer failure is retained without publishing an action. Both authenticated source codecs use one selector-bound loader and check the selector on every access. The enclosing grounding observation still uses its existing Serde embedding of gUFO summaries; six other source channels retain their separate Serde ownership.

Authored codec and cache controls do not load a selected corpus. Actual producer execution, authenticated corpus consumers, installed CLI behavior and full gates require separate qualification.
