<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Source observation JSON

Math lowering, authored source lint and epistemics use native JSON at their selected inner caches, final publishers and authenticated readers. The existing cache and selector adapters retain their original action/profile identities, coordination, authentication and terminal decode failures. No decoder reruns lowering, linting, RDF parsing or reasoning.

Records emit compact UTF-8 without a newline in declaration order. Maps sort keys, sets sort and deduplicate, and vectors preserve order and repeated entries. Known duplicate fields refuse; unknown fields are ignored. Required fields must be present and correctly typed. Records and enum struct payloads admit objects; positional record arrays are not part of these transports. Typed maps admit every duplicate value before last-wins replacement.

The math owner transports all 23 externally tagged `MathLoweringError` struct variants. Their i128 indexes preserve signed integer token semantics, including `-0` as zero and both i128 extrema. Counts, depths and positions instead require strict unsigned tokens and checked usize bounds, refusing signed zero, negatives, fractions, exponents and overflow. Source `repeated_shadow` accepts missing or null and emits explicit null; `source_lint` accepts missing or null and is omitted on output when absent. Finite value-space fields remain required even for a default empty value. Typed errors, structural keys, lexical count strings and digests are carried unchanged. Existing grounding composites retain their genuine Serde embeddings.

The concrete source-lint DTOs live in `gmeow_validate::lint::source_observation`, shared by publication and the authenticated validate consumer. This owner has no selector or repository-loader dependency. The configuration's ordered compact bytes remain the input to its existing action digest; the consumer independently constructs its expected configuration and retains profile, complete-inventory and source-digest assertions.

Each epistemics `RlTriple` requires verbatim subject, predicate, rendered object and world strings plus a Boolean EDB flag. Decoding does not reinterpret the rendered object. A probe's total counts its full closure, while its vector contains selected subjects, so those sizes may differ. Probe failures retain complete RecordedDiag roots and causal graphs. Authored transport controls use plain records and in-memory diagnostics only; actual corpus production, authenticated corpus consumer execution and installed behavior require separate qualification.
