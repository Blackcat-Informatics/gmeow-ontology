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

## Privacy by generalization: coarsening coordinates (#72 / #79)

A place may declare the coarsest level at which its location should be disclosed:

```turtle
ex:secretLab a gmeow:Place ;
    gmeow:containedInPlace ex:metropolis ;     # … ⊂ city ⊂ region ⊂ country
    gmeow:hasCoordinates ex:secretCoords ;     # the precise point (retained, never deleted)
    gmeow:coarsenTo gmeow:granularityCity .     # disclose no finer than city

ex:metropolis a gmeow:Place ;
    gmeow:hasGranularity gmeow:granularityCity ;
    gmeow:hasCoordinates ex:metroCoords .       # the city's representative point
```

At projection time the precise point is **suppressed** and the enclosing ancestor at
the target `gmeow:GranularityLevel` (reached along the `gmeow:containedInPlace+`
mereology spine) is emitted instead — *a coarser region rather than exact coordinates,
never deletion* (CONSTITUTION P10). This is the **coarsen** half of the unified
disclosure-control mechanism; `gmeow:displayable false` is the **withhold** half.
`gmeow:GranularityLevel` is an ordered axis (`gmeow:coarserThan`) aligned by reference
to OWL-Time `time:TemporalUnit` (temporal) and `gmeow:placeType` / ISO 19112
LocationType (spatial); the operation aligns to `dpv:Generalisation`. Heavier geomasking
/ k-anonymity stays in the solver layer (P12). The GeoSPARQL and schema.org projections
both honour it (`mapGeoPointCoarsened`, `mapSchemaPlaceCoordsCoarsened`); the
access/consent *trigger* on the same control is PRIV-GEN (#73).

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

Every measured or expressed value is relative to an explicit reference system. GMEOW models this by separating frame-independent **structure** (topology, containment, order) from frame-relative **values** through **Reference Frame Profiles**:

- **`gmeow:ReferenceFrame`** describes any reference system — not only spatial CRS, but also units of measure, currencies, calendars/timescales, colourspaces, and languages/registers.
- Each reference frame declares its parameters via descriptors:
  - **`gmeow:frameRealm`** (e.g. terrestrial, indoor, celestial, virtual, measurement, currency, temporal, colourspace, linguistic).
  - **`gmeow:hasAxis`** points to its coordinate axes or dimensions (`gmeow:Axis`).
  - **`gmeow:dimensionCount`** (e.g. `3` for 3D, `6` for a Gregorian calendar).
  - **`gmeow:frameKind`** (e.g. geodetic, Cartesian, polar, scalar, temporal).
  - **`gmeow:requiresHost`** (boolean indicating if the frame depends on a physical host).
  - **`gmeow:determinacyModel`** (e.g. crisp, fuzzy, vague).
  - **`gmeow:parentFrame`** & **`gmeow:transformsTo`** define hierarchical nesting and mathematical transformation targets.
  - **`gmeow:frameSolver`** points to external software packages or solvers responsible for frame conversion (Principle 12).
- **`gmeow:hasReferenceFrame`** is the universal property linking any entity or value to its reference frame. `gmeow:coordinateFrame` is a sub-property for spatial coordinates specifically.

Seed reference frames are provided for all realms — spatial (WGS-84, local grid, celestial equatorial, robot base, virtual platform), measurement (SI), currency (USD), temporal (Gregorian, Unix epoch), colourspace (sRGB, CMYK), and linguistic (English). External ontologies are aligned by reference: QUDT and OM for measurement, FIBO for currency, OWL-Time `time:TRS` for temporal reference systems, and Lexvo for language instances.

## Distance, Proximity, and Frame-Declared Metrics (#95)

`gmeow:MetricKind` is a value vocabulary (individuals, never subclasses) that names the computational method by which distance or dissimilarity is calculated in a reference frame. A frame declares its metric via **`gmeow:hasMetricKind`**:

- **`metricGeodesic`** — shortest path along a curved surface (great-circle on WGS-84, celestial sphere).
- **`metricEuclidean`** — straight-line distance in Cartesian space (indoor grids, robot bases).
- **`metricCosine`** — angular proximity in a latent vector space.
- **`metricEditDistance`** — string or sequence dissimilarity (Levenshtein, Hamming).
- **`metricGraphHops`** — shortest-path edge count in a network.

The metric is **declared in the frame, computed by the solver** (Principle 12). The ontology never asserts numeric proximity values; it provides the structure for the solver to compute them.

### ProximityMeasurement — the reified relator

A **`gmeow:ProximityMeasurement`** is a `gmeow:Measurement` subclass that records distance between two entities. The flat shortcut `gmeow:proximity` links an entity to its measurement; the measurement itself carries:

- `gmeow:observedFeature` — the entity measured *from* (inherited from Observation).
- `gmeow:proximityTo` — the target entity.
- `gmeow:observationResult` → `gmeow:ScalarQuantity` — the numeric value, unit (QUDT), and reference frame.

```turtle
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
@prefix ex:    <https://example.org/loc/> .

ex:office a gmeow:Place ; gmeow:locatedAt ex:buildingA .
ex:home   a gmeow:Place ; gmeow:locatedAt ex:buildingB .

ex:commute a gmeow:ProximityMeasurement ;
    gmeow:observedFeature ex:office ;
    gmeow:proximityTo ex:home ;
    gmeow:observationResult [
        a gmeow:ScalarQuantity ;
        gmeow:quantityValue "12.4"^^xsd:decimal ;
        gmeow:hasUnit <http://qudt.org/vocab/unit/KM> ;
        gmeow:hasReferenceFrame gmeow:referenceFrameWGS84
    ] ;
    gmeow:vantage ex:commuterApp .
```

The `hasReferenceFrame` on the `ScalarQuantity` points to `referenceFrameWGS84`, whose `hasMetricKind` is `metricGeodesic`. A GeoSPARQL solver would resolve this to `geof:distance` with the WGS-84 CRS; a graph solver would use Dijkstra for `metricGraphHops`; a vector store would compute cosine similarity for `metricCosine`.

### Alignment to surface vocabularies

- **schema.org**: `gmeow:proximity` `skos:closeMatch` `schema:distance` (directional lossy: schema.org uses flat `QuantitativeValue`; GMEOW uses reified Measurement with frame-declared metric).
- **GeoSPARQL**: `geof:distance` is a function, not an assertable property. The projection layer maps `ProximityMeasurement` with `metricGeodesic`/`metricEuclidean` to the appropriate GeoSPARQL distance call pattern.
- **QUDT**: No direct counterpart for `MetricKind` or `hasMetricKind`. QUDT models units and quantity kinds, not computational distance metrics.

### Authoring Guidance: Adding a Novel Realm

To introduce a new domain (e.g. a proprietary robotic configuration space, a custom calendar, or a specialised colourspace) without modifying the core ontology classes:

1. **Declare the Frame Realm**: Create a new individual of type `gmeow:FrameRealm` (e.g., `ex:proprietaryMeasurementRealm`).
2. **Define a Reference Frame Profile**: Declare a `gmeow:ReferenceFrame` instance with complete profile descriptors (including `gmeow:frameRealm`, `gmeow:hasAxis`, `gmeow:dimensionCount`, `gmeow:frameKind`, `gmeow:requiresHost`, and `gmeow:determinacyModel`). All of these properties are required by the SHACL shapes (validated in `test_shapes.py`), so omitting `gmeow:requiresHost` or any other mandatory descriptor will cause validation to fail.
3. **Align by Reference**: Add external vocabulary mappings in your domain-specific mapping DSL file (e.g. using `skos:closeMatch` or `skos:relatedMatch` to standard terms), leaving core class definitions untouched.
