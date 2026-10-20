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


## Comments (3)

### paudley — 2026-10-10T20:21:12Z

A related case for item 2 (machinery has no named type): **`gmeow:toolArguments` (core/agentic) is documented as "the verbatim arguments payload presented to the tool (typically JSON)".**

A consumer enforcing a no-JSON admission law cannot write that: a JSON object in a literal is opaque to every graph tool. memetics instead keeps a call's arguments as labelled facets of the call, one per argument leaf, labelled by its path (see the labelled-facets issue, #1808). Could `toolArguments` either:
- document arguments as structure (facets, or properties of the call) and keep a literal only for an opaque payload, typed by its media type (#1807 item 2); or
- be complemented by a structured form, so a no-JSON consumer has a sanctioned home?

The same applies to `gmeow:toolResult`.


### paudley — 2026-10-20T13:59:15Z

# Literal typing across words, machinery and time

Preserve the complete issue and its tool-payload comment. Consumers are memetics and katamari. Principles 4, 6, 7, 11, 17, 18 and 19 govern canonical logic authoring, exact bytes, typed machine values and explicit time precision. This plan does not close the issue from a prerequisite or a narrow synthetic test.

## Completeness contract

| Requirement | Delivery |
| --- | --- |
| Language and direction on words | Classify every current string-ranged property by actual semantics, separate words from identifiers/source-code payloads, admit ordinary and directional language literals with logic-authored validation; preserve private/public tags |
| Machine tokens | Move identifier/code/version fields to their appropriate typed ranges, without treating arbitrary source code or whitespace-bearing expressions as tokens |
| Digest algorithms | Algorithm-specific byte-digest datatypes with exact 64-lowercase-hex lexical form for BLAKE3-256 and SHA-256 as needed, nonfunctional coexistence and RDF term equality; identifier schemes remain distinct |
| Media types | Use IANA resource identity and separate parameters, replace obsolete string handling in actual producers/consumers rather than retain competing paths; inspect existing media additions before minting |
| Precision | Admit instant/day/month/year without invented padding, and use existing named periods/interval entities for named periods; preserve frame and datatype in actual consumers |
| Tool comment | Sanction structured tool arguments/results with labelled facets or existing RDF properties, preserve opaque payload media typing, and integrate the overlapping generic-record contract |
| Qualification | Focused actual compiler/projector/native-validator controls, complete migration examples and source annotations/translations; normal hooks, honest hosted/corpus status and ghprsq integration |

## Task 1: Repair shared validation projection

The actual classifier recognizes XSD and rdfs:Datatype but treats RDF language datatypes as classes. Explicitly closed anonymous property ranges are skipped. Repair the existing classifier and ObjectsOf range derivation, reusing the existing constraint IR, projection and native list walker. Prove language/direction and coarse-date union behavior through generated SHACL and the native validator over tiny authored inputs. Keep open-world and explicit opt-out behavior; reject malformed selected list structure without prefix truncation. No hand-authored ontology shapes, alternate datatype validator or dependency change.

Publish this prerequisite separately with the complete issue still open. It is not the property migration or algorithm/precision delivery. Preserve remaining tasks visibly on the issue and active queue.

## Task 2: Complete the words and machine-field census and migration

Refresh the historical 132-property report from actual current source, classify each property from its definition and consumers, and change canonical ranges/logic constraints. Retain opaque source/code semantics rather than blindly tokenize every non-prose value. Include paired annotation updates, actual emitters and corpus fixtures wherever a changed contract requires them. Distinguish structural emitted metadata from human language and test real selected properties, including positionLabel, conditionText and version.

## Task 3: Integrate byte digests, media identity and structured payloads

Use current sources/documents owners and existing media additions. Trace every producer/consumer that writes or interprets contentDigest/mediaType/identifier/toolArguments/toolResult, replacing inferior paths coherently. Generic content digests hash exact bytes and never scheme-prefixed or domain-separated identifiers. Keep parameters separate from IANA media resources. Complete the comment using the existing generic-record overlap, with no opaque JSON requirement and no compatibility cruft.

## Task 4: Complete precision and full consumer acceptance

Implement the chosen exact datatype/precision model across selected temporal properties, existing named periods and interval links. Test day/month/year/instant distinctions without padding or sorting coarse values as invented instants. Run authorized focused checks, source hygiene and normal signed hooks; hosted producer/corpus acceptance remains distinct and is never manufactured locally. Close the original issue only after its complete implementation is delivered, not from this first repair.

Standing limits: no local optimized producer, corpus/fixture production, full check/sync, host probes, resource caps or hook bypass. CI is ignored for sequential merges by explicit user instruction. No independent dispatcher is available; self-assessments are labelled honestly. Preserve the GMN sibling and unrelated work.


### paudley — 2026-10-20T14:26:37Z

Fix language datatypes and closed range projection

Explicitly closed anonymous language and calendar ranges produced no validation shape. RDF language datatypes were also classified as classes in restriction-derived shapes. Repair the existing shared classifier and property-range derivation; canonical Datarange declarations use the shared structural typing view.

Closed range unions now produce ObjectsOf constraints through the existing IR and SHACL projector. Native strict list walking rejects malformed or conflicting selections with a typed diagnostic focused on the property. Empty and universal unions preserve their meanings. Open-world defaults and explicit opt-outs remain intact. Other anonymous closed range forms fail explicitly rather than producing partial constraints.

Five compiler controls cover classification, structural typing, closure selection, malformed inputs and union boundaries. The generated-SHACL/native-validator matrix covers public/private tags, both text directions, instant/day/month/year values and rejected machine/nonliteral values. This is the real production derivation/projector path over tiny authored inputs, with no corpus construction. Principles 4, 7, 11, 17 and 18 apply.

Validation: the baseline reproduced zero shapes where two were required. The final affected selection passed all 73 tests. Strict producer-independent workspace/all-target Clippy passed with warnings denied. Source hygiene, static recipe admission, whitespace checks and normal signed commit hooks passed. Earlier test-assertion API build errors were corrected without weakening assertions; their logs remain in the selected archive. No local optimized producer, corpus/fixture generation, synchronization or full make check ran.

Cache invalidation was traced through the existing selected implementation identity and stage action key; no redundant version mechanism was introduced. Full generated-product and corpus qualification remain unrun locally. Hosted producer/helpers/lint/Rust-prebuild checks are in progress and CodeRabbit is pending; CI is explicitly ignored for integration under “merge them one by one, ignoring CI.” No hosted success is claimed.

Review surface: one automated review-in-progress comment, no submitted source findings, inline comments or unresolved threads. No optional automation was activated. Completion judgment is self-review because no independent dispatcher is available. The fetched base has no intervening delta and merge-tree is clean; no conflict resolution or synchronization is needed.

This completes the shared projection prerequisite only. The original issue remains open for the complete property census/migration, exact-byte digest and media contracts, time-precision migration and structured tool payload requirement. These remain Tasks 2–4 of the authoritative plan, without a replacement issue or reduced parent scope.

Authoritative plan: .stage/term-literal-typing-language-on-words/plan.md. Selected archive: .stage/term-literal-typing-language-on-words.

Refs #1807.
Defect-Class: none


