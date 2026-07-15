<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# GMEOW Brand

## Positioning

GMEOW (the Global Metadata and Entity Ontology for the Web) should read as
rigorous, reasoning-centric, and open linked-data infrastructure — a canonical,
RDF 1.2-native super-vocabulary grounded by its co-foundational language,
mathematics, and logic slices — rather than as a single application.

## Tagline

A reasoning-centric super-vocabulary for the web.

## Repository description

Global Metadata and Entity Ontology for the Web — a reasoning-centric, OWL 2 DL,
RDF 1.2-native super-vocabulary grounded by `lang:`, `math:`, and `logic:`
(FOAF/REL/DOAP/PROV-O/Wikidata-aligned) and its
publishing toolchain.

## Family system

GMEOW shares the black-cat silhouette of the "g + cat-sound" family (e.g.
`gmeow` for mail, `gpurr` for Drive). Keep the family recognizable by reusing the
shared `cat-head-core` SVG group and swapping only the **service object** held by
the cat. This repository's service object is `service-graph` — a small linked
**knowledge graph** (four nodes, accent-coloured edges) — in place of the mail
`service-envelope`. The four accent colours (red `#ea4335`, blue `#4285f4`,
yellow `#fbbc05`, green `#34a853`) are isolated to the graph edges for theming.

## Colour tokens

- cat / ink: `#111214`
- paper (nodes): `#fffdf5`
- feature (eyes, whiskers): `#ffffff`
- accents: red `#ea4335`, blue `#4285f4`, yellow `#fbbc05`, green `#34a853`

## Logo assets

The machine-readable project and brand self-description lives in
[`metadata/gmeow-self.ttl`](../metadata/gmeow-self.ttl). It describes the GMEOW
software project, repository, canonical logo, and social-preview assets using
GMEOW's own `SoftwareProject`, `Repository`, `MediaObject`, and `hasLogo` terms.

- `docs/gmeow-logo.svg` — the canonical GMEOW logo (cat + knowledge graph),
  including a soft white glow so the black silhouette remains legible on dark
  backgrounds.
- `docs/social-preview.svg` — the editable GitHub sharing-card source (1280×640).
- `docs/social-preview.png` — the rendered 1280×640 GitHub social preview.

Use the SVG for README, icon, and card placements. The README references this
asset by relative path so branch previews render the branch's current logo.

Rebuild the PNG after editing the SVG:

```bash
rsvg-convert -w 1280 -h 640 docs/social-preview.svg -o docs/social-preview.png
```

GitHub repository social previews are uploaded in **repository Settings → Social
preview** (there is no API for this). Upload `docs/social-preview.png` there.
