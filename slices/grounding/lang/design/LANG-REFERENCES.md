<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Language — References and the External Survey

> The **references appendix** of the GMEOW Language design set: the classified survey of the
> external standards, ontologies, formalisms, classification schemes, and engines the language
> grounding layer subsumes, projects to, links, or references — each tagged with its relation,
> license posture, and kind. Where no external
> ontology exists for something the design needs, this appendix says so and marks the surface as
> GMEOW-authored — original authorship is recorded, never disguised as alignment.
>
> **Tags.** *subsume* = the contribution is carried natively and exceeded; *project* = a generated
> lossy emission target ([`LANG-PROJECTIONS.md`](LANG-PROJECTIONS.md)); *link* = an
> alignment/authority surface via `gmeow:TermEquivalence`; *reference* = cited theory or spec,
> no artifact. Licenses are as reviewed at design time; anything marked *review* requires a
> license pass before any data (not spec) ingestion.

## Primary anchors

The four external anchors the design leans on hardest, peer to `math:`'s mathlib/Wikidata/QUDT
set:

| Anchor | Role | Relation | License |
|---|---|---|---|
| **OntoLex-Lemon** (W3C CG) | the form/sense/reference lexicon interface; primary lexical projection | subsume + project | W3C CG report — spec use unencumbered |
| **Universal Dependencies** | the de facto morphosyntax standard; feature and relation inventories | subsume (inventories) + project (CoNLL-U) | guidelines CC-BY-SA 4.0; treebank licenses vary — *review per treebank* |
| **Unicode / UAX #15 / UTS #10** | character identity, scripts, normalization, collation | reference (properties referenced, never re-modeled) | Unicode License v3 |
| **Wikidata (lexemes + QIDs)** | authority ids for languages, lexemes, senses, concepts | link | CC0 |

### Realized grounding disposition

Every surveyed family has a concrete treatment; absence from the live bridge
catalog is not a vague deferral.

| Treatment | Families |
|---|---|
| Shipped grounding correspondences | OntoLex-Lemon, LexInfo 3.0, Global WordNet schema, NIF Core, and W3C Web Annotation in `mappings/lexical-bridges.ttl`, plus the existing Wikidata/identifier catalog in `mappings/equivalences.ttl` |
| Identifier or metadata linkage / generated view | LIME, OLiA, ILI, Universal Dependencies, UniMorph, Lexvo, Glottolog, BCP-47, ISO 639, and ISO 15924; use identifiers or inventories without importing them as the `lang:` canon |
| Codec or annotation projection | CoNLL-U, ISO 24617 SemAF, AMR, TEI, EBNF, ABNF, and GMN; these are serialized or annotation views with explicit loss/round-trip judgments, not fake term equivalences |
| Citation only | Unicode algorithms, restricted lexical resources, and theory/specification sources whose data cannot or should not be folded |
| Native authorship | Vantage-bearing denotation, co-resident ambiguity, audited translation, and the unified rendering theory where no adequate external ontology exists |

The live rows are oriented from `lang:` and ship in the meta-level
`graph/correspondence-laws` graph. Downstream slices consume the `lang:` terms;
they do not repeat OntoLex, NIF, or other target vocabulary terms.

## Lexical and sense resources

| Resource | Contributes | Relation | License / posture |
|---|---|---|---|
| WordNet (Princeton) | synsets, lexical relations | link | WordNet license (permissive) — data ingestion after review |
| Open English WordNet | maintained fork, RDF-native | link | CC-BY 4.0 |
| ILI (interlingual index) | cross-WordNet sense pivot | link | CC-BY 4.0 |
| BabelNet | multilingual sense graph | reference only | restrictive (non-commercial) — **no data ingestion** |
| Wiktionary / DBnary extracts | broad multilingual lexica | link, ingestion *review* | CC-BY-SA — share-alike implications gate any folding |
| lexinfo / OLiA | linguistic category registries for Lemon | project (property targets) | open — used as emission vocabulary |
| GOLD | general ontology for linguistic description | reference | inactive; historical alignment only |

## Morphosyntax, annotation, and meaning representation

| Resource | Contributes | Relation | License / posture |
|---|---|---|---|
| CoNLL-U format | token/feature/dependency exchange | project + ingest | spec open |
| UniMorph | inflectional feature schema across languages | link (feature alignment) | CC-BY-SA 4.0 |
| ISO 24617 (SemAF, all parts) | dialogue acts, semantic roles, time (ISO-TimeML), discourse | project (annotation surfaces) | ISO specs — *reference by citation, not reproduction* |
| AMR / PropBank / FrameNet / VerbNet | meaning-graph and role-inventory practice | project (AMR) + link (inventories) | AMR/PropBank open; FrameNet *review* (non-commercial data) |
| NIF 2.0 / W3C Web Annotation | stand-off anchoring | project | open |
| TEI P5 | scholarly document encoding | project | CC-BY 4.0 / BSD-2 |
| ISO-TimeML | temporal annotation of text | reference (the `temporal` slice owns time; the annotation surface routes through SemAF) | ISO |

## Identification and registries

| Resource | Contributes | Relation | License / posture |
|---|---|---|---|
| BCP-47 / RFC 5646 | language tagging | project (tag emission) | IETF |
| ISO 639-1/-3 (SIL registry) | language codes | link | registry open for identification use |
| ISO 15924 | script codes | link | open |
| Glottolog | languoid catalog with genealogy — richer than ISO 639 | link (preferred identity companion) | CC-BY 4.0 |
| IANA language subtag registry | the operative BCP-47 registry | link (generated-tag validation source) | IETF |
| CLDR | locale data | reference | Unicode License v3 |
| Lexvo | RDF ids for languages/scripts | link | CC-BY |

## Grammars, formal languages, and self-hosting surfaces

| Resource | Contributes | Relation | License / posture |
|---|---|---|---|
| ISO/IEC 14977 EBNF | grammar interchange | project + ingest (round-trip bar) | ISO — syntax use unencumbered |
| RFC 5234 ABNF | IETF grammar dialect | project + ingest | IETF |
| W3C Turtle / RDF 1.2 grammars | GMEOW's own serializations as sign systems | subsume (as `lang:Grammar` individuals) | W3C |
| GTS grammar | the bundle format's own grammar — the self-hosting apex | subsume (GMEOW-authored individual) | project-internal |
| Grammatical Framework (GF) | abstract/concrete syntax architecture; the buildability proof | reference (architecture), *not* a runtime dependency | GF runtime licensing mixed — no linkage planned |

## Token-efficient notations and symbol provenance (the GMN dialect)

The external surfaces the GMN dialect charter ([`LANG-GMN.md`](LANG-GMN.md)) surveys: the
token-compact notation patterns GMN learns from without adopting any as a canon, the tokenizer
practice its rate contract is measured against, and the symbol-provenance sources its glyph
citations draw on.

| Resource | Contributes | Relation | License / posture |
|---|---|---|---|
| TOON (token-oriented object notation) | schema-once tabular compaction pattern for LLM channels | reference (pattern) | open spec |
| JTON / Zen Grid | token-lean JSON restylings; grid-shaped record batching | reference (pattern) | open specs |
| ONTO (token-efficient ontology notation) | sigil-prefixed ontology records over the token channel | reference (pattern) | open spec |
| KL3M domain tokenizers | domain-tuned tokenizer practice; evidence that rate is tokenizer-relative | reference | open (model artifacts *review*) |
| Leipzig Glossing Rules | interlinear morpheme-gloss conventions — real notation for the lang symbology plane | project + reference | conventions freely usable |
| ISO 80000 | quantity/unit symbol canon — symbol provenance for the math symbology plane | reference (citation source) | ISO — *reference by citation, not reproduction* |
| Unicode UTS #39 | confusable-detection skeleton — the normative confusables rule | reference | Unicode License v3 |

## Theory (cited)

Frege, *Über Sinn und Bedeutung* (the sense/reference discipline); Peirce, the triadic sign
(interpretation as act); Montague, *Universal Grammar* / PTQ (compositional denotation into a
typed logic; the natural-formal continuity license); Saussure (system-relative form identity);
Austin & Searle (speech acts and force); Kaplan, *Demonstratives* (indexical anchoring); Ranta,
*Grammatical Framework* (abstract syntax as canon); Melʹčuk (morph/lexeme stratification); Nida
(translation loss as the object of study, not an embarrassment). Each is cited for its
claim-supporting role, per the repository's citation discipline.

## Engines (oracles, never authorities)

Candidate external engines for the [`LANG-RUNTIME.md`](LANG-RUNTIME.md) handoff seam, all held to
the oracle-not-authority rule and the divergence-ledger demotion discipline the logic stack
already applies:

| Engine | Use | Posture |
|---|---|---|
| UDPipe / Stanza / spaCy | UD parsing, tagging, lemmatization | oracle producing vantage-held readings; Rust-side invocation via subprocess seam; **no Python in-tree** — outputs ingested as CoNLL-U |
| Apertium | rule-based MT with published lexical data | data link *review* (GPL data); engine as oracle |
| ICU4X | segmentation, normalization, collation in Rust | candidate direct dependency (Rust-native, Unicode-licensed) — the one entry here that may join the core rather than sit behind the seam |
| Lindera / Vaporetto | Rust tokenizers (CJK) | candidate direct dependency, review at implementation |

The deliberate absence: **no LLM in the conformance path.** LLM-produced analyses are lawful as
oracle readings with engine vantage like any other engine output, but no gate may depend on one —
gates must be deterministic, and the acceptance bar of the runtime charter is deterministic
round-trips and typed failures throughout.

## GMEOW-authored surfaces (no adequate external source exists)

Recorded as original, per the honesty rule:

- the **reified denotation record** with kind-typed targets bottoming out in `logic:` — no
  external vocabulary carries denotation with vantage, context, and preservation;
- the **co-resident reading model** for ambiguity — annotation formats pick winners; none holds
  the multiplicity as first-class data with vantages;
- **translation as `logic:Correspondence`** with per-unit preservation judgments — translation
  memory formats (TMX) carry pairs, not audited maps; TMX is a projection target only where a
  consumer materializes;
- the **analyzed/unanalyzed discipline** as a recorded status over every surface form;
- the **rendering relation** as a single grounded theory across notations, serializations, and
  documentation.
