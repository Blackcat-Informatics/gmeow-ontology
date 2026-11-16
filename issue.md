# Issue #2267: Marks on an agent's important events for a trace recorder, and when marking is final

State: OPEN   Repo: Blackcat-Informatics/gmeow-ontology   Forge: github
Labels: enhancement, foundational, cross-cutting, logic, dependencies, complexity-hard, terms

## Body

A memory service for an agent keeps the agent's raw reasoning traces only
for a short tier. A separate trace recorder keeps the traces around the
agent's important events past that tier, and may discard any other trace
once it is old enough. The memory service decides which events are
important and marks them; the recorder reads the marks and joins its
traces on them. GMEOW can say that an event happened and that one record
was derived from another. It cannot say that an event is marked for a
recorder, what the recorder joins its traces on, or when the marking of a
trace is final. The service holds local terms meanwhile. They are general
memory terms, not specific to one agent.

1. **A mark on an important event.** A `TraceMark` names the event's
   record (`traceMarkOf`), its class (`traceMarkClass`, a
   `TraceMarkClass`), when it happened (`traceMarkAt`) and its anchors
   (`traceMarkAnchor`). It carries no trace content. Local classes:
   `traceMarkModalitySwitch`, `traceMarkIntentionOpened`,
   `traceMarkIntentionClosed` and `traceMarkMilestone`. Proposed: the
   mark and its class scheme as general terms, under whatever upstream
   uses for a curator's annotation of an event, with the class members
   left open.
2. **What a recorder joins its traces on.** A `TraceAnchor` names the
   digest of the source packet a trace was recorded for (`anchorPacket`,
   `xsd:hexBinary`), with the episode and round where known
   (`anchorEpisode`, `anchorRound`). A mark held without a packet
   identity says so (`traceMarkWithoutPacket`) instead of omitting it.
3. **The order of the packets, and when marking is final.** The service
   records each packet it sees, with the time that orders it
   (`TracePacket`, `tracePacket`, `tracePacketAt`). Packets are ordered by
   that time, then by digest. A `TraceWatermark` says that marking is
   final for every packet at or below one packet in that order: no mark
   will name it, and it never moves backwards. A recorder may then
   discard an unmarked packet at or below it. A derivation that marks
   events later states its committed point (`TraceMarkFrontier`), which
   the watermark never passes. Marks held only in memory and lost on a
   restart are given up explicitly, with a reason (`TraceMarksGivenUp`,
   `givenUpReason`), before the watermark may pass their packet.
   Proposed: general terms for a ledger of recorded items in a stated
   order, a finality watermark over that order, a producer's committed
   frontier, and an explicit give-up with its reason.

## Delivery map

- [x] #2504 delivered curator marks and exact source-packet anchors in #2505. Existing annotationTarget is the sole actual event-record target; local classification members remain open. Fresh repair #2506/#2507 corrects the three literal cardinality qualifiers.
- [x] #2508 delivered by #2510 owns the bounded recorded-packet evidence foundation: actual ledger membership, inherited digest/profile/scalar contracts and original ordering time with its complete TemporalFrame. This child supplies ordering inputs and claims no comparator or finality result.
- [ ] The remainder of requirement three stays here in full: actual time-then-digest order, an inclusive never-backward marking watermark, every applicable late producer frontier, and explicit lost-mark give-up with its reason before advancement. Preserve conditional old-enough/unmarked discard eligibility without claiming recording or deletion execution.

Neither child closure nor valid source records complete the remaining finality protocol. Current numeric integrity comparison cannot be substituted for temporal/digest comparison; the existing PurRDF typed comparison owner and any GMEOW exposure/projection qualification remain explicit dependencies. No invented pairwise order tuples, snapshot boundary, complete inventory or source history is required by the recorded-packet evidence child. Source-only validation and the authorized terms CI waiver do not establish generated, native, installed-consumer or source-authentication acceptance. Principles 4, 7, 9, 11, 17 and 19.

#2508 is integrated through #2510 at d537bc1b68b2f03652e0fbb40978e97f4f8d5105. Independent full source, English/French/Chinese and owner-prose review, exact five-path transfer, corrected cached catalog lint with zero warnings and normal signed hooks passed. This closes the recorded-packet evidence child only. The entire remaining ordering/finality/frontier/give-up contract above stays open; generated/native/consumer qualification remains unverified under the terms CI waiver.


## Source-reported feed position metadata delivered

#2557/#2558 adds the optional, nonfunctional tracePacketPosition property on existing TraceAnchor at cf0c124f3dc570b9529ef21c4d04e8c77a83aa38. It retains a complete original xsd:nonNegativeInteger literal under that anchor's actual applicable anchorSourceProfile, preserving inherited packet digest/preimage obligations and exact-statement source, declaring perspective and recorder. The full seven-field English/French/Chinese coat and owner guide/design ship together. No new class, law, closure, requiredness, maximum, dependency, test or example file is added.

Current consumer-source inspection found a feed-position/digest key without a clock, distinct from the original time/digest requirement above. Its actual convention starts at one and rejects zero; generic datatype membership cannot override that convention. The annotation's zero example explicitly assumes a different hypothetical supplied full profile permitting zero. Neither canonical entryOrdinal nor tracePacketAt should be substituted for the reported input. The consumer's local raw packet predicate also has a literal contract different from canonical ledger membership; this delivery establishes no alias or producer migration.

Complete independent source/language/profile/provenance review, exact five-file transfer and post-hook blob checks, cached catalog lint for two languages with zero warnings and recorded exit zero, prose/whitespace and normal signed hooks passed. Hosted review supplied only a spending-cap notice, without a source verdict or inline finding. Terms CI was explicitly waived. No other manual RDF/native/generated/runtime/build/test/producer/corpus/fixture/sync/reasoner execution ran. Current private-file observations were not compared with a historical retained revision. Automatic worktree setup subsequently completed successfully; its ignored-cache lease did not change the reviewed source.

This closes the field-retention source child only. Every original ordering/finality/frontier/give-up requirement and actual generated/native/installed/consumer admission remain open. A source-reported position establishes no timestamp/frame, observed ledger membership, comparator, committed frontier, finality, source authentication or retention/deletion permission. Principles 2, 4, 7, 9, 11, 17 and 19.


## Typed trace-order prerequisite assessed

Current source inspection confirms an existing native partial-comparison mechanism for the original temporal literal families and decoded bytes of same-datatype binary literals. The generic sorting/total RDF-term order is a different operation and cannot substitute for the requested time/digest comparator. Existing finite-numeric termLess/termGreater meanings remain unchanged. No new upstream mechanism is needed or authorized by this assessment.

A proposed separate bound integrity comparison predicate must retain full original operands and refuse invalid, unsupported or incomparable pairs explicitly in either truth polarity. The native constraint candidate-skip path must not turn those refusals into absent violations. Existing comparison lowering recognizes consequent positions; parent protocol guards require explicit builtin lowering and binding-mode integration, not an ordinary asserted graph edge. Serialization, cached/RDF/query identities and all real planner, resolver and dispatch callers must agree before any execution claim.

Binary comparison has no honest generic raw SPARQL relational projection. A native-only first prerequisite would require explicit unsupported-target loss plus a hard failure when that target operation is selected; simply omitting a procedural block and recording residue would not prove a fail-closed consumer. Supported families, allowed Formula positions and target policy remain unfrozen. Frame/profile/algorithm compatibility remains a separate actual domain premise, and no timestamp, zone, packet ordinal, RDF tie-break or comparison result is fabricated.

This read-only source/design assessment adds no primitive, protocol law, ordering tuple, source instance, code or runtime proof. No parser/build/test/producer/corpus/reasoner or retry was run. Every original ledger/order/watermark/frontier/give-up requirement remains open, independently of the current consumer's different feed-position/digest convention. Principles 2, 4, 7, 11, 12, 17 and 19.


## Comments (2)

### paudley — 2026-11-14T23:30:24Z

Blocks: #2795 (Release 0.4.0).

### paudley — 2026-11-15T00:32:16Z

Decision by the maintainer: this issue is split.

**Stays here (and lands now):** the structural vocabulary for finality, with no order law: the marking watermark, the late-producer frontier, and the explicit give-up of lost marks with its reason. Each term says plainly that it records what a recorder declared and that, without a typed comparison, nothing checks time-then-digest order or that a watermark never moves backward. Conditional old-enough/unmarked discard eligibility is preserved without claiming that recording or deletion was executed.

**Moves to its own issue:** the typed time and digest comparison primitive, and the order laws that need it (actual time-then-digest order, the inclusive never-backward watermark, every applicable late-producer frontier before advancement). The existing numeric integrity comparison must not be used for it.

Acceptance for this issue:

1. The three records are declared in the memory-records slice with definitions, usage notes, translations and one worked example each, and with axioms for what is structurally required (a give-up names its reason; a watermark and a frontier name the ledger and producer they belong to).
2. The design note states which laws are deferred to the comparison issue and why, without citing any tracker.
3. No constraint in this change compares times or digests.


