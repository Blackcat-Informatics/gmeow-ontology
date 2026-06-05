<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Projections — exporting GMEOW to pure FOAF / schema.org / GeoSPARQL / vCard

GMEOW's definitions are richer than the vocabularies people consume. **SSSOM**
gives 1:1 term equivalence; projecting *down* to a target needs **structural
transformations** SSSOM can't express. GMEOW's four-layer alignment stack:

| Layer | Expresses | Artifact |
|---|---|---|
| SSSOM | 1:1 term equivalence | `mappings/*.sssom.tsv` |
| **EDOAL** | complex alignments (value→class, compositions, conditions) | `projections/*.edoal.ttl` |
| **FnO** | the transformation *functions* EDOAL invokes (+ the language conversion catalog) | `projections/functions.fno.ttl`, `projections/transforms.fno.ttl` |
| **SPARQL CONSTRUCT** | the executor → a pure-profile graph | `queries/projections/*.rq` |

FnO + EDOAL are the declarative, standards-consumable **spec**; the CONSTRUCT is
the **executor** (pure-Python rdflib). **None of this is imported into the reasoned
core** — it is a consumable *view* layer. That separation is the whole point:
SSSOM equivalence is a logical claim the reasoner enforces, so it must be honest;
a **projection is deliberately lossy and directional** — it downgrades GMEOW into
a target consumer's vocabulary without corrupting the canonical model.

## Run it

```sh
gmeow project                       # project the worked-example fixtures, all profiles
gmeow project --profile schema-org  # one profile
gmeow project --data mydata.ttl     # project your own GMEOW data
```

Outputs `dist/gmeow-example-{schema-org,geosparql,vcard,foaf}.ttl` (round-trip
verified). Also runs in `gmeow build`.

## Transformation types (worked on locations + naming + languages)

| Type | Example | Function |
|---|---|---|
| **value→class** | `placeType placeTypeCountry` → `rdf:type schema:Country` | `fnPlaceTypeToClass` |
| **datatype retag** | `gmeow:asWKT "POINT…"` (rdfs:Literal) → `geo:asWKT …^^geo:wktLiteral` | `fnRetagWkt` |
| **multi-property combine** | `latitude` + `longitude` → `POINT(lon lat)^^geo:wktLiteral` | `fnLatLongToWktPoint` |
| | `languageCode` + `scriptCode` → BCP-47 `ja-Hani` (`schema:alternateName`) | `fnComposeBcp47` |
| **compose / select** | displayable `fullName` → `schema:name`/`vcard:fn`/`foaf:name` | `fnSelectDisplayName` |
| **relator flatten** | `LanguageProficiency` (agent×lang×level) → `schema:knowsLanguage` (lossy) | `fnProficiencyToKnownLanguage` |
| **value→property by sub-value** | `honorific` (+ position) → `schema:honorificPrefix`/`Suffix` | `fnHonorificToAffix` |
| **domain conversion** (catalog) | transliteration / transcription / translation as `fno:Function`s | `transforms.fno.ttl` |
| **lossy drop** | `StorageLocation`, fine place types, `authorityLink`, pronouns, `NameUsage`, `WritingSystemUsage`, version lineage, proficiency level | — |

The **language conversion catalog** (`projections/transforms.fno.ttl`) is a
different use of FnO: it declares Hepburn / Pinyin / ISO 233 / IPA / translate as
*domain* functions (script→script, language→language), each linked to its
`gmeow:TransliterationScheme` value individual, so a `gmeow:romanization` records
*how* it was derived. These are declarative specs with no bound executor.

## Profile lossiness

- **schema.org** — richest fit: place value→class, decomposed addresses, GeoCoordinates, co-equal `schema:name`s, honorifics, `schema:Language`/`ComputerLanguage` with composed BCP-47 tags and flattened `schema:knowsLanguage`. Drops StorageLocation, fine place types, NameUsage/register/script, WritingSystemUsage, proficiency level, version lineage.
- **GeoSPARQL** — geometry only: `geo:Feature` + WKT (retagged), lat/long→POINT, `geo:sfWithin`. Drops names, addresses, types.
- **vCard** — contact-card fit: `vcard:Address` components, `vcard:fn`, `vcard:given-name`/`family-name`. Drops the nested place hierarchy + QIDs, geometry.
- **FOAF** — lowest common denominator: place+coords → `wgs84:SpatialThing`, `foaf:name`, `foaf:based_near`. Drops nearly all structure.

## Naming: the co-equality + deadname contract (honoured by every profile)

The names model has no "primary" name (anti-colonial co-equality) and a single
display control, `gmeow:displayable`. The projection therefore:

- emits **every** displayable `fullName` as the target name property — Patrick's
  `"Patrick Colm Audley"@en` and `"欧德理"@zh-Hans` are **both** `schema:name`s,
  neither privileged;
- **suppresses** any `displayable false` name — a recorded deadname is never
  emitted to any target. (`fnSelectDisplayName`.)

## Future

The CONSTRUCT executors are hand-authored and kept in sync with the FnO/EDOAL
specs; auto-generating the CONSTRUCT from the EDOAL alignment is a future
enhancement. Other modules extend the same framework by adding cells + functions.
