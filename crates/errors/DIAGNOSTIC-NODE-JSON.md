<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Lowered diagnostic JSON

`DiagNode` and its complete nested value graph use the native
`purrdf_lex::json::record::{ToJson, FromJson}` owner. Live `Diag` values are
never serialized or reconstructed from a node. Fingerprint computation,
ledger insertion and replay remain their original semantic owners; decoding
retains the supplied fingerprint, and replay refuses a mismatched digest.

The stage-cache contribution is exactly `[stage, nodes]`, including an empty
node vector. Its writer emits compact JSON in declaration order with no final
newline. The cache reader requires exactly two items and the actual product
stage must match. A declared producer's missing contribution is corrupt input.
The public bundle reader has a distinct node-vector contract. A legitimately
absent wheel ledger remains empty; a present malformed vector fails. Neither
operation guesses or substitutes the other envelope.

Records preserve their emitted field order, required empty collections,
explicit nulls and omission of optional payloads. Defaulted vectors may be
absent but cannot be null. Known duplicate members fail; unknown members remain
ignored. Native record admission intentionally requires named objects rather
than positional struct arrays. Unit enums retain their exact string spellings
and single-key null-object forms; empty, multiple-member and non-null enum
objects fail. Newtype stage and focus values remain strings.

Every unsigned coordinate and fingerprint byte uses the native strict unsigned
accessor with checked width. Negative zero, negative values, fractional and
exponent tokens are refused. Fingerprints require exactly sixteen bytes. Graph
strings pass through the native absolute-IRI constructor without resolution or
normalization. The native RFC JSON reader may retain arbitrary number tokens
inside ignored unknown fields; such fields never enter diagnostic identity.

The selected node cache and vector operations have no Serde fallback or
native-to-Serde bridge. Shared types retain Serde derivations for separately
owned enclosing records, including `RecordedDiag`, loss ledgers, conformance
observations and `Finding`/`Report`. Those operations are separate from this
node transport. Report metadata and its canonical content-digest contract are
not changed by this codec.

Authored literal and in-memory controls cover the codec and actual cache/vector
adapters. They do not establish authenticated corpus, shipped bundle or installed
consumer acceptance, which requires separately produced and selected inputs.
