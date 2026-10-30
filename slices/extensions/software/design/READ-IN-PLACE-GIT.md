<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Read-in-place Git records

The software slice retains its five existing facets. These additions describe
read-only object and working-view observations; they add no Git execution,
traversal, parser, hashing, signature or configuration mechanism. The governing
[Git provenance boundary](../../../../docs/git-provenance-boundary.md) keeps
deep traversal in native external consumers and materialization depth as a hint
(Principles 4 and 12).

## Object and reference sorts

Git object holdings and targets use an explicit union of Blob, SourceTree,
Commit and AnnotatedTag. Commit is already an Activity/Event; it is not moved
under SourceNode or InformationObject to make the union convenient. AnnotatedTag
is a SourceNode artifact, while TagRef is a Ref and Release is an event. The
folksonomy Tag remains separate. Domain and range axioms preserve those sorts.

The new holdings relation is nonfunctional. It says neither which repository
created a commit nor which objects a ref reaches. The existing functional
commitInRepository record is unchanged. General refTarget admits all four object
sorts; pointsToCommit supplies its narrower direct-commit case. `releaseRef`
connects a release event to a VCS tag reference without changing releaseTag's
folksonomy range.

The existing Software Heritage concept URIs are informal endpoints, not an
imported authoritative RDF ontology. AnnotatedTag has a curated close match to
the tagged-object Release concept. The old event-to-artifact leg is relatedMatch
with an explicit limited-overlap rationale. Neither leg asserts equivalence.

## Parent occurrences and identity

The original parentCommit relation preserves unordered DAG provenance. An
explicit CommitParentSequence adds an ordered occurrence roster. The existing
domain-free entryOrdinal supplies one zero-based ordinal on each separately
identified CommitParentOccurrence. Intrinsic class restrictions require exactly
one nonnegative ordinal; canonical Formula laws require actual reifiers,
sequence-scoped ordinal uniqueness and one owning sequence per occurrence.
Different positions can reify the same underlying tuple, preserving repeated
targets as well as octopus parent lists. An empty roster can describe a root
commit. No first-parent-only shortcut loses later positions.

The actual rdf:reifies tuple is the only child/parent source. The consumer checks
that its predicate is parentCommit and its child is the commit naming this
sequence, and admits consecutive positions starting at zero when publishing a
complete parent list. No duplicate child or parent role fields pretend to prove
tuple agreement. The structural laws do not claim tuple introspection or Git
object parsing.

## Exact path and checkout frames

A TreePathOccurrence's identity is root-tree identity plus exact relative path
bytes, rather than root plus terminal entry. Reusing one subtree under two names
creates distinct occurrences of its shared terminal entry. `entryInTree` names the
entry's immediate owning tree; a path occurrence's root need not equal that tree.
The canonical laws require a terminal entry's owner and consistency of the
terminal mapping for the same represented path literal under one root.

Path and tag-name literals specialize the existing lang:literalValue property,
whose domain is open and whose range is a complete RDF literal. This introduces
no linguistic Form type. The allowed path/name representations are UTF-8 strings
and hexBinary byte values. Literal bytes are not trimmed, case-folded, Unicode
normalized or resolved through dot-segment filesystem rules. Path admission
rejects a leading solidus, NUL and empty segments in the decoded byte path.
Byte-aware identity across literal encodings, lexical path admission and tree
traversal belong to the consumer. No native enforcement of those algorithms is
claimed by a structural Formula over declared RDF relations.

A Checkout is an observation snapshot with a repository and recordedAt. The
optional checkedOutCommit permits unborn views and incomplete observations;
absence alone proves neither. A WorkingTreeEntry belongs to one such snapshot,
with a path and an optional untracked report. It is disjoint from committed
TreeEntry. Neither dirty nor untracked observations require a content digest,
and false untracked status does not imply committed membership. RecordedAt and
existing provenance distinguish observation time from author and tagger clocks.

## Canonical ownership

The open GitObjectFormat vocabulary selects object-id semantics. Two guarded
laws require exactly one format for repositories explicitly classified as Git;
they add no required field to other VCS types. Hash datatype and digest validation
machinery remain outside this vocabulary change.

All structural axioms, local cardinalities, closure entries and integrity Formula
trees are authored in module.ttl. No new direct functional-property characteristic,
hand-authored OWL/RDFS surface or validation shape duplicates them (Principle 17).
The annotation catalogs cover this new and touched term batch, preserving the
rest of the existing slice without unrelated translation changes. Ordinary
examples illustrate records; they are not execution receipts or conformance cases.
