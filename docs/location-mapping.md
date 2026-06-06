<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Locations — modelling & interoperability guide

GMEOW models **where** as `gmeow:Location`, with structurally-distinct kinds as
subclasses and homogeneous kinds as value vocabularies:

- **`gmeow:Place`** — geographic, at any granularity (country → region → city →
  thoroughfare → premises → building → floor → room). The granularity is the
  **`placeType` value vocabulary**, not subclasses.
- **`gmeow:VirtualLocation`** — online (`accessUrl`, `virtualPlatform`).
- **`gmeow:StorageLocation`** — where digital objects reside (`storageService`,
  `storageMedium` value, `storagePath`, `physicalPlace → Place`, `storedIn`).

## The five interoperability layers

1. **Term alignment** — classes/properties ≡/closeMatch schema.org, GeoSPARQL,
   vCard, WGS84 (see `mappings/gmeow-places.sssom.tsv`).
2. **Coreference / gazetteer identity** — link a place to its external records by
   IRI: `skos:exactMatch` (asserted identity) and/or `gmeow:authorityLink`
   (see-also). **Wikidata is the hub**: linking the WD item transitively yields
   GeoNames (P1566), Getty TGN (P1667), OSM (P402) and ISO 3166 (P297).
3. **Hierarchy / topology** — `containedInPlace` (transitive) aligns to
   `geo:sfWithin`, `gn:parentFeature`, `wdt:P131`, `dcterms:isPartOf`,
   `schema:containedInPlace`.
4. **Round-trip fidelity** — full vCard `ADR`/`GEO`/`TZ` (table below).
5. **Names & timezone** — co-equal `gmeow:PlaceName` toponyms borne via
   `gmeow:hasPlaceName` (multilingual / endonym / exonym / historical, for gazetteer
   matching; the names module, issue #105 — the structured replacement for the retired
   flat `gmeow:alternateName`) and `gmeow:timezone` (IANA, also feeds the calendar slice).

## Address: surface literals ↔ resolved QID-bearing places

A `gmeow:PostalAddress` holds the **as-written** components; the **resolved**
geographic hierarchy is a chain of first-class `gmeow:Place`s, each able to carry
its own identifier. The two coexist, bridged by `addressPlace → containedInPlace*`:

```turtle
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix skos:  <http://www.w3.org/2004/02/skos/core#> .
@prefix wd:    <http://www.wikidata.org/entity/> .
@prefix ex:    <https://example.org/loc/> .

# Resolved hierarchy — a QID at every level (Wikidata as hub).
ex:canada       a gmeow:Place ; gmeow:placeType gmeow:placeTypeCountry ; skos:exactMatch wd:Q16 .
ex:alberta      a gmeow:Place ; gmeow:placeType gmeow:placeTypeRegion ;  skos:exactMatch wd:Q1951 ;
                gmeow:containedInPlace ex:canada .
ex:spruceGrove  a gmeow:Place ; gmeow:placeType gmeow:placeTypeCity ;    gmeow:containedInPlace ex:alberta .
ex:westbourne112 a gmeow:Place ; gmeow:placeType gmeow:placeTypePremises ; gmeow:containedInPlace ex:spruceGrove ;
                gmeow:hasCoordinates [ gmeow:latitude 53.544972 ; gmeow:longitude -113.924398 ] .
ex:homeOffice   a gmeow:Place ; gmeow:placeType gmeow:placeTypeRoom ;    gmeow:containedInPlace ex:westbourne112 .

# Surface form — what was written / what a vCard carries.
ex:addr a gmeow:PostalAddress ;
    gmeow:streetAddress "112 Westbourne Rd" ; gmeow:addressLocality "Spruce Grove" ;
    gmeow:addressRegion "AB" ; gmeow:postalCode "T7X 0A1" ; gmeow:countryCode "CA" ;
    gmeow:addressPlace ex:westbourne112 .
```

This is only possible because place-kind is a **value** (`placeType`): you can mint
a place at any level — even a room — and give it a QID, without new classes.

## vCard round-trip

| vCard | GMEOW |
|---|---|
| `ADR` street-address | `gmeow:streetAddress` |
| `ADR` extended-address | `gmeow:extendedAddress` |
| `ADR` post-office-box | `gmeow:postOfficeBox` |
| `ADR` locality | `gmeow:addressLocality` |
| `ADR` region | `gmeow:addressRegion` |
| `ADR` postal-code | `gmeow:postalCode` |
| `ADR` country-name | `gmeow:countryCode` (ISO 3166-1 alpha-2 by convention) |
| `GEO` | `gmeow:hasCoordinates` → `latitude`/`longitude` |
| `TZ` | `gmeow:timezone` (IANA) |

## GeoSPARQL: geometry & spatial queries

The OWL range of `gmeow:asWKT` is `rdfs:Literal` (so a custom `geo:wktLiteral`
range can't push the ontology out of OWL 2 DL). **Emit data with the
`^^geo:wktLiteral` tag**, and because `gmeow:Geometry`/`hasGeometry`/`asWKT` align
to `geo:Geometry`/`geo:hasGeometry`/`geo:asWKT` and `gmeow:Place` to `geo:Feature`,
a GeoSPARQL engine can consume GMEOW places after a one-step projection:

```sparql
CONSTRUCT {
  ?place a geo:Feature ; geo:hasGeometry ?g .
  ?g a geo:Geometry ; geo:asWKT ?wkt .
} WHERE {
  ?place gmeow:hasGeometry ?g . ?g gmeow:asWKT ?wkt .
}
```

Then `geof:within` / `geof:distance` and topological `geo:sfWithin` work natively.

## Place-type values vs the `schema:Country ⊑ Place` alignment

GMEOW's own discriminator is the **`placeType` value** (`placeTypeCountry`). The
alignment layer also keeps `schema:Country rdfs:subClassOf gmeow:Place` — these are
**not** contradictory: the subsumption lets external data typed `schema:Country`
flow in *as* a `gmeow:Place`, while GMEOW marks its kind with the value. We do not
mint a GMEOW `Country` class. `placeType` itself aligns to Getty's `gvp:placeType`
and the GeoNames feature classes (`gn:A`/`gn:P`/…).

## Storage & virtual locations (for the events/documents slices)

`StorageLocation` is the structured form of the import-provenance
`Source.sourceLocation` string: `ex:source gmeow:storedIn ex:drive` where
`ex:drive a gmeow:StorageLocation ; gmeow:storageMedium gmeow:storageMediumCloudService ;
gmeow:storageService "Google Drive"`. A physical disk sets `gmeow:physicalPlace`
to the room it sits in — a `StorageLocation` composed with a `Place`.
`VirtualLocation` (a meeting URL) is what the calendar slice will use for online
events, alongside geographic `Place`s.

## Reference Frame Profiles & Extensibility (Principle 11)

Every spatial measurement or coordinate tuple is relative to a reference system. GMEOW models this relative geometry by separating frame-independent structure (topology) from frame-relative values (geometry) through **Reference Frame Profiles**:

- **`gmeow:ReferenceFrame`** describes coordinate reference systems (CRS), grids, datums, or local platform coordinates.
- Each reference frame declares its parameters via descriptors:
  - **`gmeow:frameRealm`** (e.g. terrestrial, indoor, celestial, virtual).
  - **`gmeow:hasAxis`** points to its coordinate axes (`gmeow:Axis`).
  - **`gmeow:dimensionCount`** (e.g. `3` for 3D).
  - **`gmeow:frameKind`** (e.g. geodetic, Cartesian, polar).
  - **`gmeow:requiresHost`** (boolean indicating if the frame depends on a physical host).
  - **`gmeow:determinacyModel`** (e.g. crisp, fuzzy, vague).
  - **`gmeow:parentFrame`** & **`gmeow:transformsTo`** define coordinate hierarchical nesting and mathematical transformation targets.
  - **`gmeow:frameSolver`** points to external software packages or solvers responsible for coordinate updates (Principle 12).

### Authoring Guidance: Adding a Novel Spatial Realm

To introduce a new domain (e.g. robotic configuration space, narrated fictional settings, or astronomical coordinates) without modifying the core ontology classes:
1. **Declare the Spatial Realm**: Create a new individual of type `gmeow:SpatialRealm` (e.g., `ex:narrativeRealm`).
2. **Define a Reference Frame Profile**: Declare a `gmeow:ReferenceFrame` instance with complete profile descriptors (including `gmeow:frameRealm`, `gmeow:hasAxis`, `gmeow:dimensionCount`, `gmeow:frameKind`, `gmeow:requiresHost`, and `gmeow:determinacyModel`). All of these properties are required by the SHACL shapes (validated in `test_shapes.py`), so omitting `gmeow:requiresHost` or any other mandatory descriptor will cause validation to fail.
3. **Align by Reference**: Add external vocabulary mappings in your domain-specific mapping DSL file (e.g. using `skos:closeMatch` or `skos:relatedMatch` to standard terms), leaving core class definitions untouched.
