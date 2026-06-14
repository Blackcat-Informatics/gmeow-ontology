# GMEOW Music Slice

Frame-relative musical content; every notation is a lossy projection.

This slice is the first full-scale Principle 16 extension. It scaffolds the
five-layer music model described in the #306 EPIC:

1. **Pitch** — `TuningSystem` as a `ReferenceFrame`; exact rational `PitchValue`;
   `PitchCollection` and `PitchSpelling` as projections.
2. **Time** — `MusicalTimeFrame`; `TimeMapping` and `TempoMap`; `MetricStructure`,
   `MeterAssignment`, `MetricModulation`; `GrooveProfile`.
3. **Structure** — `MusicalSegment` graph; `ToneEvent`; `Voice`; `SegmentTransformation`.
4. **Performance** — `DegreeOfFreedom`; `TraversalConstraint`; `GenerativeProcess`;
   `PerformanceParticipation`; `InstrumentConfiguration`.
5. **Analysis** — `MusicAnalysisClaim` as standpoint-indexed observations against
   explicit theory frames.

The structural foundation issue (#307) lands only the extension scaffold and the
universal core touch-points (`MusicalWork`, `Recording`, `ScoreEdition`,
`CreativeDerivation`, `Genre`, `RealizationMode`, and the role/format seeds) in
`slices/core/creative-works/module.ttl`. Child issues fill the layers above.

## Pitch collections and spelling (issue #309)

Pitch collections (`gmeow:PitchCollection`) are categorised by a single
`gmeow:PitchCollectionKind` value — scale, mode, maqam, jins, raga, thaat,
pathet, mode of limited transposition, pitch-class set, row/series, or spectrum
collection — rather than by subclassing (Principle 9).

Membership is a reified relator (`gmeow:PitchCollectionMembership`) binding a
collection, a `PitchValue`, a `CollectionMemberRole`, and an optional context.
Contested memberships — e.g. the size of the Rast third — coexist as relators
carrying distinct `gmeow:accordingTo` annotations.

A maqam is composed of ordered ajnas via the universal `gmeow:hasPart` plus a
local `gmeow:collectionPartOrder` property on each jins. A raga is a collection
plus member roles such as `vādī` and `samvādī`.

Pitch spellings (`gmeow:PitchSpelling`) are relators binding a `PitchValue`, a
`PitchSpellingSystem`, and a spelled name string. Note names (C♯4, sargam Ga,
Johnston +7) are projections of frame-relative pitch, not canonical values;
enharmonic ambiguity is modelled as two co-equal spellings of the same pitch.

Seed fixtures: Rast maqam (ordered ajnas in 24-EDO), Raga Yaman (member roles in
12-EDO), Messiaen's whole-tone mode of limited transposition, the pitch-class
set `[0,2,7]`, and co-equal C♯4 / D♭4 spellings.

## Musical time (issue #310)

A `gmeow:MusicalTimeFrame` is the time-layer analogue of `TuningSystem`: a
reference frame that anchors musical events and defines what it means for one
instant or span to precede, contain, or align with another. A
`gmeow:MusicalTimeSpan` is a concrete interval within that frame, described by a
rational start position and a rational duration.

`gmeow:TimeMapping` relates two `ReferenceFrame`s — typically a musical frame
(measure, beat, tatum) and another musical or clock-time frame. Each mapping
carries a `gmeow:timeMappingKind` from the value vocabulary: tuplet, tempo
canon, tempo map, or unsynchronized ad-lib. It references a solving function
from the FnO catalogue for any arithmetic that the ontology does not assert as
triples (Principle 12). A `gmeow:TempoMap` is a time-ordered piece-wise mapping
composed of `TempoMapSegment`s; each segment carries its own ratio data, and
segments may abut or overlap when multiple tempi coexist (metric modulation,
metric polyphony).

`gmeow:MetricStructure` groups `gmeow:MetricGroup`s. A `MetricGroup` is a
regular pulse layer (e.g. quarter-note beat, eighth-note subdivision) and is
instantiated by `gmeow:MeterAssignment`s that bind a meter signature to a span.
`gmeow:MetricModulation` records a deliberate equivalence between two pulse
durations across a boundary, making tempo changes derivable rather than
mysterious. `gmeow:GrooveProfile` captures expressive timing and dynamics
(e.g. swing ratio, lay-back) relative to a metric reference; it is a standpoint-
relative projection, not a literal tempo-map override.

The layer distinguishes **polymeter** from **polyrhythm** without inventing new
primitives. In polymeter, concurrent `MeterAssignment`s on different carriers
coexist over one shared tempo context. In polyrhythm, concurrent `TimeMapping`
ratios over one span encode differing pulse relations that periodically align.
Both coexist within a `MusicalTimeFrame`.

Seed fixtures: a 5/8 → 7/8 → 4/4 sequence, 7/8-over-4/4 polymeter, nested 5:4
and 3:2 tuplets, a √2:2 mensuration canon, a swing groove, and a Carter-style
metric modulation.

## Structure graph (issue #311)

A `gmeow:MusicalSegment` is the single structural node for musical content at
any granularity: riff, motif, phrase, section, fragment, talea, color, drone,
loop, or tone-event container. Granularity is a `gmeow:segmentKind` value —
there are no subclasses per granularity (Principle 9). Containment rides the
universal `gmeow:hasPart` / `gmeow:partOf` spine; placement in musical time is
declared with `gmeow:segmentSpan` pointing to a `MusicalTimeSpan`.

`gmeow:ToneEvent` is the one structural subkind of `MusicalSegment` — an atomic
sounding unit. Its pitch content is exactly one of a `PitchValue`, a
`PitchTrajectory`, or the unpitched flag. Dynamics and articulation are symbolic
value shortcuts; measured dB and timbre analysis are standpointed Observations
or M11 sensory claims.

`gmeow:PitchTrajectory` models continuous pitch: glissandi, gamaka, UPIC curves.
It owns ordered `PitchTrajectoryControlPoint`s, each carrying a
`PitchValue` and a rational position inside a `MusicalTimeFrame`, plus an
`interpolationKind`. The actual curve evaluation is solver work (Principle 12).

`gmeow:Voice` is a continuity strand that binds segments and may host its own
`MusicalTimeFrame`, `TuningSystem`, and `MetricStructure`. It is the carrier for
polymeter, tempo canons, and per-voice frames.

`gmeow:SegmentTransformation` is a relator `{source × target × type × parameter}`
and the AI-analysis backbone. A transformation may be asserted by composer text
or by an analysis standpoint through the statement layer (Principles 2, 9, 14).

**Note-level scale doctrine.** Bulk `ToneEvent` data for real pieces (millions
of events) lives in GTS `music-package` bundles, not the reasoned core. The
ontology gates the TBox and seed fixtures; package bundles carry the instance
payload (Principles 8, 12, 13).

Seed fixtures: riff A → transposed riff A′ → re-accented riff A″ transformation
chain; a C4 `ToneEvent`; a two-point C4→G4 glissando trajectory; and a bass
`Voice`.

## Consumer

- The **GTS `music-package`** single-file format.
- The **MCP analysis-claims** recall/revise surface.
- The **19-case stress corpus** that closes the EPIC.
