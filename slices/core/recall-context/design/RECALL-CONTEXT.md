<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Recall context design

This owner extends existing plans, action schemas, situations, history positions,
activities, questions and provenance. It defines neither a second plan language
nor a retrieval executor (Principles 4 and 12). The plan kind classifies a
precomposed context plan; it does not change the underlying transaction semantics.

## Allocation and parameter ownership

Recall parameters describe ActionSchema instances. Required parameter paths apply
only to the new plan-local RecallOperation profile. Existing action schemas gain
supplied-value typing without new mandatory paths. The open depth and presentation
vocabularies are explicit capability selections, not opportunistic fallbacks.

An allocation's identity is its operation record, not its numerical share or
reusable template. The finite roster belongs to one plan budget, with one share
per operation. The single source of share truth is budgetShare on that operation.
The native reduce rule derives the total from full substitutions containing the
operation identity before grouping. It therefore counts two equal shares twice.
The separate scalar and sum constraints express the exact fraction bounds and
finite allocation law. No hand-authored shape or SPARQL calculation owns the law.

## History and RDF 1.2 statements

AttentionSlot is an enduring slot identity. SlotPosition is a SubKind of the existing
State Kind and retains its meaning as a position along a Path. A position references
an existing resource; it neither copies that resource nor changes its foundational
sort. Slot-state assignment claims are the reifiers of actual RDF 1.2 assertions.
Their authoritative reified tuples identify the slot and position directly;
there are no independently authored role indexes. Supersession relates those
claims, preserving the original records and competing reports (Principles 9
and 10). The canonical predecessor constraint requires another actual assignment
reifier, while the consumer checks same-slot and bearer history against the
actual tuples. No canonical law here claims to inspect nested triple components.
Writer attribution remains separate from the slot bearer and may change between
claims. Supersession does not establish a total causal order or a single
current-view answer across agents.

Rank and exclusion reasons belong to membership assertions, so different answers
and assemblies can describe the same record independently. Language-bearing prose
uses the existing termHasLang integrity relation and retains base direction.
Actual instance records use rdf:reifies with native triple terms. Schema-level
reification obligations use the existing classic declaration form consumed by the
validation-shape derivation: rdf:Statement, subject class, predicate and typed
reifier class. These declarations are metadata about required reifiers, not
pretend exclusions, returned records or assignment occurrences.

The per-record assembly profile lives in grounding logic so its generic
reification and reason contracts introduce no dependency on this domain owner.
The existing aggregate law remains the default. Explicit profile selection moves
reason requiredness to every excluded membership, without requiring both an
aggregate and a per-record explanation. The domain-facing exclusionReason is a
canonical specialization of the generic reason, not a second independent fact.

## Answer and judgement identity

A RecallAnswer is the activity that produced one answer record. It is not identified
by cue wording, returned-record set or an outcome value. Its actual Question and
asking agent are explicit. The open domain of answers admits the activity without
adding an endurant or propositional superclass. The result roster, reported depth,
outcome and completeness describe that occurrence. Complete-empty and interrupted
outcomes have different canonical conditions.

A RecallVerdict is an attributed judgement of one answer by its asker. Matching
Question wording cannot substitute for the answer identity, and another agent's
assessment cannot substitute for that asker's judgement. The recorded words are
retained verbatim, independently of the answer's search facts.

## Explicit version identity contracts

The identity selector is owned by grounding logic. Omitting it or selecting content
identity retains the existing digest-required law. The immutable IRI mode is an
explicit external-owner contract for immutable resolution, durable retention and
supersession. It is not evidence of byte equality. It changes no checkpoint fold,
approval binding, snapshot requirement or digest-bearing immutability comparison.
No runtime fallback, hash substitute or new model lifecycle operation is introduced.

Canonical declarations, restrictions, closure entries, productive sum rule and
integrity Formula trees are authored in module.ttl. Generated OWL/RDFS and validation
surfaces remain projections of those sources (Principle 17). Documentation and
ordinary examples describe the consumer contract; they claim no executed gate,
replay or runtime certificate.
