<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Places — every locus names its frame

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/places` · **tier: core**
> The Location-as-reference-frame engine: one frame discipline from latitude to chromosome, currency to C-space.

A coordinate without its frame is a bare number, and GMEOW forbids bare numbers
(Principle 11). This slice carries that discipline to its logical end: **the reference frame
is the universal abstraction for "where"**. Geographic places, virtual venues, storage
buckets, celestial positions, robot poses, latent embeddings, and biological-sequence loci
are all positions in explicit `gmeow:ReferenceFrame`s, distinguished by open realm, kind,
and metric vocabularies rather than parallel class hierarchies. Granularity is a *value*,
never a subclass: country, room, star, and SNP are all first-class, QID-bearing entities
chained by containment.

Contested geography gets the standpoint treatment, not a dispute mechanism:
a Crimea-class sovereignty conflict is two `gmeow:accordingTo`-annotated
`gmeow:containedInPlace` statements; an endonym/exonym conflict is two co-equal place
namings. There is **no `preferredJurisdiction` and no `primaryName`** — competing claims
coexist, none privileged (Principle 9), and superseded ones are suppressed via
`gmeow:displayable false`, never deleted (Principle 10). Everything computational — CIDR
containment, calendar/assembly liftover, geodesic distance, transform-tree math, RCC8
closure — is solver-layer (Principle 12): the slice models structure, `gmeow:frameSolver`
names the engine.

## The location umbrella

### gmeow:Location

The umbrella Kind: a locus where an entity can be situated, reside, or occur. Structural
SubKinds exist only where the *structure* differs (network addresses vs coordinates vs
storage paths); everything else is a type value.

### gmeow:Place · gmeow:placeType

A geographic location at any granularity; the kind (country … room, ~600 GeoNames feature
codes) is the open `gmeow:placeType` value, non-functional because multi-source
classifications coexist as evidence.

### gmeow:VirtualLocation · gmeow:StorageLocation

The online venue (video conference, website, metaverse room — open
`gmeow:virtualLocationType`) and the byte locus (bucket, filesystem path, content-addressed
store — `gmeow:storageMedium`, functional because the medium is constitutive). A storage
device may sit at a geographic `gmeow:physicalPlace`; a virtual location has
`gmeow:accessUrl` as its flat shortcut.

### gmeow:CelestialLocation

Astronomical loci from spacecraft to galaxy cluster (`gmeow:celestialObjectType`), carrying
`gmeow:CelestialCoordinates` (right ascension, declination, epoch) in explicit sky frames —
ICRS, FK5, Galactic — per Principle 11.

### gmeow:BiologicalSequenceLocation

biological-sequence design: a sequence feature (gene, exon, SNP, CDS) is a locus in a versioned reference
assembly treated as a linear 1-D frame. It bears `gmeow:SequenceCoordinates` with explicit
start, end, and strand; the kind is the open `gmeow:sequenceFeatureType` (aligned by
reference to Sequence Ontology). FALDO positions map directly; liftover between assemblies is
`gmeow:transformsTo` executed by an external solver (Principle 12).

## The reference-frame engine (Principle 11)

### gmeow:ReferenceFrame

The universal "where" abstraction: a system relative to which coordinates and measurements
are expressed. Frames carry a `gmeow:frameRealm`, a `gmeow:frameKind`, a `gmeow:hasMetricKind`,
and axes; they nest via `gmeow:parentFrame` and convert via `gmeow:transformsTo`. The slice
ships ~60 seeded frames — `gmeow:referenceFrameWGS84`, `referenceFrameICRS`,
`referenceFrameGRCh38`, `referenceFrameInternet`, `referenceFrameSRGB`, currencies, calendars
— so most facts pick a frame rather than mint one.

### gmeow:FrameRealm · gmeow:FrameKind · gmeow:MetricKind

Three open value vocabularies (Principle 9). The realm is the domain
(`gmeow:frameRealmTerrestrial`, `frameRealmCelestial`, `frameRealmVirtual`,
`frameRealmMathematical`, `frameRealmRobotic`, `frameRealmBiological`, and the
`gmeow:frameRealmNarrative` the narrative extension builds on). The kind is the structure
(geodetic, Cartesian, topological, linear-sequence). The metric is how distance is computed
(`gmeow:metricGeodesic`, `metricEuclidean`, `metricCosine`, `metricEditDistance`,
`metricGraphHops`) — the computation itself is solver-layer.

### gmeow:SpatialCoordinates · gmeow:coordinateFrame · gmeow:Axis

A position in a frame. `gmeow:coordinateFrame` names that frame (a `gmeow:requiresFrame`
warning fires on a frameless coordinate); `gmeow:hasAxis` enumerates the frame's
`gmeow:Axis` individuals. The slice seeds ~150 axes (latitude, IPv6, valence, joint angles,
sequence position). High-dimensional and ∞-D spaces use one axis plus a
`gmeow:hasCoordinateMatrix` shape, not n axis individuals — the ontology carries structure,
the solver carries cardinality (Principle 12).

## The geographic 80% and its promotions

### gmeow:hasCoordinates · gmeow:hasGeometry

Flat shortcuts to a point (`gmeow:GeoCoordinates`: `gmeow:latitude`/`gmeow:longitude`/
`gmeow:elevation`) and a shape (`gmeow:Geometry`: `gmeow:asWKT`/`gmeow:asGeoJSON`). Both are
`coarsenGuarded` and resolve by property chain to the reified `gmeow:CoordinateObservation`
(via `gmeow:coordinateResult` / `gmeow:geometryResult`), which carries provenance, frame,
confidence, and standpoint when the bare value is not enough (Principle 3).

### gmeow:containedInPlace · gmeow:Geocode

`gmeow:containedInPlace` is the transitive nesting spine (room ⊂ building ⊂ city ⊂ country),
inverse `gmeow:containsPlace`; closure is solver work, not asserted triples. Alternative
geocodes (geocoding design — Plus Code, what3words, geohash, H3, MGRS, UN/LOCODE, mile-marker)
are `gmeow:Geocode` strings in an explicit `gmeow:geocodeFrame` attached by `gmeow:hasGeocode`;
conversion to WGS84 is solver-layer. `gmeow:h3` carries the H3 hierarchical hexagonal cell
index; its resolution (0-15) is homed on `gmeow:h3Resolution`, not folded into the index
string, because resolution is itself precision-bearing — `gmeow:h3Resolution` is `coarsenGuarded`
and `avoidForConsumer gmeow:consumerPublicSite`. A high (fine) resolution sits at or near
`gmeow:granularityPoint` on the spatial `gmeow:GranularityLevel` disclosure ladder; a public
projection must not emit a resolution finer than the declared disclosure granularity permits,
the H3-native analogue of coarsening `gmeow:hasCoordinates` before publication.

## Time-scoped and contested situations

### gmeow:JurisdictionTenure · gmeow:ContainmentTenure

The flat transitive containment is the 80% case; promote to these `gmeow:TimeScopedRelation`
SubKinds (time-scoped place design) when period, standpoint, or contestation matters. Contested sovereignty is
multiple co-existing, confidence-weighted, standpoint-indexed JurisdictionTenures, never a
single winner (Principle 9). Both carry their interval via `gmeow:duringInterval` (temporal
slice).

### gmeow:RegulatoryOverlay · gmeow:overlayType

Legal overlays beyond sovereignty (regulatory-overlay design/vertical/maritime/aviation-zone design) — zoning, protected areas, restricted airspace,
maritime and aviation zones, NOTAMs — as reified situations binding a place
(`gmeow:overlayPlace`), an authority (`gmeow:overlayAuthority`), an open `gmeow:overlayType`,
and an optional `gmeow:overlayRegulation` RightsStatement for the deontic rules. 3D bounds are
frame-relative ScalarQuantities in explicit vertical frames; contested overlays coexist.

### gmeow:LandTenure · gmeow:CadastralReference

The cadastral realm (cadastral design, LADM/INSPIRE): ownership, lease, easement, mortgage, usufruct as
`gmeow:LandTenure` situations binding a place, a party, a rights statement, and an open
`gmeow:tenureType`; structured identifiers (folio, title, lot, parcel ID) as
`gmeow:CadastralReference`. Contested titles coexist; lapsed tenures keep their record with
`gmeow:displayable false` (Principle 10).

## Solver layer & alignment

The slice carries structure; the solver carries computation (Principle 12). The RCC-8 region
relations (`gmeow:rcc8dc` … `gmeow:rcc8eq`) are pairwise-disjoint by axiom, but their
exhaustiveness and composition are solver work; CIDR/DNS resolution, coordinate liftover,
transitive closure, area and intersection math, and frame transforms all live below the OWL
core. Alignment is by reference across a deliberate superset — schema.org, GeoSPARQL,
GeoNames, Getty TGN, Wikidata, WGS84, BOT, ifcOWL, IVOA, UAT, SWEET, FALDO, Sequence Ontology,
ISO 19152 LADM, INSPIRE — projected through profile mappings (SSSOM rows at the alignment
window). GMEOW's addition is that every frame is first-class and self-describing rather than
an opaque IRI.

## Dependencies

Depends on `kernel`, `coreference`, `documents`, `lifecycle`, `observations`, `rights`, and
`temporal` (tenure intervals). Consumed by every located fact — contacts, events, the mail
corpus, jurisdiction tenures, the GeoSPARQL projection — and by the narrative extension, whose
narrative frames live in the `gmeow:frameRealmNarrative` defined here.
