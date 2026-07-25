<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Projections — exporting GMEOW to pure FOAF / schema.org / GeoSPARQL / vCard

GMEOW's definitions are richer than the vocabularies people consume. **SSSOM**
gives 1:1 term equivalence; projecting *down* to a target needs **structural
transformations** SSSOM can't express. GMEOW's alignment stack has four standard
artifacts — but they are **no longer hand-authored four ways**. A single
GMEOW-grounded mapping frontend is the authoring source: slice-owned linkage and
projection cells live beside the slice, while shared projection enrichment and
transform declarations live under `dsl/mappings/`. The registered mappings
generator renders all four (see [Single-source compilation](#single-source-compilation)):

| Layer | Expresses | Generated artifact |
|---|---|---|
| SSSOM | 1:1 term equivalence | `generated/mappings/*.sssom.tsv` |
| **EDOAL** | complex alignments (value→class, compositions, conditions) + `edoal:measure` | `generated/projections/*.edoal.ttl` |
| **FnO** | the transformation *functions* EDOAL invokes (+ the language conversion catalog) | `generated/projections/functions.fno.ttl`, `dsl/mappings/transforms.fno.ttl`† |
| **SPARQL CONSTRUCT** | the executor → a pure-profile graph | `generated/queries/*.rq` |

†`dsl/mappings/transforms.fno.ttl` (the language-conversion catalog) is the one hand-authored
FnO file — it is a different concern (script→script / language→language domain
functions), not a GMEOW→external projection, so it stays outside the compiler.

FnO + EDOAL are the declarative, standards-consumable **spec**; the CONSTRUCT is
the native Rust-rendered executable view. **None of this is imported into the reasoned
core** — it is a consumable *view* layer. That separation is the whole point:
SSSOM equivalence is a logical claim the reasoner enforces, so it must be honest;
a **projection is deliberately lossy and directional** — it downgrades GMEOW into
a target consumer's vocabulary without corrupting the canonical model. The OWL 2
axiom-annotation encoding of GMEOW's RDF 1.2 statement metadata is a projection of
exactly this kind — a *reasoning-lossless* downcast for tools that cannot yet consume
RDF 1.2 ([Principles 3–4](../CONSTITUTION.md)) — not a competing source of truth.

## Run it

```sh
gmeow project                       # project the worked-example fixtures, all profiles
gmeow project --profile schema-org  # one profile
gmeow project --data mydata.ttl     # project your own GMEOW data
```

Outputs a round-trip-verified `dist/gmeow-example-<profile>.ttl` for each profile that has a
worked-example fixture (schema.org, GeoSPARQL, vCard, FOAF, iCalendar, OWL-Time today). The
complete projection set — those plus ODRL, CC REL, Dublin Core, SPDX, BOT, RDF Data Cube
(`qb` — now DSD-complete with per-observation DataSet + DataStructureDefinition),
OntoLex-Lemon, W3C Web Annotation, DCAT 3 (`dcat` — instance datasets +
distributions + checksums, the DCAT catalog leg; see
[research-objects.md](./research-objects.md)), and the five standpoint
projections (CRMinf,
Web Annotation, PROV-O, schema:Claim, Standpoint-OWL 2) — is generated as
`generated/queries/*.rq` by `make regen`. A
target-by-target summary with spec links is in the
[README projection-targets table](../README.md#projection-targets).

## Transformation types (worked on locations + naming + languages)

| Type | Example | Function |
|---|---|---|
| **value→class** | `placeType placeTypeCountry` → `rdf:type schema:Country` | `fnPlaceTypeToClass` |
| **datatype retag** | `gmeow:asWKT "POINT…"` (rdfs:Literal) → `geo:asWKT …^^geo:wktLiteral` | `fnRetagWkt` |
| **multi-property combine** | `latitude` + `longitude` → `POINT(lon lat)^^geo:wktLiteral` | `fnLatLongToWktPoint` |
| | `languageCode` + `scriptCode` → BCP-47 `ja-Hani` (`schema:alternateName`) | `fnComposeBcp47` |
| **compose / select** | displayable `fullName` → `schema:name`/`vcard:fn`/`foaf:name` | `fnSelectDisplayName` |
| | displayable endonym `PlaceName` → `schema:name` (frame choice, not a primary) | `fnSelectEndonym` |
| | displayable exonym `PlaceName` → `schema:alternateName` | `fnSelectExonym` |
| **relator flatten** | `LanguageProficiency` (agent×lang×level) → `schema:knowsLanguage` (lossy) | `fnProficiencyToKnownLanguage` |
| **value→property by sub-value** | `honorific` (+ position) → `schema:honorificPrefix`/`Suffix` | `fnHonorificToAffix` |
| **structured→flat field** | nickname-purpose `PersonName` → `foaf:nick` / `vcard:nickname` / `schema:alternateName` | `fnNicknameName` |
| | birth event (`eventType` birth) `eventTime`, via the principal `Participation` → `schema:birthDate` / vCard `BDAY` | `fnBirthEventToDate` |
| | `Membership` `Role` → `schema:jobTitle` / vCard `TITLE` | `fnMembershipToJobTitle` |
| | `hasWebPage` → `schema:url` / `foaf:homepage` / `vcard:hasURL` | `fnWebPageToUrl` |
| | `subOrganizationOf` → `schema:department` | `fnSubOrgToDepartment` |
| **domain conversion** (catalog) | transliteration / transcription / translation as `fno:Function`s | `transforms.fno.ttl` |
| **lossy drop** | `StorageLocation`, fine place types, `authorityLink`, pronouns, `NameUsage`, version lineage, proficiency level | — |

The **structured→flat field** transforms are the contact-card downcasts: GMEOW has
**no** flat `nickname` / `birthDate` / `jobTitle` properties — a nickname is a
structured `PersonName`, a birth date is a `Birth` event, a job title is a `Role` in
a `Membership` — so the flat schema.org / vCard / FOAF fields are *reconstructed on
projection*, never stored. `gmeow:description` (the one genuinely-unstructured note)
and `gmeow:hasWebPage` are 1:1 / near-1:1 renames.

The **language conversion catalog** (`projections/transforms.fno.ttl`) is a
different use of FnO: it declares Hepburn / Pinyin / ISO 233 / IPA / translate as
*domain* functions (script→script, language→language), each linked to its
`gmeow:TransliterationScheme` value individual, so a `gmeow:romanization` records
*how* it was derived. These are declarative specs with no bound executor.

## Profile lossiness

- **schema.org** — richest fit: place value→class, decomposed addresses, GeoCoordinates, co-equal `schema:name`s, honorifics, `schema:Language`/`ComputerLanguage` with composed BCP-47 tags and flattened `schema:knowsLanguage`. Drops StorageLocation, fine place types, NameUsage/register/script, proficiency level, version lineage.
- **GeoSPARQL** — geometry only: `geo:Feature` + WKT (retagged), lat/long→POINT, `geo:sfWithin`. Drops names, addresses, types.
- **vCard** — contact-card fit: `vcard:fn`, `vcard:given-name`/`family-name`, `vcard:nickname`, `vcard:bday`, `vcard:title`, `vcard:Address` components, `vcard:hasURL`, `vcard:hasGeo`, and free-text `vcardx:pronouns` (the RFC 9554 extension — no core vCard-RDF predicate exists). Drops the nested place hierarchy + QIDs, geometry.
- **FOAF** — lowest common denominator: place+coords → `wgs84:SpatialThing`, `foaf:name`, `foaf:based_near`. Drops nearly all structure.
- **iCalendar** — calendar fit: a `gmeow:Event` → `ical:Vevent` with `ical:dtstart`/`dtend` (from the crisp interval, the point `eventTime`, or the fuzzy `earliestStart`/`latestEnd` bounds), `ical:summary` (the event-type label), `ical:location`, and `ical:attendee` (the flat participants). Drops the reified `Participation` roles/periods/confidence/standpoint, the open type vocabulary beyond one summary label, `temporalPrecision`, the sub-event tree, and `EventSeries` recurrence.
- **OWL-Time** — temporal-relation fit: the 13 Allen relations between events (`gmeow:before`/`during`/`meets`/…) → OWL-Time's `time:interval*` relations 1:1, so an OWL-Time-aware reasoner runs interval-algebra inference over the result (the events are treated as their `time:ProperInterval` extents). Drops everything but the qualitative temporal ordering. See [TQL](temporal-queries.md).

## Naming: the co-equality + deadname contract (honoured by every profile)

The names model has no "primary" name (anti-colonial co-equality) and a single
display control, `gmeow:displayable`. The projection therefore:

- emits **every** displayable `fullName` as the target name property — Patrick's
  `"Patrick Colm Audley"@en` and `"欧德理"@zh-Hans` are **both** `schema:name`s,
  neither privileged;
- **suppresses** any `displayable false` name — a recorded deadname is never
  emitted to any target. (`fnSelectDisplayName`.)

## Single-source compilation

The four artifacts above are **generated**, not hand-authored. The same mapping
used to live four ways and drift independently (an FnO param typed wrong, an EDOAL
cell out of sync with its executor, a SSSOM row mapped to an inverse term). GMEOW's
own doctrine — *one canonical source, everything else a generated lossy projection*
([Principle 4](../CONSTITUTION.md)) — now applies to the mapping layer itself:
**author each mapping once, generate the four.**

The authoring source is a GMEOW-grounded Turtle frontend (vocabulary in
`dsl/mappings/vocabulary.ttl`, all in the `gmeow:` namespace, a spec layer never
reasoned over):

- `slices/<group>/<name>/mappings/equivalences.ttl` — slice-owned native alignment
  cells (a reified `skos:*Match`/`owl:equivalent*` statement carrying
  `gmeow:sssomFile`), one per SSSOM row.
- `slices/<group>/<name>/mappings/projections-<profile>.ttl` — slice-owned
  `gmeow:ProjectionMapping` cells.
- `dsl/mappings/projections/*.ttl` — shared or cross-slice projection enrichment.
- `dsl/mappings/transforms.fno.ttl` — shared FnO function declarations.

Each projection cell produces one CONSTRUCT branch and names an **anchor** (the
node the output hangs on), a GMEOW-side graph pattern, and per-profile
**bindings** (target term, EDOAL relation, target kind, transform, confidence,
SSSOM emission metadata, and loss notes). The irregular transforms (BCP-47
compose, WKT POINT, the `vcard:Name` mint) are expressed in a small **closed
algebra** (`CONCAT`/`COALESCE`/`IF`/`STR`/`IRI`/`STRDT`/`regex` +
alt/seq/zero-or-more property paths) — **no raw SPARQL** appears in the source.

```sh
make regen        # render registered generated artifacts from canonical sources
make check-sync   # CI gate: fail if a committed artifact is stale
```

Two properties hold **by construction**, eliminating the bug classes review used
to catch:

- each FnO parameter's `fno:type` is **derived from the predicate's `rdfs:range`**
  in the ontology — it can never disagree;
- the EDOAL `entity2` set and the SPARQL-emitted term set come from the *same*
  binding list — they cannot drift.

The native `gmeow_slice.lint_projection` trio runs the three cross-layer
invariants over the committed projection tree and surfaces any problem as a
`mapping-compile.{fno-type,fno-ref,spec-drift}` finding (folded into the dev-gate
report). `gmeow-dev sync --mode check --outputs generated` is wired into CI as the standing
no-drift regression. Adding a new projection is now a **single DSL cell**, not four
edits.

### Maximal use of the four target languages

The compiler uses each emitted standard to its full expressive extent (so the
DSL is ready for a much richer future ontology, not just today's):

- **EDOAL** — a multi-hop traversal sets `gmeow:edoalPath true` and its
  `align:entity1` is **derived as a real relation path** with `edoal:compose` +
  `edoal:inverse` (a birth-event date, reached through the principal
  `gmeow:Participation`, becomes
  `compose(inverse(participationParticipant), participationEvent, eventTime)`),
  so the alignment is genuinely
  declarative rather than a bare class + opaque transform. Each cell also carries
  `edoal:measure` (confidence).
- **FnO** — each function declares **how it executes**: an `fno:Implementation`
  per profile `.rq` and an `fno:Mapping` (the `fnom` vocabulary) binding every
  parameter and the output to the SPARQL variable that realises it.
- **SSSOM** — files carry deterministic provenance (`mapping_tool`,
  `mapping_tool_version`, `mapping_set_version`, `mapping_date`) and a `curie_map`
  of the prefixes they use; `subject_label`/`object_label` columns appear when a
  cell populates them (e.g. the Wikidata item labels).
- **SPARQL** — the closed algebra spans the full property-path set (alt / seq /
  `^` inverse / `*` / `+` / `?` / `!(…)` negated) and expression set (the string,
  language, datatype, arithmetic, comparison and boolean operators, `IN`), all as
  GMEOW operator individuals — still **no raw SPARQL** in the source.

### Authoring a mapping

Every DSL term carries an `rdfs:label` + `skos:definition` in
`dsl/mappings/vocabulary.ttl` — that file is the authoritative field/operator
reference. There are two cell types.

**A cross-ontology term link** → one SSSOM row. Add it to the matching
slice-local `mappings/equivalences.ttl`:

```turtle
# The native RDF-1.2 form: the match relation is asserted directly on the term, and
# the SSSOM side-data rides the statement's reifier. The predicate is one of
# skos:*Match / owl:equivalent* / owl:sameAs / rdfs:sub*Of.
gmeow:Person owl:equivalentClass foaf:Person {|
    gmeow:confidence    1.0 ;
    gmeow:objectLabel   "person" ;                # optional → object_label column
    gmeow:justification semapv:ManualMappingCuration ;
    gmeow:sssomFile     "gmeow-classes.sssom.tsv"
|} .
```

**A projection (a lossy downcast)** → an EDOAL cell + FnO function + a SPARQL
branch. Add it to the owning slice's `mappings/projections-<profile>.ttl` when
the mapping belongs to a slice, or to `dsl/mappings/projections/<profile>.ttl`
when it is shared projection enrichment. A cell names an **anchor** (the node the
output hangs on) and a **value**, a GMEOW-side graph pattern of `gmeow:atom`s,
and one `gmeow:hasBinding` per target profile:

```turtle
gmeow:mapSchemaBirthDate a gmeow:ProjectionMapping ;
    rdfs:label "Birth life-event date → schema:birthDate"@en ;
    gmeow:hasMappingPattern [
        gmeow:anchor "person" ; gmeow:value "bdate" ;
        gmeow:atom (
            [ gmeow:subjectVar "birth" ; gmeow:predicate gmeow:eventType ; gmeow:objectValue gmeow:eventTypeBirth ]
            [ gmeow:subjectVar "p" ; gmeow:predicate gmeow:participationEvent ; gmeow:objectVar "birth" ]
            [ gmeow:subjectVar "p" ; gmeow:predicate gmeow:participationRole ; gmeow:objectValue gmeow:roleParticipantPrincipal ]
            [ gmeow:subjectVar "p" ; gmeow:predicate gmeow:participationParticipant ; gmeow:objectVar "person" ]
            [ gmeow:subjectVar "birth" ; gmeow:predicate gmeow:eventTime ; gmeow:objectVar "bdate" ] ) ;
        gmeow:edoalPath true ] ;                         # entity1 ← derived compose/inverse path
    gmeow:hasBinding [
        gmeow:profile "schema-org" ; gmeow:toPredicate schema:birthDate ;
        gmeow:relation "<=" ; gmeow:transform gmeow:fnBirthEventToDate ; gmeow:confidence 0.95 ] .
```

Key authoring choices, each a single field on the pattern or binding:

- **value→class** (`gmeow:valueClassMap`) — a source value individual → a target
  class (`placeTypeCountry → schema:Country`), rendered as a SPARQL `VALUES` table
  plus EDOAL `AttributeValueRestriction` cells.
- **suppression** (`gmeow:suppressWhen`) — the displayable/deadname contract, a
  `FILTER NOT EXISTS`.
- **generalization / coarsening** (`gmeow:coarsenTo`) — the *other* half of
  disclosure control by projection (CONSTITUTION P10): a value marked
  `gmeow:coarsenTo <GranularityLevel>` is emitted at a **coarser** level rather than
  withheld. Authored as a pair of complementary cells — the precise value guarded by
  `suppressWhen` on `coarsenTo`, and a coarsen cell that walks `gmeow:containedInPlace+`
  (the mereology spine) to the enclosing ancestor at the target level and emits *its*
  value. "A coarser region rather than exact coordinates, never deletion." Worked in
  the GeoSPARQL (`mapGeoPointCoarsened`) and schema.org (`mapSchemaPlaceCoordsCoarsened`)
  projections; aligned by reference to `dpv:Generalisation`. Heavier geomasking /
  k-anonymity stays in the solver layer (P12). The access/consent *trigger* on this
  same control is PRIV-GEN.
- **composed/derived values** (`gmeow:bind` / `gmeow:mint`) — a closed expression
  algebra (`gmeow:opConcat`, `opIf`, `opStrDatatype`, `opStrLang`, … — never raw
  SPARQL); multi-triple outputs use `gmeow:templateAtoms`.
- **EDOAL entity1** — `gmeow:edoalPath true` derives the relation path for a
  traversal; otherwise `gmeow:edoalSource` names the salient term; otherwise the
  projection is structural / SSSOM-backed (no EDOAL cell).

After any change, run `make regen` or the registered generator in check mode; the
compiler runs the cross-layer invariants on its own output and refuses to emit on
violation. Never hand-edit generated mapping artifacts under `generated/mappings/`,
`generated/projections/`, or `generated/queries/` — `make check-sync` fails on drift.

## GMN-1 — the token-compact model notation projection

Alongside the vocabulary downcasts above, the `lang:` grounding slice projects
**GMN-1** (Grounded Model Notation), a token-compact serialization of the model
authored for LLM producers and constrained decoding. Like every projection it is a
lossy, directional **view of `gmeow.gts`** — graph-derived, never hand-authored —
and it is version-keyed by the graph-resolved dialect major under
`generated/projections/lang/gmn1/v<major>/**` (see
[the pipeline spine § 6.1](./PIPELINE_SPINE.md)). The ecosystem projects to:

| Layer | Expresses | Generated artifact |
|---|---|---|
| **EBNF / ABNF grammar** | the reference GMN grammar, per formalism | `generated/projections/lang/ebnf/gmn.ebnf`, `generated/projections/lang/abnf/gmn.abnf` |
| **GBNF / Lark grammar** | the same graph-derived grammar as a real **constrained-decode** artifact | `generated/projections/lang/gmn1/v*/gbnf/gmn.gbnf`, `.../v*/lark/gmn.lark` |
| **token-metrics** | a math-grounded `gmeow:Measurement` 7-vector (byte-fallback compression gate) | `generated/projections/lang/gmn1/v*/token-metrics.ttl` |
| **verbalizations** | GMN↔controlled-NL `lang:translationCorrespondence` pairs | `generated/projections/lang/gmn1/v*/verbalizations.ttl` |
| **primer card** | a ~500-token teachability card | folded into `llms.txt` / `llms-full.txt` + MCP `gmeow://ontology/gmn1-primer` |
| **training corpus** | a rejection-sampled, proof-carrying corpus (`stage-gmn-training-corpus`) | bundle-internal graph `graph/gmn-training-corpus` |

The verifier surface (`gmn_validate` / `gmn_expand` / `gmn_explain`) is documented in
[the MCP server guide](./mcp-server.md). Never hand-edit these artifacts — they
regenerate from the bundle and are drift-gated exactly like the mapping projections above.
