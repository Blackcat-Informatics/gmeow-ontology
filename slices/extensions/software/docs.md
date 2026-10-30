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
(five-facet software model): git-as-provenance, verifiable releases — and this repository itself**, which
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

The verifiable-release chain (verifiable-release chain): a build consumes `gmeow:buildSource` (commit or
repository) and produces `gmeow:buildOutput` distributions, performed by a `gmeow:Builder`
(a `SoftwareAgent` — GitHub Actions, Jenkins, a local make). Integrity claims ride on
attestations via `gmeow:hasSLSALevel` (open `SLSALevel` vocabulary, levels 1–4);
signatures and transparency-log entries are reused from the attestation/trust slices,
never redeclared.

### gmeow:AuthorIdentity

Immutable history vs current identity (immutable-history author identity): the raw git bytes (`Name <email>`,
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
`entities`, `events`, `language`, `names`, `provenance`, `rights`, and `tags`.

## Project homepage and language (project homepage and language)

### gmeow:projectHomepage · gmeow:programmingLanguage

`gmeow:projectHomepage` (functional) is the project's homepage →
`doap:homepage` / `schema:url`. `gmeow:programmingLanguage` points at a first-class
`gmeow:Language` (distinguished by `lang:signSystemKind lang:programmingLanguageKind`), non-functional →
`doap:programming-language` / `schema:programmingLanguage`.

## Git objects read in place

The repository's `objectFormat` names a `GitObjectFormat`: initially
`gitObjectFormatSha1` or `gitObjectFormatSha256`, with an open vocabulary for
further formats. A repository classified with `repoTypeGit` selects exactly one
format. Other VCS types acquire no new required field. This is an object-id
interpretation, distinct from signature algorithms. Existing domain-free
`contentDigest "git:..."` records remain the object-id carrier; the ontology
introduces no hash parser or verification operation.

`CommitParentSequence` records a commit's ordered parent occurrences through
`commitParentSequence` and `parentOccurrence`. Each `CommitParentOccurrence` is an
actual RDF 1.2 reifier of a `parentCommit` assertion with one nonnegative
`entryOrdinal`. Ordinal zero is the first parent. Separate occurrence identities
preserve repeated targets and third or later parents, even when two positions
reify the same child-parent tuple. Ordinals are unique within their sequence;
they are not global commit ranks. The consumer reads the authoritative tuples
to check the owning child commit, the parent predicate and consecutive positions.
The ontology does not claim to inspect nested tuple components or run Git traversal.

`TreePathOccurrence` is identified by its `rootTree` and exact `repositoryPath`
bytes. `pathEntry` names the terminal committed `TreeEntry`, whose `entryInTree`
names its immediate containing tree. If root R has entries a and b both pointing
to subtree T, T's entry f appears at both a/f and b/f under R. Those are distinct
path occurrences even though their root and terminal entry are the same.

`repositoryPath` carries a UTF-8 `xsd:string` or byte-preserving `xsd:hexBinary`.
The consumer admits repository-relative paths with no leading solidus, NUL or
empty segments. It must preserve bytes, case, Unicode and literal dot segments;
it must not apply filesystem path normalization. Actual traversal and byte-aware
identity across the two literal encodings remain consumer operations. The
canonical laws check explicit structural bindings and identical represented path
literals; they do not claim a Git path parser. The path's root is its frame
(Principle 11).

`Checkout` records an observed working view with `checkoutRepository` and
`recordedAt`. A supplied `checkedOutCommit` names its observed commit; absence
remains unknown and permits an unborn checkout without inventing a commit.
`WorkingTreeEntry` has an `inCheckout` and exact `repositoryPath`. It is separate
from committed `TreeEntry`; dirty and untracked entries require no committed blob
or digest. The explicit `untracked` flag is optional: absence is unknown, and
false does not assert a clean file or committed-tree membership. Provenance and
recording time scope these reports without turning observations into new commits.

`holdsObject` records that a repository holds a Blob, SourceTree, Commit or
AnnotatedTag in the reporting scope. It is nonfunctional, allowing forks and
mirrors to hold the same object. It is not the inverse of the existing functional
`commitInRepository`, and it supplies no ownership, reachability or completeness
claim. Commit retains its Activity/Event sort throughout; holdings never force
it into the SourceNode/InformationObject hierarchy.

`AnnotatedTag` is the content-addressed tag object. It has its own `contentDigest`,
exact byte-preserving `tagName` and immediate `tagTarget`, which may be a blob,
tree, commit or another annotated tag. Reported `taggerIdentity` reuses the
historical `AuthorIdentity` record and has its own `taggerTime`, separate from
commit author time and observation time. Tagger identity and timestamp occur
together when that header is reported; no absent header is filled with invented
values. `tagName` and `repositoryPath` specialize the domain-free
`lang:literalValue` carrier without changing the subjects' sorts.

`TagRef` is the distinct VCS reference. General `refTarget` admits every Git object
sort above, and existing `pointsToCommit` specializes it only for direct commit
targets. No target is silently peeled to a commit. A release event can name its
VCS tag with `releaseRef`; existing `releaseTag` remains a folksonomy Tag relation.
The informal Software Heritage Release concept is linked closely to AnnotatedTag;
its connection to a GMEOW release event is only relatedness, not artifact identity.

The [read-in-place scene](examples/read-in-place-git.ttl) shows all six record
families. The [design notes](design/READ-IN-PLACE-GIT.md) describe the structural
laws and consumer boundaries. `materializationDepth` remains a projection hint;
these additions do not require an exhaustive repository export.
