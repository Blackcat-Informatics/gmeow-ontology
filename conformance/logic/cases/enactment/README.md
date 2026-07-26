# Enactment kernel conformance corpus

Each case is a kernel A-Box (`input.nq`, a named data world) driven through the same
conformance harness the rest of `conformance/logic/` uses, together with the logic
program in `input.logic.ttl`. Every case blesses the reasoned closure, the
consistency verdict, the budget report, and the full projection fan-out — canonical
RDF 1.2, Datalog, N3, OWL-DL, OWL-EL, gUFO, the preservation ledger and the
projection report.

## The case that carries the argument

`frontier-labels-are-derived` is the one to read first. Not one `logic:entryLabel`
appears in its data world: every entry carries only its lifecycle-axis witnesses,
and the five shipped `logic:Rule` instances — lifted verbatim from
`slices/grounding/logic/module.ttl`, so the case pins the rules that actually ship
rather than a restatement of them — compute the label. The golden shows all five
derived:

    readyEntry      StepReady + ApprovalNull     -> FrontierReadyAuthorized
    gatedEntry      StepReady + ApprovalCreated  -> FrontierReadyApprovalRequired
    unknownEntry    EffectAttempted              -> FrontierReconciliationRequired
    receiptedEntry  EffectReceipted              -> FrontierCompensationEligible
    blockedEntry    StepWaiting + a gap on its   -> FrontierBlockedCapabilityOrResource
                    action step

That is the frontier as a derived total function of the axis tuple rather than an
asserted label, which is the claim the whole seven-axis design rests on.

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
exercising a different property; `frontier-labels-are-derived` is where the
derivation itself is under test.
