# Issue #1807: [term] Literal typing: language on words, types for machinery, time at its stated precision (consumers: memetics, katamari)

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, cross-cutting

## Body

## Summary

Three typing gaps make literals ill-typed or over-precise once a consumer
stores real data:

1. **Words cannot carry a language.** 132 properties range over
   `xsd:string` and none admits a language-tagged literal.
2. **Machinery has no named type.** `gmeow:contentDigest`,
   `gmeow:mediaType` and `gmeow:identifier` range over `rdfs:Literal`, so
   a digest, a media type and an identifier are all just strings.
3. **Coarse-precision time cannot be stated.** Time properties range over
   `xsd:dateTime`, so a day- or year-precision time must be padded into
   an instant nobody observed.

Checked against `main` at `a631f73` (2026-10-10).

## Consumers

- **memetics**: the memory adapter for a companion agent. Its stores hold
  her dreams, conversations, intentions and observations as RDF 1.2. It
  tags every piece of her text with its language at ingest. It types
  every piece of machinery: digests, identifiers, statuses and times.
  Its vector families embed only language-tagged text, so an untagged
  digest or status string can never be embedded as if it were prose.
- **katamari**: the governed RDF 1.2 store memetics provisions. It admits
  only declared natural languages to a vector family, keyed on the
  literal's language. It stores RDF 1.2 directional strings
  (`rdf:dirLangString`). It addresses content by plain BLAKE3-256 of the
  exact bytes. It stores time with an explicit precision: day, month,
  year or a named period.

## 1. Words should admit a language tag

**Today.** 132 properties declare `rdfs:range xsd:string` (for example
`gmeow:positionLabel` in `extensions/narrative`, and
`gmeow:conditionText` in `core/norms`). No property ranges over
`rdf:langString`. Since RDF 1.1, `"..."@en` has datatype
`rdf:langString`, which is not `xsd:string` and not a subtype of it.
So any language-tagged value of these properties is ill-typed under
D-entailment and in OWL 2 DL.

**Why it matters.**
- GMEOW's own policy (#164) tags GMEOW-authored text `@x-gmeow-english`.
  It keeps public BCP 47 tags for consumer and external data.
- A consumer that follows that policy and tags its users' text `@en`
  then contradicts the ranges it is using.
- Both consumers here gate semantic indexing on the tag: text with a
  declared language is embedded, untagged strings are not. The ranges
  currently forbid the very tag the gate needs.

**Proposal.**
- A property whose values are words (prose, labels, names, reasons,
  summaries) ranges over `rdfs:Literal`, with a SHACL constraint
  admitting `rdf:langString` and `rdf:dirLangString`.
  - `rdfs:range rdf:langString` alone would exclude RDF 1.2 directional
    strings, which katamari already stores.
- A property whose values are identifiers or codes (keys, ids, version
  strings) ranges over `xsd:token`.
  - `xsd:token` is derived from `xsd:string`, so every existing value
    that is a valid token stays valid.
- The 132 properties need sorting into words and identifiers. That is a
  judgment per property. A first pass over the ones the consumers use:
  - `positionLabel` and `conditionText` are words;
  - `version` is an identifier.

**Open questions for GMEOW.**
- Should the SHACL constraint be one shared shape (a "natural-language
  literal" node shape) that properties reference, or stated per property?
- How does this interact with the `@x-gmeow-*` canonical tags of #164?
  The constraint should admit both private-use and public tags. Is a
  tag check (well-formed BCP 47) wanted as well?

## 2. A digest's algorithm is its datatype; a media type is an IRI

**Today.** `gmeow:contentDigest` (`core/sources`), `gmeow:mediaType` and
`gmeow:identifier` (`core/documents`) range over `rdfs:Literal`.
`gmeow:identifier` is also domain-bound to `gmeow:CreativeWork`.
Consumers write digests as prefixed strings (`"blake3:..."`) or bare
hex, and media types as strings.

**Proposal (agreed by both consumers).**
- **One datatype per digest algorithm**, for example a BLAKE3-256
  datatype whose lexical form is 64 lowercase hex digits, as `b3sum`
  prints it.
  - A digest stays one triple, and digests compare by RDF term equality.
  - Several digests of one thing are several triples.
  - Hex rather than base32: shell-only conformance gates compare with
    the stock tool's output.
- **A digest is a hash of exact bytes, never a scheme-prefixed
  identifier.**
  - katamari's content id is plain BLAKE3-256 of the exact bytes, which
    stock BLAKE3 verifies. That is a generic digest and belongs here.
  - Its record ids are BLAKE3 over a domain tag (`"<scheme>:<type>:v2"`,
    a zero byte, then canonical bytes). Those are an identifier scheme,
    not a digest.
  - If GMEOW wants to name such ids, they need an identifier datatype
    that carries the scheme. A generic digest datatype must never be
    used for them, or two unrelated hashes would compare as if
    comparable.
- **A media type is the IANA media-type IRI**
  (`https://www.iana.org/assignments/media-types/image/png`).
  - Parameters such as `charset` go in their own properties, never in
    the IRI.
  - That makes `gmeow:mediaType` an object property, or adds a sibling
    object property beside it.

**Open questions for GMEOW.**
- Where do digest datatypes live: a small `core/digests` vocabulary, or
  beside `contentDigest`?
- Which algorithms are needed at first? Both consumers need BLAKE3-256.
  SHA-256 is the obvious second.
- Changing `gmeow:mediaType` from a datatype property to an object
  property is a breaking change. Is a new object property plus
  deprecating the old one preferred?

## 3. Time at the precision it was stated

**Today.** Time properties range over `xsd:dateTime`. There is no way to
state "on 2026-09-30", "in September 2026" or "in 2025" without
asserting an instant.

**Why it matters.**
- An observation known only to the day, forced into `xsd:dateTime`,
  claims a precision nobody stated.
- A later query that compares instants then treats a guess as a fact.
- Both consumers answer recall questions like "what did I do last fall".
  That needs the stored precision to be honest.
- katamari already keeps an explicit `TimePrecision` (day, month, year,
  named period) beside each time.

**Proposal.** Either of these, and GMEOW should choose:
- (a) time properties range over a union admitting `xsd:dateTime`,
  `xsd:date`, `xsd:gYearMonth` and `xsd:gYear`, stated as a shared SHACL
  shape the way item 1's text constraint is;
- (b) keep `xsd:dateTime` and add a precision annotation, a value
  vocabulary (instant, day, month, year, named period) on the claim.

  (a) is simpler for plain data. (b) also covers named periods ("last
  fall", "the summer of the move"), which no XSD type expresses.

**Open question for GMEOW.** Does a named period belong here, or with
the temporal slices as an interval entity the time points to?

## Local stand-ins

- memetics:
  - its local extension slices range words over `rdf:langString` and
    identifiers over `xsd:token`;
  - it emits digests as `xsd:hexBinary` (algorithm implied by the
    property), media types as `xsd:token`, and only exact instants as
    `xsd:dateTime`;
  - each moves to whatever GMEOW settles.
- katamari: stores time precision in its own record (`Timestamp`), and
  directional strings natively.


## Comments (1)

### paudley — 2026-10-10T20:21:12Z

A related case for item 2 (machinery has no named type): **`gmeow:toolArguments` (core/agentic) is documented as "the verbatim arguments payload presented to the tool (typically JSON)".**

A consumer enforcing a no-JSON admission law cannot write that: a JSON object in a literal is opaque to every graph tool. memetics instead keeps a call's arguments as labelled facets of the call, one per argument leaf, labelled by its path (see the labelled-facets issue, #1808). Could `toolArguments` either:
- document arguments as structure (facets, or properties of the call) and keep a literal only for an opaque payload, typed by its media type (#1807 item 2); or
- be complemented by a structured form, so a no-JSON consumer has a sanctioned home?

The same applies to `gmeow:toolResult`.


