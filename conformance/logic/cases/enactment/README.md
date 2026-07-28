# Enactment kernel conformance corpus

Each case is a kernel A-Box (`input.nq`, a named data world) driven through the same
conformance harness the rest of `conformance/logic/` uses, together with the logic
program in `input.logic.ttl`. Every case blesses the reasoned closure, the
consistency verdict, the budget report, and the full projection fan-out — canonical
RDF 1.2, Datalog, N3, OWL-DL, OWL-EL, gUFO, the preservation ledger and the
projection report.

## The two cases that carry the argument

`frontier-labels-are-derived` shows that a frontier label CAN be derived;
`frontier-roster-derives-every-label` shows that EVERY one of the sixteen is. Read
them in that order.

`frontier-roster-derives-every-label` is the closure argument. Its data world
asserts no `logic:entryLabel` either, and its sixteen entries — one per
`logic:FrontierLabel` individual — produce all sixteen labels from the sixteen
shipped `logic:Rule` instances. A roster that enumerates sixteen positions and can
compute five of them is the kernel's own named required-negative, an incomplete
frontier presented as closed; sixteen entries, sixteen derived labels and zero
asserted ones is what retires it. Four of its pairs carry the disjointness argument
and each pair differs on exactly one axis: readiness versus approval, quiescent
cancellation versus cancellation with in-flight effects (the commitment-knowledge
axis), terminal failure versus a failed compensation over a still-committed forward
effect (the enactment axis), and a capability block versus an ordinary wait — the
last told apart by what is recorded about the obstruction, because a step blocked by
a capability the deployment does not have awaits nothing.

Not one `logic:entryLabel` appears in the data world of
`frontier-labels-are-derived` either: every entry carries only its lifecycle-axis
witnesses, and the shipped `logic:Rule` instances — lifted verbatim from
`slices/grounding/logic/module.ttl`, so the cases pin the rules that actually ship
rather than a restatement of them — compute the label. Its golden shows five
derived:

    readyEntry      StepReady + ApprovalNull     -> FrontierReadyAuthorized
    gatedEntry      StepReady + ApprovalCreated  -> FrontierReadyApprovalRequired
    unknownEntry    EffectAttempted              -> FrontierReconciliationRequired
    receiptedEntry  EffectReceipted              -> FrontierCompensationEligible
    blockedEntry    StepWaiting + a gap on its   -> FrontierBlockedCapabilityOrResource
                    action step

That is the frontier as a derived total function of the axis tuple rather than an
asserted label, which is the claim the whole seven-axis design rests on — and it is
`frontier-roster-derives-every-label` that makes the function total rather than
merely non-empty.

Two of those rows carry the load the rest of the kernel rests on. `unknownEntry` is
the no-blind-retry law stated POSITIVELY: rather than only forbidding a retry from
an undetermined position, the frontier names reconciliation as the licensed action,
so an operator is told what to do instead of only what not to do. `receiptedEntry`
derives ELIGIBILITY and nothing further — no compensation attempt, no outcome, and
no claim that compensating will succeed, because compensation is a new action with
its own possibility of failing.

`readyEntry` and `gatedEntry` differ in exactly one axis position, which is the
argument for keeping readiness and approval separate: a single blended state would
need a combined value for every such pairing.

## The rest

The remaining seven cases are scenario A-Boxes — the continuing cluster across two
occurrences, capability present and absent, the crash at the effect boundary, exact
approval binding, contextual recommendation with a tie and an incomparability, and a
budget-bounded run. They pin the closure and the projection surface for those
shapes. Their frontier labels are asserted rather than derived, because each case is
exercising a different property; the two frontier cases are where the derivation
itself is under test.
