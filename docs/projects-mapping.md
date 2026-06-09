<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Projects, Software & Verifiable Provenance — modelling & interoperability guide

A "project" is not one thing. DOAP's original sin — collapsing the *endeavour*,
*product*, *codebase*, *repository*, and *history* into a single `doap:Project` —
flattens five orthogonal facets into one class, freezes contributor roles into a
six-item enum, and pretends git history is just a bag of strings. No existing
ontology unifies the social project, the software work, the content-addressed
source tree, the cryptographic supply chain, and people-in-roles-over-time.

GMEOW models all of it the way it modelled names and gender: **de-conflate,
reify, ground in gUFO, fold by reference.**

## The reframe

| Real-world fact | What a flat `doap:Project` cannot express | GMEOW facet |
|---|---|---|
| A project continues after its repo is deleted or moved | conflates project and repository | **Project** (endeavour) |
| A library has many packages (npm, PyPI, Cargo) | conflates product and package | **Product / Work** |
| The same source tree lives in two repos (forks, mirrors) | conflates codebase and location | **Codebase** |
| A commit's author is not the committer | no authorship provenance | **History** |
| A contributor transitions; old commits must not be rewritten | history is immutable; identity is not | **Suppression, not erasure** |
| An AI agent commits code and reviews PRs | no role for non-human agents | **AI as first-class subject** |
| "Is this release the output of that reviewed commit?" | no bridge to supply-chain attestation | **Verifiable provenance** |

Two ETHOS centrepieces fall straight out of this architecture:

1. **AI / agent contributors as first-class self-asserting subjects** (Principle 9).
2. **Correct identity over immutable git history** — the deadname / `.mailmap`
   problem — via suppression, not erasure (Principle 10).

## The five facets — orthogonal, never bridged

`Project ≠ Product ≠ Codebase ≠ Repository ≠ History`. Each facet is separately
identified, separately classed, and never linked by `rdfs:subClassOf` or
`owl:equivalentClass`. A SHACL shape enforces this at instance level: an
individual typed in two facet classes is a violation.

### Facet 1 — Project (the endeavour)

`gmeow:Project ⊑ gmeow:Entity` (social object, endurant). A project persists,
bears identity and PIDs (typically a RAiD, ISO 23527), and participants come and
go. It is *realised through* events (releases, sprints, milestones), not
identical to them.

`gmeow:SoftwareProject ⊑ gmeow:Project` — the social object facet only.

| Property | Range | Purpose |
|---|---|---|
| `projectIdentifier` | `rdfs:Literal` | RAiD (activity PID) |
| `hasRepository` | `Repository` | Monorepo, mirrors, forks allowed |
| `maintenanceStatus` | `MaintenanceStatus` | active / maintained / deprecated / abandoned / EOL |
| `governanceModel` | `GovernanceModel` | BDFL / foundation / meritocracy / DAO / corporate |
| `projectLicense` | `License` | Dual-licensing valid |
| `hasRelease` | `Release` | Many over time |

Both status and governance are **non-functional**: one community may judge a
project abandoned while another considers it stable (Principle 9).

### Facet 2 — Product / Work

`gmeow:SoftwareProduct ⊑ gmeow:Work` (the WEMI spine). The intellectual creation
— design, functionality, and behaviour — independent of any particular source
repository, release event, or concrete artifact.

`gmeow:Package` — a named, versioned unit published through a package ecosystem.
`gmeow:Distribution` — a concrete artifact (tarball, wheel, binary, container)
identified by `contentDigest`.

### Facet 3 — Codebase (content-addressed source)

`gmeow:SourceNode` — the common kind for all objects in a versioned source tree.
`gmeow:SourceTree` (Merkle directory), `gmeow:SourceFile` / `Blob` (named blob
in a tree), `gmeow:TreeEntry` (name + mode + pointer). Identity is by content
digest (git hash or SWHID), not by path or repository.

### Facet 4 — Repository (location / hosting)

`gmeow:Repository ⊑ gmeow:InformationObject`. The hosting facet: clone URL,
web URL, forge platform, VCS type. A repository has exactly one
`repositoryType` (git, hg, svn, fossil, jj, pijul, …) — a **value vocabulary**
(open, individuals only, never subclasses).

### Facet 5 — History (provenance / events)

`gmeow:Commit ⊑ gmeow:Activity` (content-addressed provenance event).
`gmeow:Release ⊑ gmeow:Event` (the occurrence, not the product).

| Property | Range | Notes |
|---|---|---|
| `parentCommit` | `Commit` | `⊑ wasDerivedFrom`; non-functional (merge commits) |
| `commitAncestor` | `Commit` | Transitive closure of `parentCommit` |
| `authoredBy` | `Agent` | Creative origin of the patch |
| `committedBy` | `Agent` | Who created the commit object |
| `authorTime` / `committerTime` | `xsd:dateTime` | Four-clocks pattern |
| `commitTree` | `SourceTree` | Exactly one tree per commit |

Also: `Branch`, `Tag`, `Ref` (mutable pointers); `Push`, `Merge`, `CodeReview`
(collaboration events); `Diff` (patch content).

## Contributions & roles — reified relators, never flat properties

The `developer` property from the original stub is **removed** (Principle 6,
greenfield). In its place:

- **`gmeow:Contribution`** — a `gufo:Relator` binding {agent} × {target} ×
  {role} × {degree} × {period} × provenance. Reuses #211's universal credit
  relator (CRediT + software/mapping-authorship roles).
- **Open value vocabulary**: `ContributionRole` carries `roleSoftwareMaintainer`,
  `roleSoftwareDeveloper`, `roleCodeReviewer`, `roleReleaser`,
  `roleSecurityContact`, `roleBotContributor`, `roleAIAssistant`, … —
  individuals, never subclasses (Principle 9).
- **`gmeow:hasContributor`** — flat shortcut for the 80% case.

AI agents fit cleanly: a bot or AI assistant is a `gmeow:SoftwareAgent` (a
`gmeow:Agent`), and its contribution is a `Contribution` with attributed
provenance, confidence, and `selfAsserted` metadata.

## Identity over immutable history — suppression, not erasure

Git history is content-addressed and immutable. A contributor's old name/email
is hashed into every commit and cannot be rewritten without destroying the DAG.
`.mailmap` is the crude patch — and, tellingly, it does **not** rewrite the
stored author bytes; it is a *presentation-layer* canonicalisation.

GMEOW models this correctly:

- **`gmeow:AuthorIdentity`** — the raw bytes as they appeared in the commit
  (`"Name <email>"`). This is the immutable historical assertion.
- **`gmeow:canonicalizedIdentity`** — links the raw bytes to the current
  self-asserted `Agent`.
- **Superseded identities** carry `gmeow:displayable false`; they are retained,
  never deleted (Principle 10).
- **`.mailmap`** is a **generated projection** (Principle 4), not canonical
  source. It emits the canonical line plus remapping lines for suppressed
  identities.

Old and new identities coexist as co-equal standpoints, never merged by
`owl:sameAs` (Principle 9).

## Verifiable release chain

The full chain from source to published artifact:

```text
signed commit → signed tag → BuildActivity → SLSA attestation →
cosign signature → Rekor transparency-log entry → DOI + SWHID
```

- **`BuildActivity`** (`⊑ Activity`) — consumes a commit, produces a
  `Distribution`, carries `buildConfigUri`.
- **`Builder`** (`⊑ SoftwareAgent`) — GitHub Actions, GitLab CI, Jenkins, etc.
- **`Attestation`** — reuses #162's generic attestation infrastructure:
  `attestationTypeSLSAProvenance`, `hasSLSALevel` (Level 1–4 value vocabulary),
  `attestedSubject` (content-digested artifact).
- **`TransparencyLogEntry`** — Rekor inclusion proof.
- **`releaseDoi`** — Digital Object Identifier on the release event.
- **`contentDigest`** — SWHID on commit, tree, blob, and distribution.

Trust is perspectival: a verified signature proves integrity/key control; a log
proves inclusion; an attestation records what an issuer vouched for. None makes
the release trustworthy by OWL entailment (Principle 12).

## Graph-explosion boundary

Deep materialisation of the Merkle tree for a repository with 500 000 commits
would crater a triple store. GMEOW provides a **projection boundary**:
`materializationDepth` on `Repository` controls how many levels of tree are
materialised as triples (0 = commits/refs only; 1 = root tree; 2 = root + one
level; …). Deep traversal is left to native git or SWHID APIs — computed, not
asserted (Principle 12).

## External alignments

| GMEOW term | External term | Predicate | Confidence | Lossy notes |
|---|---|---|---|---|
| `SoftwareProject` | `doap:Project` | `skos:closeMatch` | 0.75 | DOAP conflates all five facets |
| `Repository` | `doap:Repository` | `skos:closeMatch` | 0.85 | location facet only |
| `Release` | `doap:Version` | `skos:closeMatch` | 0.80 | event vs version-string conflation |
| `SoftwareProduct` | `schema:SoftwareSourceCode` | `skos:closeMatch` | 0.80 | schema.org conflates source + app |
| `SoftwareProduct` | `schema:SoftwareApplication` | `skos:closeMatch` | 0.80 | runtime vs abstract work |
| `Repository` | `schema:codeRepository` | `skos:closeMatch` | 0.85 | entity vs URL literal |
| `Package` | `spdx:Package` | `skos:closeMatch` | 0.85 | SBOM descriptor vs distributable unit |
| `SourceFile` | `spdx:File` | `skos:closeMatch` | 0.90 | direct correspondence |
| `SoftwareProduct` | `codemeta:SoftwareSourceCode` | `skos:closeMatch` | 0.80 | abstract work vs exchange form |
| `Repository` | `forgefed:Repository` | `skos:closeMatch` | 0.85 | ActivityPub representation |
| `Commit` | `forgefed:Commit` | `skos:closeMatch` | 0.85 | federated vs provenance event |
| `Issue` | `forgefed:Ticket` | `skos:closeMatch` | 0.80 | tracked work item |
| `Blob` | `swh:Content` | `skos:closeMatch` | 0.90 | raw content (bytes only) |
| `SourceTree` | `swh:Directory` | `skos:closeMatch` | 0.90 | content-addressed tree |
| `Commit` | `swh:Revision` | `skos:closeMatch` | 0.90 | commit with author/committer/tree |
| `Release` | `swh:Release` | `skos:closeMatch` | 0.90 | named revision (tag) |
| `Repository` | `swh:Origin` | `skos:closeMatch` | 0.85 | origin URL vs hosting facet |

Software Heritage and RAiD do not publish standard RDF ontologies; alignments
are carried as `skos:closeMatch` to informal concept URIs and will be refined
when stable vocabularies emerge.

## Projections (generated lossy downcasts)

All projections live in `mapping-dsl/projections/` and compile to
`queries/projections/*.rq` and `projections/*.edoal.ttl` (Principle 4).

| Target | Status | Lossy notes |
|---|---|---|
| **CodeMeta** | `codemeta.ttl` | flagship exchange form |
| **DOAP** | `doap.ttl` | collapse facets, flatten roles, drop suppressed |
| schema.org | `schema-org.ttl` | existing |
| SPDX 3.0 | `spdx.ttl` | existing |
| in-toto / SLSA | `intoto.ttl`, `slsa.ttl` | existing |
| Sigstore / Rekor | `sigstore.ttl` | existing |
| **.mailmap** | `mailmap.ttl` | existing (Phase D) |

## Reuse — little is invented

| Primitive | Source module | Used for |
|---|---|---|
| `Activity`, `Event`, `Participation` | `events.ttl`, `provenance.ttl` | Commit, Release, Push, Merge, CodeReview, BuildActivity |
| `Contribution`, `ContributionRole` | `creative-works.ttl` (#211) | Agent × project/commit/release × role |
| `VersionSet`, `VersionMembership` | `versions.ttl` (#161) | Release versioning without facet collapse |
| `Attestation`, `CryptographicSignature`, `TransparencyLogEntry` | `attestation.ttl`, `trust.ttl`, `messaging-trust.ttl` (#162) | SLSA, cosign, Rekor |
| `contentDigest`, `versionFingerprint` | `sources.ttl`, `versions.ttl` | Git hashes, SWHIDs |
| `TimeInterval`, four clocks | `temporal.ttl` | author-time, committer-time, push-time |
| `displayable`, `selfAsserted` | `core.ttl` | Suppression, AI authorship |
| `Agreement` | `agreements.ttl` | CLA, DCO |
| `Membership`, `Role` | `organization.ttl` | Corporate-OSS bridge |

## References

1. DOAP — Description of a Project. <http://usefulinc.com/ns/doap#>
2. CodeMeta — schema.org-based software-metadata crosswalk.
   <https://codemeta.github.io/>
3. SPDX 3.0 — RDF/OWL/SHACL model; ISO/IEC 5962.
   <https://spdx.github.io/spdx-spec/v3.0.1/>
4. Software Heritage data model + SWHID (ISO/IEC 18670:2025).
   <https://docs.softwareheritage.org/devel/swh-model/data-model.html>
5. ForgeFed — ActivityPub forge-federation vocabulary.
   <https://forgefed.org/>
6. RAiD — Research Activity Identifier, ISO 23527:2022.
   <https://www.raid.org/>
7. SLSA provenance + in-toto attestations + Sigstore.
   <https://slsa.dev/> · <https://in-toto.io/> · <https://www.sigstore.dev/>
8. W3C DID / Verifiable Credentials.
   <https://www.w3.org/TR/did-core/> · <https://www.w3.org/TR/vc-data-model/>
9. FRAPO, VIVO, CERIF — general / research project models.
10. git `.mailmap` — non-destructive contributor-identity canonicalization.
    <https://git-scm.com/docs/gitmailmap>
