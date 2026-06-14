# GMEOW Music Slice

Frame-relative musical content; every notation is a lossy projection.

This slice is the first full-scale Principle 16 extension. It scaffolds the
five-layer music model described in the #306 EPIC:

1. **Pitch** — `TuningSystem` as a `ReferenceFrame`; exact rational `PitchValue`;
   `PitchCollection` and `PitchSpelling` as projections.
2. **Time** — `MusicalTimeFrame`; `TimeMapping` and `TempoMap`; `MetricStructure`,
   `MeterAssignment`, `MetricModulation`; `GrooveProfile`.
3. **Structure** — `MusicalSegment` graph; `ToneEvent`; `Voice`; `SegmentTransformation`.
4. **Performance** — `DegreeOfFreedom`; `TraversalConstraint`; `PerformanceDecision`;
   `GenerativeProcess`; `OrnamentProfile`.
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

## Performance: form, process, and indeterminacy (issue #312)

### gmeow:DegreeOfFreedom

A `gmeow:DegreeOfFreedom` is a relator that positively declares how one parameter
of a `MusicalWork` or `Expression` is determined: fixed, constrained, free, or
delegated to performer, environment, or process. Indeterminacy is not an absence;
it is a declared status (Principles 9, 11, 12). Cage's 4′33″ is fully specified
by a set of such cells: duration constrained, tacet fixed, sound content
delegated to the environment, instrumentation free.

### gmeow:TraversalConstraint

A `gmeow:TraversalConstraint` is mobile form as data: fragments, allowed
successor links (`gmeow:mayFollow`), selection rules, and termination rules.
Graph reachability and termination are solver work; no transitive or chain axiom
is ever asserted over `gmeow:mayFollow` (Principle 12). Stockhausen's
*Klavierstück XI* is the seed fixture.

### gmeow:PerformanceDecision

A `gmeow:PerformanceDecision` records one documented traversal of a mobile form
during a performance. Competing traversals of the same work coexist as distinct
relators (Principle 9).

### gmeow:GenerativeProcess

A `gmeow:GenerativeProcess` is musical content that is itself a process —
phasing, stochastic distribution, verbal score, rule set, or algorithm. The
human-readable rule text is canonical; formal realization is delegated to a
solver referenced by `gmeow:processFunction` (Principle 12). Seed fixtures:
Reich-style phasing and Xenakis-style stochastic processes.

### gmeow:OrnamentProfile

A `gmeow:OrnamentProfile` is a named convention for ornamentation — a gamaka
family, baroque agrément, jazz turn — bound to a `MusicalSegment` or `Voice`. It
separates structural pitch membership from expressive execution, completing the
raga/maqam model (Principles 9, 11). Seed fixture: Raga Yaman gamaka profile.

### gmeow:mayFollow

`gmeow:mayFollow` is a directed allowed-successor link between mobile-form
fragments. It is plain data; reachability is computed by
`gmeow:fnTraverseMobileForm`, never reasoned by the DL engine.

## Performance: events and participation (issue #313)

### Event types and the no-subclass doctrine

There is **no `gmeow:MusicalPerformance` class**. A performance is an ordinary
`gmeow:Event` whose `gmeow:eventType` carries one or more musical values:
`musicalPerformance`, `concert`, `recordingSession`, `take`, `overdub`,
`rehearsal`, `jamSession`, `soundcheck`, `DJSet`, and `transmission` (the oral-
tradition teaching event used by M10). Live vs studio is not a type split; it is
`eventType × eventLocation` (Principles 6, 9, 11).

### gmeow:performanceOf

`gmeow:performanceOf` links an event to the `CreativeWork` it performs — usually
an `Expression` interpreting a known version, or a `Work` directly for an
improvised or oral rendition with no fixed mediating Expression. It is
non-functional: a medley performs several works, and a work is performed by many
events.

### gmeow:PerformanceParticipation

`gmeow:PerformanceParticipation` is a `gufo:SubKind` of the core
`gmeow:Participation` relator. It adds music-specific attributes to the universal
participation pattern:

- `participationInstrument` — the kind of instrument (`InstrumentType`).
- `participationInstrumentItem` — the specific physical instrument item.
- `participationConfiguration` — the configured instrument setup (`InstrumentConfiguration`, stub for #314).
- `participationPart` — the musical part performed (open range).
- `participationTechnique` — the playing technique (`PlayingTechnique`).

Playing bass on take 3 is event involvement modelled as one
`PerformanceParticipation`, not a third relator (Principle 4). The SHACL shape
advises ≤1 instrument per participation — mint one participation per instrument.

### Credit derivation

A `Contribution` on the resulting `Recording` is derived from the
`PerformanceParticipation` cells by a documented FnO projection rule
(`gmeow:fnParticipationToContribution`), never an OWL property chain (Principle
12). Full cataloguing of the rule lands with the projection toolchain (#319).

### Session micro-fixture

`fixtureSessionEvent` → 3 take events (`fixtureSessionTake1Event` …
`fixtureSessionTake3Event`) → an overdub event → a composite `Recording`. Each
take event generates a `Recording`; the composite `wasDerivedFrom` the take
recordings. The fixture demonstrates the “who played what on take 3” competency
query via `PerformanceParticipation`.

## Consumer

- The **GTS `music-package`** single-file format.
- The **MCP analysis-claims** recall/revise surface.
- The **19-case stress corpus** that closes the EPIC.
