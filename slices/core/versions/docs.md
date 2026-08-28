<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Versions — modelling & interoperability guide

Most vocabularies model versions as ad-hoc attributes: `schema:version` on a
`CreativeWork`, SemVer strings on a DOAP release, `dcterms:isVersionOf` between
dataset snapshots. Each domain reinvents its own taxonomy: `StableRelease`,
`YankedCrate`, `DefinitiveEdition`, `CanonicalEmail`.

GMEOW refuses this anti-pattern. **"Latest", "stable", "yanked", "canonical",
"definitive" are not intrinsic types** — they are standpoint-scoped claims
asserted by an authority (a registry, a publisher, a project, a curator). The
same artifact may be `latest` to npm and `deprecated` to a downstream mirror.
Both claims coexist without privilege (Principle 9).

## Decision tree: which property to use?

| Situation | Use | Example |
|---|---|---|
| A concrete artifact belongs to a stable lineage | `versionOf` | `release-2.0.0 versionOf my-library` |
| A concrete edition of a creative work | `editionOf` | `annotated-edition editionOf original-novel` |
| One artifact replaces another | `supersedes` | `v2.0.0 supersedes v1.1.0` |
| Cross-frame correspondence without merge | `counterpartOf` | `robot-agent counterpartOf person` |
| Derivation/provenance chain | `wasDerivedFrom` | `translation wasDerivedFrom original` |
| Role/status within a version set (reified) | `VersionMembership` | `membership roleLatest accordingTo registry` |

The **thin spine** (`versionOf`, `editionOf`, `supersedes`, `counterpartOf`) is
the 80 % flat shortcut. The **reified layer** (`VersionMembership`) is promoted
when the role claim itself must carry authority, confidence, temporal scope, or
standpoint indexing.

## The reified layer: VersionMembership

A `VersionMembership` is an `Observation` + `Relator` in the universal claim
stack. It mediates three things:

- **`versionMember`** — the concrete artifact (observedFeature)
- **`versionSet`** — the lineage it belongs to
- **`versionRole`** / **`versionScale`** — the classification, as `QualityValue` individuals
- **`membershipAuthority`** (vantage) — who asserts this role
- **`membershipInterval`** — when the claim holds

```turtle
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix ex:    <https://example.org/versions/> .

ex:myLibrary a gmeow:VersionSet ;
    gmeow:name "my-library"@en .

ex:v2_0 a gmeow:SoftwareAgent ;
    gmeow:versionOf ex:myLibrary ;
    gmeow:versionLabel "2.0.0" .

ex:memLatest a gmeow:VersionMembership ;
    gmeow:versionMember ex:v2_0 ;
    gmeow:versionSet ex:myLibrary ;
    gmeow:versionRole gmeow:roleLatest ;
    gmeow:membershipAuthority ex:npmRegistry .

ex:memLTS a gmeow:VersionMembership ;
    gmeow:versionMember ex:v2_0 ;
    gmeow:versionSet ex:myLibrary ;
    gmeow:versionRole gmeow:roleLTS ;
    gmeow:membershipAuthority ex:projectMaintainer .
```

`★` Two memberships on the same artifact, from two authorities, with two
roles — both are first-class and co-equal.

## Temporal scoping: never overwrite

When a release shifts from `latest` to `deprecated`, **do not overwrite the
old membership**. Preserve history by either:

1. **Closing the interval** — set an end date on `membershipInterval`.
2. **Minting a fresh membership** — create a new `VersionMembership` for the
   new role, leaving the old one intact.

A deprecated or yanked release may carry `gmeow:displayable false` so it is
suppressed from consumer projections, but it is **never deleted** (Principle 10).

```turtle
# Old membership retained
ex:memLatest a gmeow:VersionMembership ;
    gmeow:versionMember ex:v1_1 ;
    gmeow:versionSet ex:myLibrary ;
    gmeow:versionRole gmeow:roleLatest ;
    gmeow:membershipAuthority ex:npmRegistry .

# New membership for the deprecated state
ex:memDeprecated a gmeow:VersionMembership ;
    gmeow:versionMember ex:v1_1 ;
    gmeow:versionSet ex:myLibrary ;
    gmeow:versionRole gmeow:roleDeprecated ;
    gmeow:membershipAuthority ex:npmRegistry ;
    gmeow:displayable false .
```

## Separation from attestation layer (attestation layer)

`VersionMembership` records **what role is asserted** and **by whom**.
It does **not** embed the cryptographic or signed evidence for that assertion.

Attestation evidence — release signatures, SLSA provenance, DOI/SWHID
assertions, yanked-release notices, registry attestations — belongs in the
future attestation layer attestation layer and **composes with** `VersionMembership` by
linking evidence to the same artifacts. A version role may be *attested by* a
registry, but the attestation is evidence **for** the role claim, not the role
itself.

```turtle
# version-set layer: the role claim
ex:memLatest a gmeow:VersionMembership ;
    gmeow:versionRole gmeow:roleLatest ;
    gmeow:membershipAuthority ex:npmRegistry .

# attestation layer: the attestation evidence
ex:sig a gmeow:AttestationArtifact ;
    gmeow:artifactMediaType "application/vnd.dsse+json" ;
    gmeow:hasSignature [ a gmeow:CryptographicSignature ] ;
    gmeow:wasAttributedTo ex:npmRegistry .
```

## Value vocabularies: open, not a fence

`VersionRole` and `VersionScale` are `gufo:QualityValue` subclasses. The seed
list is an anchor, not a fence:

| Seed | Meaning |
|---|---|
| `roleCanonical` | The canonical/reference variant |
| `roleVariant` | A non-canonical variant |
| `roleLatest` | The most recent release |
| `roleStable` | Considered stable by the authority |
| `roleLTS` | Long-term support commitment |
| `roleDeprecated` | Discouraged but retained |
| `roleYanked` | Withdrawn with urgency |
| `roleDraft` | Unpublished / in-progress |
| `rolePublished` | Publicly available |
| `roleRevised` | A revised edition |
| `roleCollected` | Part of a collected volume |
| `roleWithdrawn` | Formally withdrawn |
| `scaleTrivial` | Patch-level change |
| `scaleMinor` | Backward-compatible change |
| `scaleMajor` | Breaking change |

Domain-specific values (e.g. `roleNightly`, `roleReleaseCandidate`) are minted
as fresh individuals carrying `rdfs:label`.

## Projections

The mapping compiler generates SSSOM/EDOAL/SPARQL projections from
`mapping-dsl/`. Cross-vocabulary mappings include:

| GMEOW | schema.org | DOAP | Wikidata |
|---|---|---|---|
| `VersionSet` | — | `doap:Project` (lineage) | — |
| `versionOf` | — | — | P548 (version type) |
| `versionLabel` | `schema:version` | `doap:revision` | — |
| `versionRole` | — | — | P548 qualifier |
| `supersedes` | — | — | P1365 (replaces) |

`★` Projections are lossy by design. A "latest" selection rule lives in the
importer/solver, never as an OWL axiom (Principle 12).

## Terms

### gmeow:VersionSet · gmeow:versionFingerprint

A `VersionSet` is the stable lineage a concrete artifact belongs to — the spine
the thin flat shortcuts attach to. `versionFingerprint` carries a content
fingerprint of a versioned entity (hash, SWHID, content digest, or semantic
identifier); broader than the byte-exact `contentDigest`, and non-functional, so
one entity may carry several under different schemes.

### gmeow:VersionMembership · gmeow:versionMember · gmeow:versionSet · gmeow:versionRole · gmeow:versionScale · gmeow:membershipAuthority · gmeow:membershipInterval

The reified `Observation` + `Relator` promoted when a role claim must carry
authority, confidence, temporal scope, or standpoint indexing. It mediates the
`versionMember` artifact, the `versionSet` lineage, the `versionRole` /
`versionScale` classification (as `QualityValue` individuals), the
`membershipAuthority` that asserts it (the vantage), and the `membershipInterval`
over which the claim holds. Two memberships on the same artifact from two
authorities are first-class and co-equal — never overwrite, mint a fresh one
(Principle 9, Principle 10).

### gmeow:VersionRole · gmeow:VersionScale

The open value vocabularies (`gufo:QualityValue` subclasses) — a seed list, not a
fence. `VersionRole` ranges over `roleLatest`, `roleStable`, `roleLTS`,
`roleDeprecated`, `roleYanked`, `roleCanonical`, … ("latest"/"stable"/"yanked"
are standpoint-scoped claims, never intrinsic types); `VersionScale` over
`scaleTrivial` / `scaleMinor` / `scaleMajor`. Domain-specific values are minted as
fresh individuals carrying `rdfs:label`.

### gmeow:TermStability · gmeow:termStability · gmeow:addedInVersion · gmeow:ChangelogEntry · gmeow:hasChangelogEntry · gmeow:entryVersion · gmeow:entryNote · gmeow:definitionDigest

Per-term lifecycle metadata — maturity and changelog signals about a
**vocabulary term** (a class or property in the TBox), as distinct from the
standpoint-scoped *instance-level* version roles above. The lineage layer answers
"what role does this concrete release hold?"; this layer answers "how mature is
this term, and how has it changed across releases?".

These predicates are `owl:AnnotationProperty` — the `gmeow:accordingTo` pattern
(Principle 3) — because they are asserted *about* a class or property, so the
generated OWL stays in OWL 2 DL. `gmeow:TermStability` is the open value
vocabulary (`stabilityStable` / `stabilityExperimental` / `stabilityDeprecated`,
a seed list, not a fence) referenced by `gmeow:termStability`; the ontology-docs
generator derives a **default** badge from the owner slice's tier (core → stable,
extension → experimental) and from `owl:deprecated`, and an explicit
`gmeow:termStability` overrides it. `gmeow:addedInVersion` seeds the per-term
changelog with the release a term debuted in; `gmeow:hasChangelogEntry` attaches
reified `gmeow:ChangelogEntry` records (`gmeow:entryVersion` + optional
`gmeow:entryNote` prose), ordered by version on the term's docs page.
`gmeow:definitionDigest` is the RDFC-1.0-canonical blake3 content-address of a
term's defining triples — computed by the pipeline (never hand-authored) over the
term's concise bounded description with the per-term provenance predicates
excluded, so recording provenance never perturbs the digest. It drives the
**computed** changelog: a term whose definition digest differs from the prior
release records an automatic change entry, realizing the release-as-evidence
content-address diffs (CONSTITUTION §18) that the editorially-seeded entries above
anticipate; it also serves as a stable citation permalink for the term's meaning.
The comparison authority is the tracked
`metadata/releases/term-content-authority.json`, never the ignored
`generated/catalog/term-content-manifest.nq` from a local build. Ordinary sync
only reads that authority, so fresh and warm worktrees render the same history.
At an accepted release boundary a maintainer advances it explicitly with
`make maint-refresh-term-release-authority`; that producer proves the promoted
manifest remains a fixed point before it writes the new authority.
