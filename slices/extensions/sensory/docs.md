<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Sensory — sensors, platforms, and the observation stack applied to senses

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/sensory` · **tier: extension**
> A thermometer, a smartphone, and a nose are all observers; their readings are all claims.

GMEOW does not have one model for instrument data and another for perception. This slice
deepens the `SensoryObservation` stub from the observations slice into a full sensor
stack, and in doing so applies the unified claim stance to the senses themselves: every
reading is an observation **from a vantage** (Principle 11 — values are
perceiver-relative; the vantage may be a device, a person, or an organization), and
multiple sensors observing the same property of the same entity produce **co-equal
readings that coexist without collapse** (Principle 9 — no sensor is "the primary
observer"). The Principle-15 consumer is exactly that: **sensory perception observations —
the unified stance applied to senses**. SOSA/SSN is bridged by reference, never imported
(Principle 5): `Sensor` ↔ `sosa:Sensor`, `SensorPlatform` ↔ `sosa:Platform`,
`ObservableProperty` ↔ `sosa:ObservableProperty`.

## The observers

### gmeow:Sensor

An agent that produces observations by responding to a stimulus — a physical device, a
software process, or a biological perceiver. A `gufo:RoleMixin` under `gmeow:Agent`: being
a sensor is a role, not an essence. The vantage of a `SensoryObservation` may be a Sensor,
a person, or an organization; none is privileged (Principle 9).

### gmeow:SensorPlatform

The physical carrier of sensors — a weather station, satellite, buoy, drone, or vehicle.
Deliberately distinct from both the Sensor (the observing agent it hosts) and the Place
(where it sits, via `gmeow:platformLocation`). Historical locations are modelled as past
observations or location states, never by mutating `platformLocation` (Principle 9).

## What is measured

### gmeow:ObservableProperty

The open value vocabulary (Principle 9 — individuals, never subclasses) of measurable
properties: temperature, humidity, light intensity, sound pressure level, atmospheric
pressure, air quality index, radiation level, and whatever comes next — a new property is
data, not a schema change.

### gmeow:SensoryQuantity

The scalar result of a sensory observation — value × unit × determinacy × granularity.
Declared `rdfs:subClassOf math:Quantity`: a domain specialization whose sensory provenance is
meaningful, while `math:Quantity` remains the sole quantity authority.

## The observation itself

### gmeow:sensoryProperty

Names the `ObservableProperty` a `SensoryObservation` measures. Distinct from
`observationMethod` (the procedure); one property per observation — multiple properties
mean multiple observations (Principle 9). EL-axiomatised: every `SensoryObservation`
mediates a vantage, an observed feature, a property, and a result.

### gmeow:sensoryObservationOf

The entity whose property is being observed — a subproperty of `gmeow:observedFeature`,
so generic consumers query all observations without knowing the domain. Its inverse,
`gmeow:hasSensoryObservation`, is non-functional by doctrine: competing readings coexist.

### gmeow:sensoryResult

The scalar outcome, a subproperty of `gmeow:observationResult` ranging over
`SensoryQuantity`. The reference frame is carried on the observation via
`gmeow:hasReferenceFrame` and propagates to the result through the existing
`isResultOf ∘ hasReferenceFrame` chain — frame inheritance is axiomatic, not convention.

### gmeow:hasSensoryQuantity

The flat shortcut from entity straight to quantity, **derived** by property chain:

```turtle
gmeow:hasSensoryQuantity owl:propertyChainAxiom
    ( gmeow:hasSensoryObservation gmeow:sensoryResult ) .
```

The chain keeps the property non-simple, so it is excluded from all cardinality axioms
(OWL 2 DL regularity preserved). Assert the reified path; the shortcut materialises.

## Solver layer & alignment

Unit conversion, sensor calibration, aggregation, and time-series analytics are
solver-layer concerns (Principle 12) — the graph records readings and their frames, never
recomputed derivatives. Alignment to SOSA/SSN remains by reference (Principle 5); the
pre-existing `gmeow:Agent → sosa:Sensor` broadMatch becomes precise here, with
`gmeow:Sensor` as the exact counterpart. Depends on `kernel`, `observations` (the claim
stack and `SensoryObservation` stub it deepens), and `places` (platform locations). The
sensory-environment slice consumes this stack for ambient Location×time conditions.
