<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Narrative — canon as a reference frame, the text as a projection of the story

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/narrative` · **tier: extension**
> In-universe facts hold *accordingTo* a frame; the chapter sequence is the syuzhet projection of frame-relative story content.

A narrative is two things GMEOW keeps rigorously apart. A **canon** is a coordinate system:
locations, dates, and events inside the Harry Potter world, Earth-616, or the Star Wars
Legends split are positioned relative to a `gmeow:NarrativeReferenceFrame`, which is a
`gmeow:ReferenceFrame` (the #42 Location-as-reference-frame epic) living in the
`gmeow:frameRealmNarrative` defined by the places slice. The frame *functionally serves* as
the standpoint under which in-universe claims hold true, without subclassing
`gmeow:Standpoint` — gUFO's MixIden enforces exactly-one-Kind inheritance, so the frame
*is* the canon and `gmeow:accordingTo` does the standpoint work. Competing continuities are
separate frames that may sharpen one another; **no single canon wins** (Principle 9), variants
are linked by `gmeow:counterpartOf` and never merged (Principle 5), and a retcon is a
frame-scoped claim revision preserved by `gmeow:supersedes` + `gmeow:displayable false`
(Principle 10), never a deletion.

The second thing is the **text**, and the load-bearing doctrine of this slice is that **the
text is not the story**. A `gmeow:CreativeWork` (BookRelease, SerialInstallment) is an
out-of-universe, rights-bearing artifact that *sources, witnesses, or revises* a frame — it
is never the canon itself. Story content is frame-relative; the chapter sequence is a
*syuzhet projection* of it; the narration seam relates the two without confusing them; and
every interpretive reading — what the protagonist is, what a character felt in chapter 31,
whether a string is a motif — is a vantage-indexed claim coexisting with its rivals
(Principle 9). The sections below build that stance from frame to seam to interior.

## Frames, sourcing, and myth

### gmeow:NarrativeReferenceFrame

The canon, continuity, or narrative realm — a `gmeow:ReferenceFrame` SubKind that
coordinatizes in-universe locations, dates, and events and serves as the standpoint for
in-universe claims. Ordinary GMEOW entities (Person, Place, Event) carry claims
`gmeow:accordingTo` it; cross-continuity variants link by `gmeow:counterpartOf`.

### gmeow:sourceFor · gmeow:NarrativeFrameLink

`gmeow:sourceFor` (⊑ `gmeow:contributesToFrame`) relates a creative work to the frame it
sources, witnesses, or revises — the out-of-universe → in-universe edge. Frame-to-frame
relations (canon, alternate continuity, expanded universe, fanon, crossover, adaptation —
the open `gmeow:NarrativeFrameRelation` vocabulary) ride the flat shortcuts
`gmeow:hasNarrativeFrameRelation` / `gmeow:relatesToFrame`; promote to the reified
`gmeow:NarrativeFrameLink` (source × target × relation) when provenance, confidence, or scope
must be a node.

### gmeow:Myth

A socially-sustained narrative whose currency is independent of its truth-value — a founding
myth, an urban legend, a debunked claim that persists (issue #214, Deception EPIC #212).
GMEOW asserts no truth verdict (Principle 1); in-myth claims are scoped `gmeow:accordingTo`
the `gmeow:mythFrame`. Tellings attach via `gmeow:hasMythTelling`, spread along
`gmeow:propagatesFrom` (⊑ `prov:wasDerivedFrom`, reusing the lineage spine — Principle 4),
and the propagating agent bears `gmeow:roleDupe` or `gmeow:roleDeceiver` (the deception
bridge).

## Narrative time (#359, EPIC #358)

**The text is not the story: the chapter sequence is the syuzhet projection
of frame-relative story content.** Two kinds of `NarrativeTimeFrame` (each a
`ReferenceFrame` under `frameRealmNarrative`) coordinatize narrative
position:

- **discourse time** (`axisDiscourseTime`, syuzhet) — the order of telling;
  owned by the telling work via `discourseTimeOf` (a re-segmented edition is
  a different frame);
- **story time** (`axisStoryTime`, fabula) — the order of happening
  in-universe; owned by a `NarrativeReferenceFrame` via `storyTimeOf`
  (a continuity can reorder it — a retcon is a frame-scoped remapping,
  preserved by suppression).

### gmeow:NarrativePosition

`NarrativePosition` is the narrative analogue of `SpatialCoordinates`:
frame (mandatory — no bare chapter indices), ordinal, label, or both.
`atNarrativePosition` is the single domain-free anchor that the depiction
seam (#360), arc samples (#361), and motif occurrences (#363) all reuse —
deliberately non-functional, because one diegetic event holding positions in
*both* frames whose orders disagree **is** the flashback, queryable instead
of contradictory (Principle 9).

In-universe Allen relations are claims `accordingTo` the narrative frame via
the statement layer, never global facts. Position comparison, ordering, and
discourse↔story reconciliation are solver-layer (Principle 12). A reified
discourse↔story mapping construct is deliberately deferred to coordinate
with the music extension's `TimeMapping` (#306) — one frame-mapping idiom in
the repo, not two.

## The narration seam (#360, EPIC #358)

NOnt's reference function between text and story — neither mereology nor
participation: "chapter 31 *narrates* event E; character C is *narrated in*
segment S."

### gmeow:narrates · gmeow:narratedIn

- **Flat by default**: `narrates` (segment → diegetic content) and
  `narratedIn` (content → segment; Wikidata **P1441**'s edge — SSSOM row
  deferred to the alignment window), both `⊑ narrationLink` (domain- and
  range-free ancestor for media-specific seams: panel, shot, verse).
  **No `owl:inverseOf`** between the orientations — EL-clean; query both
  (the `connectsTo` convention).

### gmeow:NarrationUsage · gmeow:NarrationMode

- **Reify with a reason**: `NarrationUsage` (the NameUsage/DepictionUsage
  idiom) = segment × subject × mode(s), SHACL-required mode — the modeless
  case is the flat shortcut. It pairs with the flat `narrates` via
  `gmeow:pairsWith`. `NarrationMode` is open: direct, mentioned,
  flashback (the #359 two-axes disagreement as a mode), dream, hypothetical,
  unreliable (the #212 boundary — narrator-level held ≠ projected;
  documented bridge, no axiom coupling).
- **Naming note**: `gmeow:depicts` belongs to the image spine
  (MediaObject → Entity, `⊑ isAbout`, documents module) and `isAbout`'s
  Entity range cannot carry diegetic *events* (occurrents) — hence the
  separate, range-free narration family.
- **The efficiency doctrine, codified**: the foundation corpus carries
  38,413 character-segment links + 23,962 narrated events + 12,354
  appearances. Budget arithmetic: flat ≈ 1 quad/link; reified ≈ 6–8
  statements/link; full reification ≈ 1.5–2M statements for one corpus.
  Flat is the default; **silent full reification is a defect, not
  thoroughness**. The consumer child (#364) gates the flat/reified split
  against a declared budget.

Diegetic events stay ordinary `gmeow:Event`s claims-scoped `accordingTo`
their frame; the events module's Participation machinery is reused untouched
for who-did-what *in* the story.

## The narrative interior (#361 / #362 / #363, EPIC #358)

### gmeow:ArcSample

- **Arc samples (#361)** — the music `PitchTrajectory` move: an arc is
  sampled control points in a frame, not prose. `ArcSample ⊑ Observation`
  reads {subject × `NarrativePosition` (#359) × state-by-IRI × vantage};
  the state is a *soft* cross-slice reference (affect `EmotionType` when
  loaded, EmotionML category otherwise — IRI, never dependency, P16).
  The existing `CharacterArc` is the integrating whole
  (`arcSample ⊑ hasPart`, purely additive); two analyzers disagreeing at
  one position are two coexisting cells, surfaced — never resolved — by
  `narrative-arc-trajectory.rq` (the syuzhet-CSV primitive).

### gmeow:RoleInNarrative · gmeow:hasNarrativeRole

- **Roles (#362)** — a narratological role is a *function relative to a
  scope*, never a property of the character: `RoleInNarrative` =
  bearer × `NarrativeScope` (named umbrella grafted over CreativeWork /
  ContentSegment / NarrativeReferenceFrame, extension-side — the
  `Rule ⊑ Norm` direction) × open `NarrativeRole` value. The flat shortcut
  `hasNarrativeRole` pairs with the relator via `gmeow:pairsWith`.
  Protagonist of the trilogy, of one book, of one chapter are different
  claims; ensemble works carry coexisting protagonist relators and **no
  `primaryProtagonist` exists or ever will** (P9). Character-as-exemplar
  composes from the rubrics facility's `exemplarSubject` (#353) with zero new
  machinery.

### gmeow:Motif · gmeow:motifOccursIn

- **Motifs (#363)** — thematic tags with *identity*: `Motif ⊑ SocialObject`
  with aliases (names module), an open `MotifKind`, and occurrences riding
  the narration seam (`motifOccursIn ⊑ narratedIn`, pairing with
  `NarrationUsage` via `gmeow:pairsWith` and inheriting the #360
  flat-by-default efficiency discipline). Cross-work identity goes through
  `counterpartOf` or a registry anchor (Thompson Motif Index / ATU /
  DBTropes — alignment window), never string equality. **Tag vs Motif
  boundary**: tags are uncurated labels (tags module); promote to Motif
  when recurrence and identity emerge — the promotion is an Activity with
  provenance, and the corpus heuristic (concepts → always; thematic_tags →
  on recurrence + curator confirmation) gates in the #364 consumer child.

## Solver layer & deferred alignment

Position comparison, discourse↔story reconciliation, arc-trajectory CSVs, and motif
recurrence analysis are all solver-layer (Principle 12); the slice carries the frames,
positions, and seams those queries read. Alignment is by reference — Wikidata P1441 for the
narration seam, the Thompson Motif Index / ATU / DBTropes for motifs — with SSSOM rows and
the reified discourse↔story `TimeMapping` (shared with the music extension #306) both
deferred to the alignment window so the repo grows one frame-mapping idiom, not two. No axiom
here references a deception-slice or norms-slice IRI: the unreliable-narration boundary and
the `NarrativeScope` graft are documented bridges and extension-side grafts (P16), never
core edits.

## Dependencies

Depends on `kernel`, `documents` (CreativeWork, ContentSegment, the image spine), `places`
(ReferenceFrame and `gmeow:frameRealmNarrative`), `provenance` (the propagation spine),
`observations` (ArcSample ⊑ Observation), and `events` (diegetic events and Participation).
Consumed by the deception slice's worked examples and by the lbox foundation-docs importer
(EPIC #358: 16,445 arc samples, 779 role links, 16,002 tags / 669 concepts) ahead of the #364
consumer child.
