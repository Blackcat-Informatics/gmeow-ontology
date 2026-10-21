<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Primitive bundled JSON readers

`Bundle::modeled_def_keys` and `Bundle::denied_cells` use the native
`purrdf_lex::json` owner after their existing representation/blob/archive reads.
Snapshot selection, blob decoding and archive membership remain unchanged.
The optional wheel-only contracts remain: an absent schema yields an empty
model-key set; an absent denied representation yields `None`. Present empty
denied rows yield `Some(empty Vec)` instead. Invalid JSON produces `BundleJson`;
blob and archive errors still propagate through their existing owners.

Model-key extraction reads a free-form document with native `read_document`:
object names are sorted recursively and the last duplicate member wins.
Only keys of an object-valued top-level `$defs` are returned, as a sorted set.
The former extraction policy is retained for null/scalar/array roots, missing
`$defs`, and non-object `$defs`: each yields an empty set. This operation does
not validate JSON Schema or inspect definition values. Duplicate `$defs` uses
only its last value; repeated definition names contribute one decoded key.

Native JSON accepts RFC numeric lexemes such as `1e400`, whereas the former
Serde decoder rejected binary64 overflow even in unused fields. That native
admission difference is deliberate here: ignored schema values stay ignored,
without conversion to floats, identity normalization or schema validation.
Keys remain decoded strings, so `-0`, `0`, `1e400` and large integer-looking
keys retain distinct textual identities. No schema bytes are rewritten.

Denied cells use native typed `Vec<Vec<String>>` admission followed by the
existing exact three-member conversion. Root and row values must be arrays;
each member must be a string. Nulls, numbers (including `1e400`), booleans and
containers are not coerced. Rows preserve order, duplicates, empty strings,
Unicode and decoded escaping. Short/long rows retain the explicit
`denied-cells row is not a 3-tuple` failure. Malformed/trailing JSON, invalid
UTF-8 and unpaired surrogate escapes fail rather than returning partial rows.

The diagnostic-node reader still depends on its canonical Serde `DiagNode`
owner. Other bundle JSON surfaces and dependencies retain their own contracts.
Pure decoder tests consume independently authored JSON bytes only; authentic
bundle/schema/consumer acceptance requires its existing selected inputs.
