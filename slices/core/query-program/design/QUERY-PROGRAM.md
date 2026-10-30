<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Query program semantics

The source models a consumer's graph and its attributed evidence. It extends
ontology vocabulary without installing evaluation strategies in a native compiler
or executor. Every required capability of a selected consumer operation remains
mandatory; partial query answers carry explicit losses rather than masquerading
as complete results (Principles 4, 9, 11, 12 and 17).

## Identity and selection

Graph membership is explicit and finite. A node's effect classification and
stratum belong to that operation description; reuse under different classifications
requires a different node description. Reified edge endpoints and binary
consumer-to-producer dependencies agree. Operand closure prevents a branch outside
the graph from evading its effect rules. Multiple terminal sinks do not weaken
the build pipeline's separate single serializer discipline.

A ranked entry belongs to one list occurrence. Candidate identity may recur across
lists, while the entry identity changes. The contribution law is keyed by selected
entry occurrence, so this ownership is essential to preserving per-list evidence.
The output set, contribution coverage and per-candidate score coverage each have
a two-sided law: no input member is omitted and no unrelated output appears.

The reciprocal law is an integer inequality, avoiding inexact recurring decimal
division. Its finite sum is an explicit well-founded fold: every positive index
has its predecessor, indices are unique within a score, the zero step is zero,
and each matching contribution is used exactly once. The final index bounds all
steps and the final total is the score. These conditions jointly express finite
summation without an invented summation predicate or hidden aggregate behavior.

Input ranks remain exactly as supplied, including tied and truncated ranks.
Tie keys order equal-rank input rows; source-stable output ordinals order equal
fusion scores. Neither ordering changes the numerical contributions or promotes
a retrieval score into confidence.

## Loss and verdict separation

A merge's exact unions preserve candidate and loss identities. Loss cause,
affected constraint and acknowledgement remain on the original resources. No
rule collapses two losses merely because their descriptions agree, and none
manufactures an answer-axis value from a loss count.

Verdict choice binds one answer and one axis. Existing answer-axis cardinalities
come from the query-context owner. Known equality selects the left operand;
known inequality selects the right. Absence and the selected axis's unknown
sentinel select the explicit unknown branch. Not-evaluated remains a meaningful
information value, not an absent field or a completeness value.

## Feedback and measured evidence

The feedback resource policy is distinct from the acyclic workflow policy.
Positive dependencies permit equal strata; negative or nonmonotone dependencies
strictly descend. Cycles therefore contain no negative or nonmonotone edge.
Effectful nodes are terminal and cannot participate in those cycles.

The concrete convergence criterion is finite monotone candidate-set equality.
Recorded states stay within a declared finite universe, are uniquely indexed and
include every completed predecessor. Each positive state names the actual body
execution that produced it and that execution's unique previous-state input.
Distinct positive indices require distinct body executions within that run. Body
extensionality excludes a random or ambient-state-sensitive operation from being
silently admitted as a deterministic set transformer under the same description.

For a converged outcome, the last two states have equal membership and at least
one pass was completed. Budget stops and unfinished evaluation have their own
values and make no convergence claim. An equality record is evidence asserted by
the consumer, not proof that GMEOW executed or certified a loop.

## Canonical expression and boundaries

Required paths use `logic:allValuesFrom` plus class-scoped `ClosureEntry` records.
Intrinsic scalar bindings have qualified at-most-one restrictions. Cross-record
obligations use realized `logic:Constraint` Formula trees with quantified variables
and indexed argument carriers. Arithmetic uses the existing finite RDF numeric
relations. No hand-authored OWL, SHACL or ShEx surface is added.

The selection leaf declares a pure SPARQL SELECT language boundary and explicit
text/dataset/context/input/output bindings. Structural obligations establish those
bindings, not grammatical validity of arbitrary text. Parsing and semantic
admission remain the selected consumer's responsibility; this source neither
claims a parser exists here nor silently executes unsupported text. Query and
transaction-program NodeKind values remain individuals, distinct from classes.

An effective enactment principal is recorded authority context. Its presence does
not establish a permission check, identify a tool provider or weaken the existing
Enactment identity contract.
