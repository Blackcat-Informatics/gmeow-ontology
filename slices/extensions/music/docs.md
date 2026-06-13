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

## Consumer

- The **GTS `music-package`** single-file format.
- The **MCP analysis-claims** recall/revise surface.
- The **19-case stress corpus** that closes the EPIC.
