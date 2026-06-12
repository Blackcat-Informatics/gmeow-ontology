<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# narrative

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/narrative` · **tier: extension**

Narrative reference frames and creative-work sourcing. Part of the #42 Location-as-reference-frame epic. DOCTRINE. \* A NarrativeReferenceFrame is a ReferenceFrame (canon supplies coordinate topology for in-universe locations, dates, and events) and functionally serves as the standpoint under which in-universe claims hold true, without subclassing gmeow:Standpoint (gUFO MixIden enforces exactly-one-Kind inheritance). \* CreativeWorks (BookRelease, SerialInstallment) remain out-of-universe rights-bearing artifacts; they source, witness, or revise a narrative frame via gmeow:sourceFor. \* …

*This is a STUB guide (#325 Tier-2): the slice is modelled, aligned, and
reasoned, but its narrative documentation has not been written yet. The
module-status matrix tracks the gap; term-level documentation (labels,
definitions) lives in `module.ttl` and renders via `gmeow describe`.*

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

- **Flat by default**: `narrates` (segment → diegetic content) and
  `narratedIn` (content → segment; Wikidata **P1441**'s edge — SSSOM row
  deferred to the alignment window), both `⊑ narrationLink` (domain- and
  range-free ancestor for media-specific seams: panel, shot, verse).
  **No `owl:inverseOf`** between the orientations — EL-clean; query both
  (the `connectsTo` convention).
- **Reify with a reason**: `NarrationUsage` (the NameUsage/DepictionUsage
  idiom) = segment × subject × mode(s), SHACL-required mode — the modeless
  case is the flat shortcut. `NarrationMode` is open: direct, mentioned,
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

- **Arc samples (#361)** — the music `PitchTrajectory` move: an arc is
  sampled control points in a frame, not prose. `ArcSample ⊑ Observation`
  reads {subject × `NarrativePosition` (#359) × state-by-IRI × vantage};
  the state is a *soft* cross-slice reference (affect `EmotionType` when
  loaded, EmotionML category otherwise — IRI, never dependency, P16).
  The existing `CharacterArc` is the integrating whole
  (`arcSample ⊑ hasPart`, purely additive); two analyzers disagreeing at
  one position are two coexisting cells, surfaced — never resolved — by
  `narrative-arc-trajectory.rq` (the syuzhet-CSV primitive).
- **Roles (#362)** — a narratological role is a *function relative to a
  scope*, never a property of the character: `RoleInNarrative` =
  bearer × `NarrativeScope` (named umbrella grafted over CreativeWork /
  ContentSegment / NarrativeReferenceFrame, extension-side — the
  `Rule ⊑ Norm` direction) × open `NarrativeRole` value. Protagonist of
  the trilogy, of one book, of one chapter are different claims; ensemble
  works carry coexisting protagonist relators and **no `primaryProtagonist`
  exists or ever will** (P9). Character-as-exemplar composes from the
  rubrics facility's `exemplarSubject` (#353) with zero new machinery.
- **Motifs (#363)** — thematic tags with *identity*: `Motif ⊑ SocialObject`
  with aliases (names module), an open `MotifKind`, and occurrences riding
  the narration seam (`motifOccursIn ⊑ narratedIn`, inheriting the #360
  flat-by-default efficiency discipline). Cross-work identity goes through
  `counterpartOf` or a registry anchor (Thompson Motif Index / ATU /
  DBTropes — alignment window), never string equality. **Tag vs Motif
  boundary**: tags are uncurated labels (tags module); promote to Motif
  when recurrence and identity emerge — the promotion is an Activity with
  provenance, and the corpus heuristic (concepts → always; thematic_tags →
  on recurrence + curator confirmation) gates in the #364 consumer child.
