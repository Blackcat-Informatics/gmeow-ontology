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

## Consumer

- The **GTS `music-package`** single-file format.
- The **MCP analysis-claims** recall/revise surface.
- The **19-case stress corpus** that closes the EPIC.
