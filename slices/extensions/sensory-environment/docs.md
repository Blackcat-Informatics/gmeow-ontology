<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Sensory Environment — ambient conditions, measured and felt

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/sensory-environment` · **tier: extension**
> What a room is like at 3 p.m. — to the instruments, and to each person in it.

"The room is 22 °C" and "the room feels stuffy" are different *kinds* of claim, and this
slice refuses to flatten them into one field. Ambient perceivable conditions at a
Location×time are reified as a first-class `SensoryEnvironment` (not a property bag), so
that everything said about it carries provenance, confidence, temporal scope, and
standpoint. **Measured** conditions are `CoordinateMatrix` values in measurement reference
frames — colourspace, audio spectrum, thermal, air-quality. **Perceived** conditions are
standpoint-indexed values in `MentalReferenceFrame`s: every perception is read in the
perceiver's own frame (Principle 11 — frames are first-class and self-describing, and a
mental frame *requires a host*; two hosts may map the same stimulus to different
coordinates). The two facets are co-equal, with no privileged representation
(Principle 9). Part of the the design Location-as-reference-frame design; the Principle-15
consumer is **environmental sensing context for sensory observations** — the sensory
slice's stack gains its where-and-what-it-was-like setting here.

## The environment and its facets

### gmeow:SensoryEnvironment

The ensemble of physical properties perceivable at one location and time. Anchored by
`gmeow:environmentAtLocation` (functional — one location is constitutive of identity) and
scoped by `gmeow:environmentAtInstant` (point-like) or `gmeow:environmentDuringInterval`
(spans) — pick one, never both for the same scope.

### gmeow:hasMeasuredCondition

The objective facet: an instrument's reading as a `CoordinateMatrix` in a measurement
frame. Non-functional by doctrine — multiple instruments produce competing measurements
that coexist rather than collapse (Principle 9).

### gmeow:hasPerceivedCondition

The subjective facet: a perceiver's `SensoryPerception` in a `MentalReferenceFrame`.
Equally non-functional, and equally first-class — the felt temperature is not a degraded
version of the measured one.

### gmeow:sensoryModality

The channel(s) through which the environment is characterised — values from
`gmeow:SensoryModality`, an open vocabulary (visual, auditory, olfactory, gustatory,
tactile, thermal, air quality). Non-functional: one space may be both thermally and
acoustically characterised.

## The measured side

### gmeow:CoordinateMatrix

The generalisation of `math:Quantity` to vectors, matrices, and tensors: a serialised
`gmeow:matrixValue` literal, a `gmeow:matrixShape` descriptor ("4×1", "256×1",
"640×480×3"), a unit, a determinacy, and exactly one frame via
`gmeow:coordinateMatrixFrame`. Used for colourspace tuples, audio spectra, thermal images,
and compound air-quality readings. All matrix algebra — dot products, transforms,
decompositions — lives in the solver layer (Principle 12).

### gmeow:coordinateMatrixFrame

Functional sub-property of `gmeow:hasReferenceFrame`: a matrix is expressed in exactly one
frame, and frame transformation is a computation, never an assertion. Being a sub-property
means the frame-inheritance chain (`isResultOf ∘ hasReferenceFrame`) applies
automatically. Seed measurement frames ship with the slice: CIE 1931 XYZ, CIE L\*a\*b\*,
and an audio-spectrum frame.

## The perceived side

### gmeow:MentalReferenceFrame

A `ReferenceFrame` whose realm is perceptual or psychological: thermal comfort (ASHRAE
PMV/PPD), the Russell affective circumplex, Gärdenfors conceptual spaces, egocentric and
allocentric cognitive maps, imagined spaces (memory palaces, dreams). Axiomatised to
require a host (`gmeow:isHostedBy someValuesFrom gmeow:Entity`); the frame deactivates
when its host ceases to exist. This is Principle 11 at full strength — subjective value
spaces get the same first-class frame machinery as CIE XYZ.

### gmeow:SensoryPerception

A `StandpointClaim` specialised to the sensory domain: vantage = perceiver, observedFeature
= the environment (via `gmeow:perceptionEnvironment`, a sub-property of `observedFeature`),
result = the perceived value, channel = `gmeow:perceptionModality`. Competing perceptions
coexist without collapse (Principle 9); superseded ones are suppressed, never deleted
(Principle 10).

## Solver layer & status

Colourspace conversion, spectral analysis, comfort-model evaluation, and matrix parsing
(shape-directed via `matrixShape`) are solver-layer concerns (Principle 12) — the graph
states which frame a value is in; it never stores re-derived coordinates. The slice is
flagged **putative** in its module header: alignments beyond the in-house frame facility
(e.g. to SOSA observation context or W3C SSN-System) are deferred until the
Location-as-reference-frame design settles. Depends on `kernel`, `observations`,
`places`, and `temporal`.
