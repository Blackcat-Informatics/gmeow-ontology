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
   matching; the names module — the structured replacement for the retired
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

## Indoor topology: BOT and ifcOWL

GMEOW's existing place model already handles sites, buildings, floors, and rooms at the
**value** level (`gmeow:placeTypeSite`, `gmeow:placeTypeBuilding`,
`gmeow:placeTypeFloor`, `gmeow:placeTypeRoom`). No new classes are introduced.
The **BOT** (Building Topology Ontology) projection maps this value vocabulary
to BOT classes and properties:

| GMEOW | BOT |
|---|---|
| `gmeow:placeTypeSite` | `bot:Site` |
| `gmeow:placeTypeBuilding` | `bot:Building` |
| `gmeow:placeTypeFloor` | `bot:Storey` |
| `gmeow:placeTypeRoom` | `bot:Space` |
| `gmeow:containsPlace` (building → floor, floor → room) | `bot:hasStorey` / `bot:hasSpace` |
| `gmeow:containsPlace` (site → building, building → floor, floor → room) | `bot:containsZone` |
| `gmeow:adjacentTo` | `bot:adjacentZone` |

The alignment is **by reference**; BOT is imported offline as a validation-time target
snapshot (`imports/targets/bot.ttl`, BSD-3-Clause). The BOT profile emits a BOT
topology graph without leaking `gmeow:` IRIs.

**ifcOWL** (IFC4) is a reference-only target with a more restrictive license.
Term equivalences (`skos:closeMatch`) map GMEOW place kinds and topology properties
to ifcOWL classes such as `ifc:IfcBuilding`, `ifc:IfcBuildingStorey`, and
`ifc:IfcSpace`, but no executable SPARQL projection is generated — consumers should
use a licensed IFC solver to derive an ifcOWL serialization.

```turtle
ex:site a gmeow:Place ; gmeow:placeType gmeow:placeTypeSite ;
        gmeow:containsPlace ex:building .

ex:building a gmeow:Place ; gmeow:placeType gmeow:placeTypeBuilding ;
            gmeow:containsPlace ex:floor .

ex:floor a gmeow:Place ; gmeow:placeType gmeow:placeTypeFloor ;
         gmeow:containsPlace ex:roomA ; gmeow:containsPlace ex:roomB .

ex:roomA a gmeow:Place ; gmeow:placeType gmeow:placeTypeRoom ;
         gmeow:adjacentTo ex:roomB .
```

Projected through the `bot` profile this yields:

```turtle
ex:site a bot:Site ; bot:containsZone ex:building .
ex:building a bot:Building ; bot:hasStorey ex:floor .
ex:floor a bot:Storey ; bot:hasSpace ex:roomA , ex:roomB .
ex:roomA a bot:Space ; bot:adjacentZone ex:roomB .
```

## Privacy by generalization: coarsening coordinates

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
access/consent *trigger* on the same control is PRIV-GEN.

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

These concrete frame profiles are instances of the reusable **`gmeow:Profile`** meta-pattern defined in `ontology/modules/profiles.ttl`: a Profile is a closed descriptor schema whose values are drawn from open, extensible value vocabularies, with self-description and a novel-value guard.

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

## Distance, Proximity, and Frame-Declared Metrics

`gmeow:MetricKind` is a value vocabulary (individuals, never subclasses) that names the computational method by which distance or dissimilarity is calculated in a reference frame. A frame declares its metric via **`gmeow:hasMetricKind`**:

- **`metricGeodesic`** — shortest path along a curved surface (great-circle on WGS-84, celestial sphere).
- **`metricEuclidean`** — straight-line distance in Cartesian space (indoor grids, robot bases).
- **`metricCosine`** — angular proximity in a latent vector space.
- **`metricPositionalDistance`** — absolute difference between coordinates on a linear 1-D frame (base pairs, residues).
- **`metricEditDistance`** — string or sequence dissimilarity (Levenshtein, Hamming).
- **`metricGraphHops`** — shortest-path edge count in a network.

The metric is **declared in the frame, computed by the solver** (Principle 12). The ontology never asserts numeric proximity values; it provides the structure for the solver to compute them.

### ProximityMeasurement — the reified relator

A **`gmeow:ProximityMeasurement`** is a `gmeow:Measurement` subclass that records distance between two entities. The flat shortcut `gmeow:proximity` links an entity to its measurement; the measurement itself carries:

- `gmeow:observedFeature` — the entity measured *from* (inherited from Observation).
- `gmeow:proximityTo` — the target entity.
- `gmeow:observationResult` → `math:Quantity` — the numeric value, unit (QUDT), and reference frame.

```turtle
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix math:  <https://blackcatinformatics.ca/math/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
@prefix ex:    <https://example.org/loc/> .

ex:office a gmeow:Place ; gmeow:locatedAt ex:buildingA .
ex:home   a gmeow:Place ; gmeow:locatedAt ex:buildingB .

ex:commute a gmeow:ProximityMeasurement ;
    gmeow:observedFeature ex:office ;
    gmeow:proximityTo ex:home ;
    gmeow:observationResult [
        a math:Quantity ;
        math:hasDimension math:lengthDimension ;
        math:quantityValue "12.4"^^xsd:decimal ;
        gmeow:unit <http://qudt.org/vocab/unit/KM> ;
        gmeow:hasReferenceFrame gmeow:referenceFrameWGS84
    ] ;
    gmeow:vantage ex:commuterApp .
```

The `hasReferenceFrame` on the `math:Quantity` points to `referenceFrameWGS84`, whose `hasMetricKind` is `metricGeodesic`. A GeoSPARQL solver would resolve this to `geof:distance` with the WGS-84 CRS; a graph solver would use Dijkstra for `metricGraphHops`; a vector store would compute cosine similarity for `metricCosine`.

### Alignment to surface vocabularies

- **schema.org**: `gmeow:proximity` `skos:closeMatch` `schema:distance` (directional lossy: schema.org uses flat `QuantitativeValue`; GMEOW uses reified Measurement with frame-declared metric).
- **GeoSPARQL**: `geof:distance` is a function, not an assertable property. The projection layer maps `ProximityMeasurement` with `metricGeodesic`/`metricEuclidean` to the appropriate GeoSPARQL distance call pattern.
- **QUDT**: No direct counterpart for `MetricKind` or `hasMetricKind`. QUDT models units and quantity kinds, not computational distance metrics.

### Authoring Guidance: Adding a Novel Realm

To introduce a new domain (e.g. a proprietary robotic configuration space, a custom calendar, or a specialised colourspace) without modifying the core ontology classes:

1. **Declare the Frame Realm**: Create a new individual of type `gmeow:FrameRealm` (e.g., `ex:proprietaryMeasurementRealm`).
2. **Define a Reference Frame Profile**: Declare a `gmeow:ReferenceFrame` instance with complete profile descriptors (including `gmeow:frameRealm`, `gmeow:hasAxis`, `gmeow:dimensionCount`, `gmeow:frameKind`, `gmeow:requiresHost`, and `gmeow:determinacyModel`). All of these properties are required by the SHACL shapes (validated in `test_shapes.py`), so omitting `gmeow:requiresHost` or any other mandatory descriptor will cause validation to fail.
3. **Align by Reference**: Add external vocabulary mappings in your domain-specific mapping DSL file (e.g. using `skos:closeMatch` or `skos:relatedMatch` to standard terms), leaving core class definitions untouched.

## Pose: position + orientation

GMEOW represents a full 6-DOF pose as a compound object — **position** and **orientation** are peers, neither is privileged:

- **`gmeow:Pose`** — a frame-relative position + orientation; not a subclass of `SpatialCoordinates`, but composed of them.
- **`gmeow:hasPosePosition`** → `gmeow:SpatialCoordinates` — the translational component, reusing the existing coordinate model.
- **`gmeow:hasPoseOrientation`** → `gmeow:Orientation` — the rotational component.
- **`gmeow:poseFrame`** — sub-property of `gmeow:hasReferenceFrame`; the frame in which both position and orientation are expressed.

`gmeow:Orientation` is representation-agnostic: a single orientation may carry **co-equal** facets (quaternion, Euler angles, compass angles, or a homogeneous matrix). No form wins:

```turtle
ex:dronePose a gmeow:Pose ;
    gmeow:poseFrame ex:wgs84Frame ;
    gmeow:hasPosePosition ex:dronePosition ;
    gmeow:hasPoseOrientation ex:droneOrientation .

ex:droneOrientation a gmeow:Orientation ;
    gmeow:quaternionX 0.0 ; gmeow:quaternionY 0.0 ;
    gmeow:quaternionZ 0.70710678 ; gmeow:quaternionW 0.70710678 .
```

The SHACL `PoseShape` enforces exactly one position, one orientation, and one frame per pose. The `OrientationShape` accepts any of: a complete quaternion (`quaternionX/Y/Z/W`), complete Euler angles with `eulerOrder` (`yaw/pitch/roll` + order), a `heading`, a `bearing`, or a `hasCoordinateMatrix` homogeneous transform.

### External alignment

- **IEEE 1872-2015 CORA/POS**: `gmeow:Pose` ↔ `pos:QuantitativePose`; `gmeow:Orientation` ↔ `pos:OrientationMeasure`; `gmeow:hasPose`/`hasPosePosition`/`hasPoseOrientation` ↔ `pos:pose`/`pos:posePosition`/`pos:poseOrientation`.
- **Wikidata**: `gmeow:Pose` → `wd:Q1055020`; quaternion properties → `wd:Q462283`; Euler properties → `wd:Q465493`; `heading` → `wd:Q41154`; `bearing` → `wd:Q123429`.
- **OGC GeoPose 1.0**: structurally compatible (frame + position + orientation), but no RDF terms exist yet — the projection layer will add a GeoPose JSON profile in a later parcel.

### Projection behaviour

GeoSPARQL 1.0/1.1 has no native pose model, so the `geosparql` profile projects **only the translational component** of a pose to a WKT POINT. Orientation is an intentional, documented lossy drop (`fnPosePositionToWktPoint`). Heading/bearing may be projected separately once a GeoPose JSON profile is added. The existing `Place` point projection (`mapGeoPoint`) continues to work independently.

## Spatial Aggregation and Privacy-Preserving Statistics

GMEOW models spatial aggregation as a reified `gmeow:SpatialAggregation` — a `gmeow:Measurement` specialisation that summarises entities located within a `gmeow:Place`. The aggregation function (`gmeow:aggregationFunction`) is a value vocabulary (`gmeow:AggregationFunction`) with seeds for count, sum, average, density, centroid, minimum, and maximum. The aggregation region is the `gmeow:observedFeature`; the result is a `math:Quantity`. The actual arithmetic is performed by the solver layer (Principle 12), never materialised as asserted triples.

For k-anonymity, a `gmeow:minimumPopulation` datatype property on `SpatialAggregation` declares the minimum population size (k) required for disclosure. A result failing this check is suppressed at projection time (coarsen or withhold, Principle 10), never deleted.

The flat shortcut `gmeow:hasCentroid` on `gmeow:Place` provides the geometric centroid directly; the full relator form is a `SpatialAggregation` with `aggregationFunction` `aggCentroid`, carrying provenance, frame, and solver reference.

```turtle
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix math:  <https://blackcatinformatics.ca/math/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
@prefix ex:    <https://example.org/loc/> .

ex:cityCensus a gmeow:SpatialAggregation ;
    gmeow:observedFeature ex:metropolis ;
    gmeow:aggregationFunction gmeow:aggCount ;
    gmeow:observationResult [
        a math:Quantity ;
        math:hasDimension math:dimensionless ;
        math:quantityValue "15000"^^xsd:decimal ;
        gmeow:unit <http://qudt.org/vocab/unit/UNITLESS> ;
    ] ;
    gmeow:minimumPopulation 5 ;
    gmeow:vantage ex:censusBureau .
```

**Alignments:**

- **GeoSPARQL**: `gmeow:hasCentroid` → `geo:hasCentroid`; `gmeow:containsPlace` → `geo:sfContains`.
- **RDF Data Cube**: `gmeow:SpatialAggregation` → `qb:Observation`; `gmeow:AggregationFunction` → `qb:MeasureProperty`.

## Time-scoped jurisdiction and containment

A place's **sovereignty** and **parent containment** are time-scoped, contested, and historically varying. GMEOW models them as reified `gmeow:TimeScopedRelation` subtypes:

- **`gmeow:JurisdictionTenure`** — place × governing polity × interval. Non-functional on the place: contested sovereignty (Crimea-class) is *multiple co-existing tenures*, each standpoint-indexed, never a single winner (Principle 9).
- **`gmeow:ContainmentTenure`** — child place × parent place × interval. Records border changes and re-organisations (a region that moved from one country to another in 1954).

Both carry their interval via `gmeow:duringInterval` (inherited from `gmeow:TimeScopedRelation` in the temporal module). The plain transitive `gmeow:containedInPlace` remains the flat 80%-case shortcut.

```turtle
ex:crimea a gmeow:Place ; gmeow:placeType gmeow:placeTypeRegion .
ex:russia a gmeow:Place ; gmeow:placeType gmeow:placeTypeCountry .
ex:ukraine a gmeow:Place ; gmeow:placeType gmeow:placeTypeCountry .

ex:jurisdictionRU a gmeow:JurisdictionTenure ;
    gmeow:jurisdictionPlace ex:crimea ;
    gmeow:jurisdictionPolity ex:russia ;
    gmeow:jurisdictionDeterminacy gmeow:determinacyDisputed ;
    gmeow:duringInterval [ gmeow:startedAtTime "2014-03-18T00:00:00Z"^^xsd:dateTime ] .

ex:jurisdictionUA a gmeow:JurisdictionTenure ;
    gmeow:jurisdictionPlace ex:crimea ;
    gmeow:jurisdictionPolity ex:ukraine ;
    gmeow:jurisdictionDeterminacy gmeow:determinacyDisputed ;
    gmeow:duringInterval [ gmeow:startedAtTime "1991-08-24T00:00:00Z"^^xsd:dateTime ] .
```

## Geometry type and GeoJSON

Every `gmeow:Geometry` may declare its structural kind via `gmeow:geometryType` (a value vocabulary aligned to GeoSPARQL simple-features) and may carry both WKT (`gmeow:asWKT`) and GeoJSON (`gmeow:asGeoJSON`) serializations:

| GMEOW | GeoSPARQL |
|---|---|
| `gmeow:geometryTypePoint` | `sf:Point` |
| `gmeow:geometryTypeLineString` | `sf:LineString` |
| `gmeow:geometryTypePolygon` | `sf:Polygon` |
| `gmeow:geometryTypeMultiPoint` | `sf:MultiPoint` |
| `gmeow:geometryTypeMultiLineString` | `sf:MultiLineString` |
| `gmeow:geometryTypeMultiPolygon` | `sf:MultiPolygon` |

Both `asWKT` and `asGeoJSON` have OWL range `rdfs:Literal` (DL-safe); data may tag the literal with `^^geo:wktLiteral` or `^^geo:geoJSONLiteral` respectively.

```turtle
ex:cityBoundary a gmeow:Geometry ;
    gmeow:geometryType gmeow:geometryTypePolygon ;
    gmeow:asWKT "POLYGON((...))"^^geo:wktLiteral ;
    gmeow:asGeoJSON '{"type":"Polygon","coordinates":[[...]]}' ;
    gmeow:geometryDeterminacy gmeow:determinacyVague .
```

## Place determinacy and lifecycle

- **`gmeow:placeDeterminacy`** — the ontic determinacy of a place's existence or boundary (crisp, vague, disputed).
- **`gmeow:geometryDeterminacy`** — the ontic determinacy of a geometry's boundary.
- **`gmeow:placeSupersededBy` / `gmeow:placeSupersedes`** — sub-properties of the universal `supersededBy` / `supersedes` for place-to-place succession (Constantinople → Istanbul, merged municipalities). The superseded place is retained with `gmeow:displayable false` (Principle 10), never deleted.

## External alignment (maximal bridging)

| GMEOW term | External alignment |
|---|---|
| `gmeow:JurisdictionTenure` | `crm:E4_Period` (CIDOC-CRM), `wd:Q19517` (Wikidata: sovereignty) |
| `gmeow:ContainmentTenure` | `wdt:P131` (Wikidata: located in admin entity) |
| `gmeow:PlaceNaming` | `crm:E41_Appellation` (CIDOC-CRM, time-spanned) |
| `gmeow:GeometryType` values | `sf:Point`, `sf:LineString`, `sf:Polygon`, … (GeoSPARQL simple-features) |
| `gmeow:asGeoJSON` | `geo:asGeoJSON` (GeoSPARQL) |
| `gmeow:placeSupersedes` | `wdt:P1365` (Wikidata: replaces) |
| `gmeow:placeSupersededBy` | `wdt:P1366` (Wikidata: replaced by) |
| `gmeow:Place` | `pleiades:Place`, `whg:Place`, `lgdo:Place` |
| `gmeow:hasPlaceName` | `pleiades:Name` |
| `gmeow:authorityLink` | `whg:closeMatch` |

---

## Regulatory Overlays

GMEOW models legal and regulatory spatial overlays as reified **`gmeow:RegulatoryOverlay`** instances — time-scoped situations (⊑ `gmeow:TimeScopedRelation`) that bind a **place**, an **authority**, a **regulation type**, and optional **deontic rules** (`gmeow:RightsStatement`). This covers zoning, protected areas, restricted airspace, sanctions, tax/electoral districts, postal zones, civil-time zones, fishing zones, and customs zones.

### Distinction from JurisdictionTenure

- **`gmeow:JurisdictionTenure`** — governance / sovereignty (who rules the place).
- **`gmeow:RegulatoryOverlay`** — specific regulations applied over a place (what rules apply, and where).

A place may have multiple overlays of different types simultaneously (a national park within a tax district within a civil-time zone), and contested overlays (disputed EEZ, overlapping indigenous claims vs. state zoning) coexist as standpoint-indexed instances, never a single winner (Principle 9).

### Structure

- **`gmeow:RegulatoryOverlay`** — the reified situation.
- **`gmeow:overlayPlace`** → `gmeow:Place` — the geographic area (functional).
- **`gmeow:overlayAuthority`** → `gmeow:Agent` — the body that imposed it (functional).
- **`gmeow:overlayType`** → `gmeow:RegulatoryOverlayType` — the kind of overlay (non-functional, open value vocabulary).
- **`gmeow:overlayRegulation`** → `gmeow:RightsStatement` — the deontic rules that govern activity within the overlay.
- **`gmeow:overlayDeterminacy`** → `gmeow:Determinacy` — crisp, vague, fuzzy, or disputed boundary.
- **`gmeow:overlayLowerBound` / `gmeow:overlayUpperBound`** → `math:Quantity` — 3D bounds (altitude, depth, elevation) with QUDT unit and reference frame (Principle 11). Optional for 2D overlays.
- **`gmeow:duringInterval`** → `gmeow:TimeInterval` — the period during which the overlay is in force (inherited from `TimeScopedRelation`).

### Value vocabulary: `RegulatoryOverlayType`

An open value vocabulary (individuals, never subclasses):

| Individual | Meaning |
|---|---|
| `overlayTypeZoning` | Land-use regulation (residential, commercial, industrial, agricultural, mixed-use) |
| `overlayTypeProtectedArea` | National park, wildlife reserve, marine protected area (IUCN categories Ia–VI are additional classifications, not subclasses) |
| `overlayTypeRestrictedAirspace` | Prohibited, restricted, danger area, TRA, TSA, no-fly zone |
| `overlayTypeSanctions` | Embargoed / sanctioned territory |
| `overlayTypeTaxDistrict` | Tax assessment / collection district |
| `overlayTypeElectoralDistrict` | Voting / constituency boundary |
| `overlayTypePostalZone` | Postal code / ZIP delivery area |
| `overlayTypeCivilTimeZone` | Civil time zone boundary (authority-decreed, not merely astronomical) |
| `overlayTypeFishingZone` | EEZ / fisheries management zone (UNCLOS Article 55) |
| `overlayTypeCustomsZone` | Customs territory, free-trade zone, bonded warehouse area |

### 3D bounds and frame-relativity

Airspace and maritime overlays often have vertical limits. These are **not** asserted as raw numbers on the overlay; they are `math:Quantity` values carrying:

- `math:hasDimension math:lengthDimension` — the bound's physical dimension.
- `math:quantityValue` — the numeric bound.
- `gmeow:unit` — the QUDT unit (metres, feet, flight levels).
- `gmeow:hasReferenceFrame` — the reference frame (e.g. WGS-84 for altitude above MSL, a local datum for depth below chart datum).

```turtle
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix math:  <https://blackcatinformatics.ca/math/> .
@prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .
@prefix ex:    <https://example.org/loc/> .

ex:noFlyZone a gmeow:RegulatoryOverlay ;
    gmeow:overlayPlace ex:airportApproach ;
    gmeow:overlayAuthority ex:caa ;
    gmeow:overlayType gmeow:overlayTypeRestrictedAirspace ;
    gmeow:overlayLowerBound [ a math:Quantity ; math:hasDimension math:lengthDimension ; math:quantityValue "0"^^xsd:decimal ; gmeow:unit <http://qudt.org/vocab/unit/M> ; gmeow:hasReferenceFrame gmeow:referenceFrameWGS84 ] ;
    gmeow:overlayUpperBound [ a math:Quantity ; math:hasDimension math:lengthDimension ; math:quantityValue "3000"^^xsd:decimal ; gmeow:unit <http://qudt.org/vocab/unit/M> ; gmeow:hasReferenceFrame gmeow:referenceFrameWGS84 ] ;
    gmeow:duringInterval ex:intervalOngoing .
```

The 2D/3D polygon of the overlay's footprint lives on the `gmeow:Place` via `hasGeometry`; the vertical bounds live on the `RegulatoryOverlay`. This separation keeps geometry in one place and makes frame-relativity explicit (Principle 11).

### External alignment (maximal bridging)

| GMEOW term | External alignment |
|---|---|
| `gmeow:RegulatoryOverlay` | `schema:AdministrativeArea` (partial, lossy), `wd:Q1055894` (administrative territorial entity) |
| `gmeow:overlayTypeZoning` | `wd:Q160730` (zoning) |
| `gmeow:overlayTypeProtectedArea` | `wd:Q1041938` (protected area); IUCN management categories (Ia–VI) by reference |
| `gmeow:overlayTypeRestrictedAirspace` | `wd:Q2574180` (no-fly zone); AIXM airspace types by reference |
| `gmeow:overlayTypeSanctions` | `wd:Q1540287` (sanctions) |
| `gmeow:overlayTypeElectoralDistrict` | `wd:Q171441` (electoral district) |
| `gmeow:overlayTypePostalZone` | `wd:Q37447` (postal code) |
| `gmeow:overlayTypeCivilTimeZone` | `time:TimeZone` (OWL-Time), `wd:Q12868` (time zone) |
| `gmeow:overlayTypeFishingZone` | `wd:Q1285733` (exclusive economic zone) |
| `gmeow:overlayTypeCustomsZone` | `wd:Q783930` (customs territory) |
| `gmeow:overlayAuthority` | `schema:organizer` (lossy directional projection) |
| `gmeow:overlayRegulation` | `schema:legislation` (lossy directional projection) |

---

## Accessibility

GMEOW models accessibility as a **cross-cutting facet layer** over locations and routes. A location can have **features** (positive facilitators) and **barriers** (negative impediments) for any accessibility dimension; an entity can declare **needs** for the same dimensions. There is no privileged or primary dimension — wheelchair, step-free, visual, auditory, cognitive, clearance, and life-support are orthogonal, co-equal facets (Principle 9).

### Facet model

- **`gmeow:AccessibilityFacet`** — the dimension (wheelchair, step-free, visual, auditory, cognitive, clearance, life-support). An open value vocabulary: individuals, never subclasses.
- **`gmeow:hasAccessibilityFeature`** — flat shortcut: a location positively provides a facet.
- **`gmeow:hasBarrier`** — flat shortcut: a location impedes a facet.
- **`gmeow:hasAccessibilityNeed`** — flat shortcut: an entity requires a facet.

A location MAY simultaneously carry **both** `hasAccessibilityFeature` and `hasBarrier` for the **same** facet (e.g. a ramp at the front entrance and stairs at the side). The two properties are declared `owl:propertyDisjointWith` to enforce conceptual separation — a single triple cannot be both.

### Reified assertions (provenance + suppression)

When the claim itself must be a node (provenance, confidence, temporal scope, or retraction), promote to **`gmeow:AccessibilityAssertion`** — a `gufo:Relator` with:

- `assertionSubject` — the location or connection being assessed.
- `assertionFacet` — the AccessibilityFacet.
- `assertionPolarity` — `polarityFeature`, `polarityBarrier`, or `polarityLimited`.

A retracted or disputed assertion sets `gmeow:displayable false` — suppressed from projection, never deleted (Principle 10).

### Accessible routes

An **accessible route** is a `gmeow:Route` of kind `routeKindAccessible`. The actual path is computed by the **solver layer** (Principle 12): it filters `gmeow:spatiallyConnectsTo` and locations to exclude any that have a `gmeow:hasBarrier` for a needed facet. The OWL core does not assert route triples.

### External alignment

- **schema.org**: `hasAccessibilityFeature` → `schema:accessibilityFeature` (lossy: GMEOW emits typed facet IRIs; schema.org expects specific text tokens). `hasBarrier` → `schema:accessibilityHazard`.
- **WHO ICF**: Each AccessibilityFacet bridges to ICF categories by reference (e.g. `facetWheelchair` ↔ ICF d4 Mobility + e115 Products for personal mobility). Alignment is by reference only — GMEOW never imports the ICF ontology.
- **OSM**: Conceptual alignment documented in SSSOM comments. OSM tags (`wheelchair=yes/no/limited`, `ramp:wheelchair`, `tactile_paving`, `step_count`) map directionally to facet assertions. No stable RDF ontology exists for OSM tags, so no executable SPARQL projection is generated.
- **IMDF / IndoorGML**: Conceptual alignment to IMDF accessibility attributes and IndoorGML barrier constructs. No stable RDF IRIs exist for most terms, so alignment uses `skos:relatedMatch` to documentation URLs.

---

## Celestial Locations

GMEOW models astronomical and space-domain locations as **`gmeow:CelestialLocation`** — a fourth structural kind under `gmeow:Location`, parallel to `Place`, `VirtualLocation`, and `StorageLocation`. Celestial locations are not geographic: they live on the celestial sphere, in the solar system, or in deep space, and their coordinates are expressed as right ascension, declination, and epoch rather than latitude and longitude.

### Structure

- **`gmeow:CelestialLocation`** — an astronomical object or position: a star, galaxy, nebula, planet, asteroid, comet, spacecraft, star cluster, etc. The specific kind is given by `gmeow:celestialObjectType` (a value vocabulary), not a subclass, so any astronomical object from star to galaxy cluster can be a first-class entity with its own coordinates and external identifiers (SIMBAD, NED, VizieR).
- **`gmeow:CelestialCoordinates`** — a point on the celestial sphere expressed as right ascension, declination, and an optional epoch. The reference frame (ICRS, FK5, Galactic, etc.) is declared via `gmeow:coordinateFrame` (Principle 11: frame-relativity).
- **`gmeow:hasCelestialCoordinates`** — links a celestial location to its coordinates.
- **`gmeow:rightAscension`** / **`gmeow:declination`** — in degrees, frame-relative.
- **`gmeow:celestialEpoch`** — the Julian year for which the coordinates are expressed (e.g. `2000.0` for J2000.0), a frame-relative scalar.

### Reference Frame Model for Celestial Coordinates

The existing `gmeow:ReferenceFrame` Profile pattern (§ Reference Frame Profiles & Extensibility) is extended with two new descriptors for celestial frames:

- **`gmeow:hasReferencePosition`** — the origin of the frame: topocentric (observatory site), geocentric, barycentric (solar system barycenter), or heliocentric. Values are `gmeow:CelestialReferenceOrigin` individuals, aligned to IVOA `refposition`.
- **`gmeow:hasTimeScale`** — the time standard: UTC, TT (Terrestrial Time), TDB (Barycentric Dynamical Time), or GPS time. Uses existing `gmeow:TimeScale` individuals from the temporal module, aligned to IVOA `timescale`.

Seed celestial reference frames:

| Frame | Axes | Realm | Ref Position | Timescale |
|---|---|---|---|---|
| `referenceFrameCelestialEquatorial` | RA, Dec | celestial | geocentric | UTC |
| `referenceFrameICRS` | RA, Dec | celestial | barycentric | TDB |
| `referenceFrameFK5` | RA, Dec | celestial | geocentric | UTC |
| `referenceFrameGalactic` | l, b | celestial | barycentric | TDB |

Coordinate transforms (precession, proper-motion propagation, frame conversion) are **solver-layer computations** (Principle 12), never asserted as triples in the OWL core.

### External Alignment (maximal bridging)

| GMEOW term | External alignment |
|---|---|
| `gmeow:CelestialLocation` | IVOA `object-type:astronomical-object`, UAT `astronomical-objects` |
| `gmeow:CelestialCoordinates` | SWEET `reprSpaceReference:CelestialCoordinate` (reference-only) |
| `gmeow:referenceFrameICRS` | IVOA `refframe:ICRS` |
| `gmeow:referenceFrameFK5` | IVOA `refframe:FK5` |
| `gmeow:referenceFrameGalactic` | IVOA `refframe:galactic` |
| `gmeow:refOriginTopocentric` | IVOA `refposition:TOPOCENTER` |
| `gmeow:refOriginGeocentric` | IVOA `refposition:GEOCENTER` |
| `gmeow:refOriginBarycentric` | IVOA `refposition:BARYCENTER` |
| `gmeow:refOriginHeliocentric` | IVOA `refposition:HELIOCENTER` |
| `gmeow:timeScaleUTC` | IVOA `timescale:UTC` |
| `gmeow:timeScaleTT` | IVOA `timescale:TT` |
| `gmeow:timeScaleTDB` | IVOA `timescale:TDB` |
| `gmeow:timeScaleGPS` | IVOA `timescale:GPS` |
| `gmeow:timeScaleTAI` | IVOA `timescale:TAI` |
| `gmeow:timeScaleUT1` | IVOA `timescale:UT1` |
| `gmeow:celestialObjectTypeStar` | IVOA `object-type:star`, SIMBAD `*` |
| `gmeow:celestialObjectTypeGalaxy` | IVOA `object-type:galaxy`, SIMBAD `G` |
| `gmeow:celestialObjectTypePlanet` | IVOA `object-type:planet`, UAT `planets` |

**SLSO** (Spacecraft Location System Ontology): No stable published RDF IRIs were found at the time of authoring. Alignment is documented as a placeholder pending stable term publication, following the precedent used for sources without resolvable RDF terms (e.g. OSM tags, IMDF accessibility attributes).
