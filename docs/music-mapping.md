<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Music mapping and design rationale

GMEOW Music treats musical content as **frame-relative** and every notation as a **declared-loss projection**. This document is the analogue of `languages-mapping.md` for the music extension: it explains the five-layer doctrine, the alignment surface, and the deliberate departures from common practice.

## The five-layer doctrine

| Layer | Canonical construct | What it replaces |
|---|---|---|
| **Pitch** | `TuningSystem` as a `ReferenceFrame`; exact rational `PitchValue`; `PitchCollection` / `PitchSpelling` as projections | Fixed letter-name pitch classes; enharmonic collapse |
| **Time** | `MusicalTimeFrame`; `TimeMapping` / `TempoMap`; `MetricStructure`, `MeterAssignment`, `MetricModulation`; `GrooveProfile` | A single global meter + single BPM number |
| **Structure** | `MusicalSegment` graph; `ToneEvent`; `Voice`; `SegmentTransformation` | Note-event tables; implicit part-of hierarchy |
| **Performance** | `DegreeOfFreedom`; `TraversalConstraint`; `PerformanceDecision`; `GenerativeProcess`; `OrnamentProfile` | Indeterminacy as missing data; mobile form as prose |
| **Analysis** | `MusicAnalysisClaim` as standpoint-indexed observations against explicit theory frames | Analysis as ground truth; genre as tag string |

Every value is relative to an explicit reference frame (Principle 11). A pitch without its `TuningSystem` is ill-formed; a duration without its `MusicalTimeFrame` is ill-formed; a notation render without its `NotationProjectionProfile` and declared losses is incomplete.

## What's deliberately non-standard

| Common assumption | GMEOW model | Rationale |
|---|---|---|
| Pitch is a letter name (C♯4) | Pitch is a frame-relative `PitchValue`; letter names are `PitchSpelling` projections | Enharmonic ambiguity becomes co-equal spellings; microtones, JI, and spectral tunings are first-class |
| Tempo is a BPM number | Tempo is a `TimeMapping` between a musical frame and clock time; `TempoMap` is piecewise and per-voice | Tempo canons, metric modulation, and polymeter are native |
| Meter is a single time signature | `MetricStructure` groups `MetricGroup`s; concurrent `MeterAssignment`s on different voices = polymeter | 7/8 over 4/4 is data, not a special case |
| A work is its score | A score is a `ScoreEdition` — one `Manifestation` projected from canonical frame-relative content | CMN, mensural, tablature, graphic, JI, MEI, MusicXML, MIDI are all renders |
| Indeterminacy is absent data | `DegreeOfFreedom` positively declares fixed / constrained / free / delegated status | Cage's 4′33″ is fully specified by what it determines |
| Genre is a tag string | `Genre` is a `Kind` with derivation lineage; attribution is a standpoint-indexed claim | "math rock" can be asserted and refuted from different vantages |
| Analysis is ground truth | `MusicAnalysisClaim` carries analyst, theory frame, confidence, and displayable | Two analysts in the same frame can disagree; one analyst in two frames is not contradicting herself |
| A note event is the atom | `ToneEvent` is one kind of `MusicalSegment`; continuous pitch is a `PitchTrajectory` | Glissandi, gamaka, and UPIC curves are first-class |

## Projection layer

Every music notation is a directional, lossy projection of canonical content. The canonical object is a graph of `MusicalSegment`s carrying `PitchValue`s in explicit `TuningSystem`s and durations in explicit `MusicalTimeFrame`s. A staff score, a MIDI file, a MusicXML export, and a LilyPond engraving are all renders — none is the work itself (Principles 4, 11, 12).

The generic projection framework (`NotationProjectionProfile`, `ProjectionLoss`) lives in the core `slices/core/notation/` slice. The music slice provides the music-domain `NotationSystem` individuals and the `MusicalParameter`-specific losses. See `slices/extensions/music/docs.md` § Notation projection layer for the full loss table and external alignments.

## External alignment

The music extension bridges by reference (Principle 5) to:

- **Music Ontology** (`mo:`) — `MusicalWork` closeMatch.
- **Polifonia PON** — primary linkage hub for work/expression metadata and JAMS segment annotations.
- **LRMoo 1.0** — Work/Expression/Performance via the crmarchaeo EDOAL pattern.
- **Wikidata** — QID anchors for tuning systems, notation systems, composers, and works; MusicBrainz MBID PIDs.
- **MusicBrainz** — by MBID only.
- **OMRAS2 chord ontology** — chord-symbol `closeMatch`.
- **schema.org** — `MusicComposition`, `MusicRecording`, `MusicAlbum` lossy projections.
- **MEI, MusicXML, MIDI, ABC, Humdrum \*\*kern, LilyPond, Scala .scl** — one projection profile and one FnO function per format.

No external axioms are imported. Alignment is by reference only.

## Stress corpus

The 19-case stress corpus in `slices/extensions/music/fixtures/` exercises every layer and edge case:

| # | Fixture | Exercises |
|---|---|---|
| 1 | Ferneyhough-style excerpt | nested rational `TimeMapping`s, fractional meter, per-`Voice` `TempoMap`s, notation-saturation round-trip |
| 2 | Nancarrow tempo canon | symbolic √2:2 `TimeMapping` between `Voice` frames |
| 3 | Cage 4′33″ | complete `DegreeOfFreedom` profile |
| 4 | Stockhausen *Klavierstück XI* | fragment graph + `TraversalConstraint` + two documented traversals |
| 5 | Partch 43-tone excerpt | JI `TuningSystem`, integer-pair ratios, JI spelling |
| 6 | Xenakis glissando field | `PitchTrajectory`s + stochastic `GenerativeProcess` + UPIC graphic notation |
| 7 | Grisey *Partiels* opening | `Spectrum` → derived `PitchCollection` → staff projection with declared loss |
| 8 | Cardew *Treatise* page | graphic `Manifestation` canonical; standpointed symbolic interpretations |
| 9 | Ars subtilior excerpt + isorhythmic motet | mensural notation profile; talea/color unequal cycles |
| 10 | Messiaen excerpt | added-value `MetricGroup`s; non-retrogradable retrograde-identity claim; mode of limited transposition |
| 11 | Lutosławski ad-lib section | unsynchronized `Voice` spans bounded by cue anchors |
| 12a | Raga Yaman ālāp | score-less `Work`, collection+roles+`OrnamentProfile`, gharana `VersionSet`, transmission lineage |
| 12b | Maqam Rast taqsim | ordered ajnas composition, quarter-tone-ish intervals |
| 12c | Gamelan piece | slendro hosted by instrument-set `Item`, colotomic cycle |
| 13 | Aksak folk tune | additive 2+2+3 `MetricGroup`s, changing meters |
| 14 | Math rock track | polymeter over shared `TempoMap`, contested bar-17 meter pair, riff transformation graph, drop-D `InstrumentConfiguration`, refuted genre claim |
| 15 | Carter / Don Caballero metric modulation | pivot-equivalence frame transition |
| 16 | Riff-form track | the form as a `SegmentTransformation` graph |
| 17 | Dilla-feel groove | `GrooveProfile` + measured microtiming `Observation`s (drummer + MIR vantages) |
| 18 | Prepared-piano piece | `InstrumentConfiguration` + `PlayingTechnique`s |
| 19 | Reich *Piano Phase* | `GenerativeProcess` + realizations `wasGeneratedBy` |

The corpus is gated by `queries/competency/music.rq` (15 competency questions with expected bindings) and by the Docker-backed reasoning case that proves the fixtures stay coherent under broad disjointness.

## Consumers

- The **GTS `music-package`** single-file format (`src/gmeow_tools/ext/music/`).
- The **MCP analysis-claims** recall/revise surface.
- The **19-case stress corpus** itself, as the Principle 15 consumer proof for the entire music design.
