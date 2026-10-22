<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Terminal observation JSON

Contextual result metadata, flagship wiring findings and the worked claim audit
use native JSON records. The terminal producer still computes each selected
operation over its actual admitted inputs. Consumers still authenticate the
selected artifact before decoding; decoding never runs a query, validation or
corpus producer. The claim audit restores its original diagnostic nodes through
the existing ledger replay and retains the schema-1 check.

Compact UTF-8 has no trailing newline. Record field order is:

* Contextual node: `properties`; contextual envelope: `nodes`, `readiness_graphs`.
* Flagship envelope: `profile`, `shapes_digest`, `input_digest`, `conforms`,
  `findings`; finding: `focus`, `path`, `component`, `source_shape`, `severity`,
  `failure_class`, `message`.
* Claim audit envelope: `schema`, `query_rows`, `nodes`, `claims`.
* Flat claim: `claim`, `text`, `model`, `method`, `confidence`, `evidence`,
  `flags`, `contradicts`.
* Claim evidence: `span`, `chunk`, `source`, `start`, `end`, `polarity`.
* Claim flags: `ungrounded`, `contradicted`, `stale`.

Vectors keep their order. Maps and sets use their existing sorted order. Optional
readiness worlds keep `null` for the default graph, distinct from strings naming
IRI or blank-node worlds. RDF object spelling stays an opaque string, including
datatype, language, direction and quotation syntax. Confidence is text, so a
string such as `"1e400"` is retained without numeric interpretation.

Optional finding/claim/evidence fields write explicit null for `None`; absence or
null decodes to `None`, and an empty string remains present. Empty collections
remain explicit; null required collections refuse. Known duplicate record fields
refuse. Unknown fields are ignored except on the claim-audit envelope, whose
existing closed-field policy refuses them. Map duplicates remain last-wins only
after every value is admitted. The `Default` implementations do not make required
record fields optional.

Schema uses unsigned integer tokens in the u32 range; `-0`, negative, fraction,
exponent and overflow tokens refuse. Evidence offsets retain signed i64 integer
bounds, including negative offsets, but refuse the `-0` token as the original
ordinary signed transport did. Missing/null offsets remain absent. Native errors
retain field and nested collection pointers. Malformed and trailing JSON refuse.
Records admit named objects; incidental positional record arrays are not part of
these producer-owned transports.

The public claim JSON renderer retains a separate format: recursively sorted
object keys, two-space pretty indentation and no final newline. This includes
sorting each nested claim, evidence and flag record; compact observation records
retain declaration order instead.

Query diagnostic identity retains the compact encoding of sorted variable names
to optional canonical RDF byte arrays. Unbound slots remain null. Canonical RDF
encoding and diagnostic fingerprint framing stay with their existing native
owners. Decoding or rendering does not replace those bytes with display strings,
normalize them as arbitrary JSON numbers, or recompute recorded fingerprints.
Other observation families, generic execution caches and public report metadata
remain separate owners.
