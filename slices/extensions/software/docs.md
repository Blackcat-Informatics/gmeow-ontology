<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Software — the five-facet de-conflation of the project domain

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/software` · **tier: extension**
> Project ≠ Product ≠ Codebase ≠ Repository ≠ History — five identities, never one class.

DOAP's original sin was collapsing the endeavour, the product, the repo, and the history
into one class. This slice corrects it: each facet is **separately identified, separately
classed, and never bridged by subclassing or equivalence**. DOAP is aligned by reference
as a lossy downcast, never imported (Principle 5). The slice is aggressively reuse-first:
contribution comes from creative-works' `Contribution` relator (the flat `gmeow:developer`
property is *removed* — Principle 6, greenfield: the inferior element does not survive its
replacement), events and provenance from their own slices, attestation and signatures from
the trust stack. The Principle-15 consumer is **the five-facet software model
(issue #231): git-as-provenance, verifiable releases — and this repository itself**, which
the slice describes when GMEOW dogfoods its own citation and release chain.

## Facet 1 — the endeavour

### gmeow:Project

A social object — purpose, participants, duration — independent of any product, repo, or
codebase. An *endurant*, not a process: it persists and bears identity (RAiD-style
`gmeow:projectIdentifier`) while being realised through events. Carries
`gmeow:maintenanceStatus` and `gmeow:governanceModel` from open value vocabularies
(Principle 9 — maintenance judgements are standpoint-scoped: one community's "abandoned"
is another's "stable"). `gmeow:SoftwareProject` is the software `SubKind`, re-parented
from `Work` to `Project` in the de-conflation.

## Facet 2 — the product

### gmeow:SoftwareProduct

The software as intellectual creation — design, functionality, behaviour — a
specialization of `gmeow:Work` on the WEMI spine, independent of any repository or
artifact. Distributed through `gmeow:Package` (the named, versioned ecosystem unit) and
`gmeow:Distribution` (the concrete tarball/wheel/image, identified by content digest).

## Facet 3 — the codebase

### gmeow:SourceNode

The common kind for content-addressed source objects, aligned by reference to the git
object model and the Software Heritage graph: `gmeow:SourceTree` (Merkle directory
snapshot), `gmeow:SourceFile`, `gmeow:SourceDirectory`, `gmeow:Blob` (bytes only — the
name lives in the `gmeow:TreeEntry` that points to it, exactly as in git).
`gmeow:materializationDepth` on a repository bounds how much Merkle tree becomes triples —
a projection boundary, not a deletion (Principle 10); deep traversal stays in native git
or SWHID APIs.

## Facet 4 — the location

### gmeow:Repository

The hosting facet: history, branches, tags, refs. One `gmeow:repositoryType` (open VCS
vocabulary: git, hg, svn, fossil, jj, pijul), hosted at one or more `gmeow:ForgePlatform`s
(mirrors are non-functional), with `cloneUrl`/`webUrl` literals. Aligned by reference to
Software Heritage Origin and ForgeFed.

## Facet 5 — the history

### gmeow:Commit

A content-addressed provenance event (`gmeow:Activity`). `gmeow:parentCommit` (a
sub-property of `wasDerivedFrom`) is deliberately non-functional — merges have many
parents, the root has none; `gmeow:commitAncestor`/`commitDescendant` give transitive DAG
traversal and are kept non-simple, out of all cardinality axioms. Author and committer are
held apart twice over: `authoredBy` vs `committedBy` (canonical agents) and `authorTime`
vs `committerTime` (the four-clocks pattern).

### gmeow:Release

An *event* that produces a versioned product — never the product itself. Functional
`gmeow:releaseOf` ties it to its project; `releaseVersion`, `releaseTag`, and
`gmeow:releaseDoi` (non-functional — multiple registrars coexist, Principle 9) identify
it. Collaboration events (`gmeow:Push`, `gmeow:Merge`, `gmeow:CodeReview`) and artifacts
(`gmeow:Issue`, `gmeow:MergeRequest`, `gmeow:Review`, `gmeow:Diff`) follow the same
event-vs-information-object discipline: the `Review` document is not the `CodeReview`
event that produced it.

### gmeow:BuildActivity

The verifiable-release chain (issue #233): a build consumes `gmeow:buildSource` (commit or
repository) and produces `gmeow:buildOutput` distributions, performed by a `gmeow:Builder`
(a `SoftwareAgent` — GitHub Actions, Jenkins, a local make). Integrity claims ride on
attestations via `gmeow:hasSLSALevel` (open `SLSALevel` vocabulary, levels 1–4);
signatures and transparency-log entries are reused from the attestation/trust slices,
never redeclared.

### gmeow:AuthorIdentity

Immutable history vs current identity (issue #234): the raw git bytes (`Name <email>`,
via `gmeow:authorIdentityString`) recorded by `commitAuthorIdentity` /
`commitCommitterIdentity`, linked to the present self-asserted agent through
`gmeow:canonicalizedIdentity`. Old and new identities are co-equal standpoints, never
merged (Principle 9); superseded ones carry `gmeow:displayable false` (Principle 10). The
`.mailmap` file is a *generated projection* from `gmeow:mailmapEntry`, not canonical
source (Principle 4).

## Solver layer & alignment

DAG analytics beyond the asserted transitive closure — merge-base computation, blame,
diff derivation, Merkle re-hashing — belong to the solver layer (Principle 12); the graph
records the objects and their digests. Alignments are all by reference (Principle 5):
git object model, Software Heritage (Content/Directory/Revision/Origin), ForgeFed, SLSA,
PROV-O, and the DOAP downcast. Depends on `attestation`, `kernel`, `creative-works`,
`entities`, `events`, `names`, `provenance`, `rights`, and `tags`.
