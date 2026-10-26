<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Grounding, medium and prepared-policy JSON

Grounding and medium observations have one shared DTO owner in `gmeow-validate`. Their native JSON implementations are native-target-only, matching that crate's dependency boundary. Producer caches, final publication and authenticated readers use those same types; decoding neither loads source nor reconstructs a corpus. The selected source inventories, operation keys, digests and authentication checks remain the authority at their existing callers.

Records write fields in declaration order, maps in key order, sets in element order and vectors in their existing order, including duplicates. Compact UTF-8 bytes have no trailing newline. Grounding and medium record fields refuse known duplicates and ignore unknown fields; required fields refuse absence and null. Optional fields accept absence or null and write explicit null. Native maps admit every member value before retaining the last duplicate key. Record transport is object-only; incidental positional record arrays are not admitted.

Grounding confidence uses `NumericLiteral<true>` and its existing checked constructor. Both native JSON and retained Serde use the same four-cell RDF literal owner: lexical string, datatype IRI, optional language and optional lowercase direction. Numeric decoding preserves the lexical coordinate and datatype, requires a finite numeric value, and enforces the selected unit interval without converting to binary floating point. Other `NumericLiteral` uses retain their own range contract. Genuine Serde embeddings of the numeric owner remain supported.

Grounding catalog cache payloads are exact two-cell arrays: ordered cells and an optional count-or-diagnostic outcome. Declaration payloads are exact three-cell arrays: subject set, class set and label map. Count values require unsigned integer tokens and checked `usize` admission, including inside every duplicate map occurrence. Recorded failures retain root and cause identities. A missing optional catalog outcome differs from an outcome containing count zero.

Medium values are externally tagged single-member `Iri`, `Literal` or `Other` string objects. They record selected presentation strings, not lossless RDF terms. The `classes` field is required; an absent class key stays absent, while a key with null remains a declared missing observation. An absent predicate key differs from a present empty vector. Decoding does not synthesize class or predicate inventory.

Prepared action policy is stricter: its records refuse unknown fields as well as known duplicates. The three types are owned by `gmeow-logic-compile`, including a decode-only byte entrypoint used by MCP. The entrypoint does not validate source identity or hydrate RDF. MCP first applies its byte-size bound, reports decoding failures in their existing context, and then calls the unchanged semantic `validate` method. Dataset hydration remains lazy and its lock is not serialized.

Policy object literals use an object containing lexical text, optional datatype, optional language and optional **boolean** direction (`true` for RTL, `false` for LTR). This is intentionally distinct from the four-cell IR literal protocol. Schema versions require strict unsigned `u32` tokens. Row order, original source statements, omitted annotations and NQuads bytes are preserved; the transport does not parse or normalize the retained strings. Unrelated policy RPC envelopes and other Serde owners remain separate.

Pure controls use independently authored JSON or the existing tiny authored policy dataset. They do not load authenticated production artifacts. Actual selected-corpus, installed MCP, bundle authentication and end-to-end producer acceptance require their existing external qualification paths.
