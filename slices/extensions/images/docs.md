<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Images — contextual depiction, regions, and scene graphs

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/images` · **tier: extension**
> Who is shown, where in the pixels, and how the parts relate — without a "primary photo".

Most schemas reduce imagery to a `photo` URL field. GMEOW treats depiction the way it
treats naming: as a **contextual, attributed, time-scoped claim**. The flat
`gmeow:depicts` shortcut (a subproperty of `gmeow:isAbout`, domain `MediaObject`) covers
the 80 % case; when context, audience, period, or authority must be recorded, promote to
the **DepictionUsage** relator — the exact mirror of `NameUsage`. That pairing is
machine-readable: the module asserts `gmeow:depicts gmeow:pairsWith gmeow:DepictionUsage`,
so tooling knows the flat form and the reified form are the same fact at two fidelities.
There is **no primary or preferred depiction** (Principle 9): co-equal depictions coexist,
and a superseded one is suppressed with `gmeow:displayable false`, never deleted
(Principle 10). The slice builds on the WEMI spine — a visual work is a
`gmeow:Work`, the file a `gmeow:Manifestation`/`MediaObject` — and exists for its
Principle-15 consumer: **high-fidelity depiction (regions, scene graphs) atop the core
MediaObject, driven by the `gmeow-image` CLI (CLI roll-up design)**.

## Layer 1 — contextual depiction

### gmeow:DepictionUsage

The reified, context-dependent depiction relator — an `Observation` in the universal claim
stack. Mint one per *(entity, image, context)* tuple: `depictionSubject` (functional),
`depictionImage` (functional), `depictionContext`, plus optional `depictionAudience` and
`depictionInterval`. Perspectival, never global; paired with the flat `gmeow:depicts` via
`gmeow:pairsWith`.

### gmeow:depictionAuthority

The `gmeow:Agent` that confers or sanctions the depiction in this use — a photographer, an
archive, a self-asserting subject. Deliberately **non-functional**: joint or competing
authorities coexist with no privileged claimant (Principle 9), each attributable and
confidence-weighted.

### gmeow:DepictionContext

The open value vocabulary (Principle 9 — individuals, never subclasses) for *how* an image
shows its subject: work, family, childhood, portrait, candid, self-portrait, event, and so
on. A new context is data, not a schema change.

## Layer 2 — region encoding

### gmeow:ImageRegion

A structural part of an image — the visual counterpart of `ContentSegment`: a face, a
bounding box, a pixel mask. Belongs to exactly one image (`gmeow:regionOf`, functional;
inverse `gmeow:hasRegion`) and carries a human-readable `gmeow:regionLabel`.

### gmeow:RegionSelector

The encoding descriptor for a region's boundaries: one `gmeow:selectorType` plus the raw
`gmeow:selectorValue` literal. The canonical superset of W3C Web Annotation selectors,
IIIF selectors, and domain mask formats. Attached via `gmeow:regionSelector` (functional —
one selector per region).

### gmeow:SelectorType

Open value vocabulary of encoding kinds: SVG path, pixel rectangle, fractional rectangle,
polygon, RLE, COCO RLE, DICOM-SEG, pixel mask, Web Annotation fragment. Parsing and
coordinate conversion are **solver-layer work** (Principle 12) — the graph records which
encoding was used, never re-derives geometry.

## Layer 4 — scene graphs

### gmeow:SceneGraphEdge

A reified spatial or semantic relationship between two regions — a `gufo:Relator`
mediating `sceneSubject` and `sceneObject` (both functional, both `ImageRegion`), a
`sceneRelation`, and an explicit `gmeow:sceneConfidence` decimal in [0.0, 1.0]. Aligns by
reference to Visual Genome and OVSR relation strings.

### gmeow:SceneRelationType

The open relation vocabulary for edges — `leftOf`, `inside`, `holding`, `wearing`,
`riding`, `sameAs` (co-reference), `partOf` (mereological), and friends. The relation is a
**value, never a subclass** (Principle 9): a new predicate from a new detector is a fresh
individual.

## Solver layer & projections

The slice models *which* facts hold; computation lives outside the graph (Principle 12):
mask parsing, coordinate-space conversion, scene-graph inference, and IIIF Presentation
API 3 rendering (Layer 3 of the design is *purely projection* — Canvas/Annotation mapping
adds no ontology terms). Technical metadata (`pixelWidth`, `captureDevice`, colourspace
via `gmeow:hasReferenceFrame`) lives on `MediaObject` in the documents slice; rights reuse
the rights facility; provenance reuses `wasGeneratedBy`/`wasDerivedFrom`.

## Alignment & dependencies

All alignments are by reference, never import (Principle 5): schema.org `ImageObject`,
IIIF, W3C Web Annotation, W3C EXIF Ontology, XMP, IPTC, and CIDOC-CRM/CRMdig. Depends on
`kernel`, `documents` (MediaObject), `observations` (the claim stack DepictionUsage sits
in), and `temporal` (depiction intervals).

## EXIF tags (project homepage and language)

### gmeow:ExifTag · gmeow:hasExifTag · gmeow:exifTagId · gmeow:exifTagValue

A camera's EXIF metadata is an **open, camera-defined** set of tag/value pairs. To carry
it **losslessly** — every tag survives an `index.ttl → GMEOW → index.ttl` round-trip —
each tag is a reified `gmeow:ExifTag` in the `schema:PropertyValue` shape:
`gmeow:exifTagId` (the tag, "FNumber" → `schema:propertyID`), `gmeow:exifTagValue`
("f/2.0" → `schema:value`), and `rdfs:label` ("Aperture" → `schema:name`). Never a fixed
property per tag (P9). The meaningful facts (capture device/time, GPS, colourspace) also
ride their typed properties; the ExifTag set is the complete, faithful carrier, projected
to `schema:exifData`.

## Logo ( follow-through)

### gmeow:hasLogo

`gmeow:hasLogo` links an entity to the **media object that serves as its logo** — its
emblematic, identity-bearing image. An organization, software project, dataset, place, or
product may carry one (domain `gmeow:Entity`, range `gmeow:MediaObject`). It is
**deliberately distinct from `gmeow:depicts`**: a logo *represents* the entity rather than
*picturing* it (the gmeow-logo.svg does not depict the ontology), so `hasLogo` is **not** a
subproperty of `gmeow:isAbout` — mirroring why `schema:logo` is separate from `schema:image`
upstream. Non-functional: light/dark, wordmark/glyph, and seasonal variants coexist (P9).
Projects to `schema:logo` and `foaf:logo`.
