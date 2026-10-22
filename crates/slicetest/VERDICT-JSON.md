<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Slice-spec verdict JSON

Standalone aggregate and task verdict products use native compact JSON. The
three record types retain declaration order, task-vector order and complete
UTF-8 string contents. Every field is required; known duplicates, nulls and
wrong kinds are refused. Unknown fields are ignored. Records must be objects,
not positional arrays. Unsigned tokens exclude negative zero, negative values,
fractions and exponents, with checked `u32` and host-`usize` bounds.

The actual producer publications and read-only product replay share these
codecs. Replay retains schema, input census, payload equality, task uniqueness,
kind/path identity and product-byte digest checks. The untrusted aggregate's
three spec counts are added with checked arithmetic before non-vacuity and
task-count comparisons; overflow is an actionable refusal.

Generic `ActionReceipt` serialization, authentication, context keys and receipt
digests remain owned by the action cache. The three payload types retain their
Serde traits for that enclosing contract. Native product serialization does not
bridge through that owner or alter its canonicalization.

The `json_spec_` controls use authored JSON and inert `VerifiedEntry` values
through the actual replay validators. They neither authenticate a real receipt
nor discover inputs, execute workers, run specifications or produce a corpus.
