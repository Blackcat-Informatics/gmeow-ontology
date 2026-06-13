# Up-projection gap triage (#449 → #451 plan)

Curated triage of the 100 distinct coverage-gap terms from
`docs/up-projection-audit.md` (run `gmeow up-projection-audit --gaps` for the raw
list). Categorization is judgment + verification against the merged vocabulary —
"genuine gap" means the concept was confirmed **absent**, not heuristically guessed.

| Bucket | response |
|---|---|
| **A — has-concept, needs a cell** | GMEOW already models it; author the up+down cell (improves both directions). The bulk of #451. |
| **B — pass-through** | authority / concept-scheme links; carry as-is, not "coverage" |
| **C — genuine GMEOW gap** | flesh out the slice (real modeling) |
| **D — declared out-of-scope** | the #34 site-structure tail; stays deferred |

## C — genuine gaps (verified absent) → slice work

### Organization business facet (✅ landed — full schema:Org coverage)

`schema:slogan`, `schema:taxID`, `schema:priceRange`, `schema:currenciesAccepted`
all confirmed absent; `schema:paymentAccepted` is half-present (`gmeow:Payment`/
`PaymentMethod` exist, but no "an organization accepts X" relation). A coherent
commercial-organization facet on the Organization slice — model once, get up- and
down-projection for every business that publishes these.

### Other genuine gaps, by home slice

- **Person:** `schema:nationality`; `schema:alumni`/`alumniOf` (an education
  relation — likely a Membership/Participation specialization, not new terms).
- **Media:** `schema:transcript`, `schema:caption` (GMEOW's `transcriptionOf` is
  *phonetic* transcription — wrong sense).
- **Identity:** `schema:brand` (only `registerBrandVoice` exists, unrelated).
- **Software project:** `doap:bug-database`, `doap:download-page`.
- **Niche / likely out-of-scope:** `schema:callsign` (broadcast).

## A — has-concept, needs a cell (the #451 cell-authoring segments)

Clean 1:1 quick wins (GMEOW has the identical term, no cell ever wired it):
`geosparql:Geometry`/`asWKT`/`hasGeometry`, `schema:conformsTo`→`conformsTo`,
`schema:validFrom`→`validFrom`, `schema:validThrough`→`validUntil`,
`schema:dataset`→`Dataset`, `schema:owner`→`ownerOf`, `schema:editor`→`hasEditor`,
`schema:subOrganization`→`subOrganizationOf`, `foaf:phone`→`telephone`,
`vcard:role`→`Role`.

Richer-cell cases (concept exists): `org:Site`/`siteOf`/`siteAddress`→
`hasSite`/`SiteType` (**overturns the #408 refusal** — the slice grew a site model
since), `schema:employee`→Employment, `schema:reviewedBy`→Assessment,
`schema:hoursAvailable`→`hasOpeningHours`, `schema:paymentAccepted`→PaymentMethod,
`schema:occupationalCategory`→`occupationClassification`, the location cluster
(`contentLocation`/`homeLocation`/`workLocation`/`locationCreated`/`foundingLocation`),
the credential cluster, the media-encoding cluster, `schema:legalName`,
`schema:member`/`foaf:member`→`hasMember`, `foaf:depiction`→depiction machinery,
the `interactionStatistic`/`InteractionCounter`/`userInteractionCount` engagement
metrics (most modelling exists via observation/measurement).

**Semantic trap:** `foaf:title` is a *courtesy title* (Mr/Dr), **not** a work
title — map to the honorific machinery, never `gmeow:title`.

## B — pass-through

`skos:closeMatch`, `skos:exactMatch`, `skos:hasTopConcept`,
`mads:authoritativeLabel`, `odrl:inheritFrom`.

## D — declared out-of-scope (the #34 site-structure tail)

`schema:BreadcrumbList`/`ListItem`/`itemListElement`/`EntryPoint`/`WebPageElement`/
`breadcrumb`, `schema:Periodical`/`PodcastEpisode`/`PodcastSeries`,
`schema:hasMap`/`urlTemplate`.
