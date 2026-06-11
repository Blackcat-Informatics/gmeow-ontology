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
