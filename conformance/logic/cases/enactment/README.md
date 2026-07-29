# Enactment kernel conformance corpus

Each case is a kernel A-Box (`input.nq`, a named data world) driven through the same
conformance harness the rest of `conformance/logic/` uses, together with the logic
program in `input.logic.ttl`. Every case blesses the reasoned closure, the
consistency verdict, the budget report, and the full projection fan-out — canonical
RDF 1.2, Datalog, N3, OWL-DL, OWL-EL, gUFO, the preservation ledger and the
projection report.

## Every case derives, and derives with the rules that ship

No case in this corpus asserts a `logic:entryLabel`, and no case restates a
`logic:Rule` inside its own input. Each `profile.json` names the rule IRIs it loads
in `"shipped_rules"`, and the harness resolves every one of them against
`slices/grounding/logic/module.ttl`, hard-failing on an IRI the module no longer
declares. That resolution is the pin: renaming or deleting a shipped rule reds every
case that reasons with it, where a case carrying its own copy of the rule would have
stayed green and gone on certifying a rule set nobody ships.

The scenario cases load the WHOLE shipped enactment rule set — all sixteen
frontier-label rules and all eleven means–end refinement rules — rather than the
handful that fire. A label a case's golden does not show is therefore a label its
data does not license, not one no rule was present to reach, which is what makes the
negative half of each case (no blind retry, no invented capability, no manufactured
winner) a gated fact rather than an omission.

Because the derived quads are the difference between `input.nq` and
`expected/materialized.nq`, a case whose materialization equals its input is by
construction proving nothing:

```sh
for c in conformance/logic/cases/enactment/*/; do
  diff <(sort "$c/input.nq") <(sort "$c/expected/materialized.nq") >/dev/null \
    && echo "STILL AN ECHO: $c"; done
```

must print nothing.

## The two cases that carry the argument

`frontier-labels-are-derived` shows that a frontier label CAN be derived;
`frontier-roster-derives-every-label` shows that EVERY one of the sixteen is. Read
them in that order.

`frontier-roster-derives-every-label` is the closure argument. Its data world
asserts no `logic:entryLabel` either, and its sixteen entries — one per
`logic:FrontierLabel` individual — produce all sixteen labels from the sixteen
shipped `logic:Rule` instances its profile names. A roster that enumerates sixteen positions and can
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
witnesses, and the five shipped `logic:Rule` instances its profile names compute the
label. Its golden shows five derived:

```text
readyEntry      StepReady + ApprovalNull     -> FrontierReadyAuthorized
gatedEntry      StepReady + ApprovalCreated  -> FrontierReadyApprovalRequired
unknownEntry    EffectAttempted              -> FrontierReconciliationRequired
receiptedEntry  EffectReceipted              -> FrontierCompensationEligible
blockedEntry    StepWaiting + a gap on its   -> FrontierBlockedCapabilityOrResource
                action step
```

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

## The scenario cases, and what each one derives

The remaining seven are scenario A-Boxes. They pin the closure and the projection
surface for their shapes, and each one derives its own answer:

| case | derives |
| --- | --- |
| `cc1-continuing-cluster` | the week's six review actions under five distinct labels, plus the approval obstruction standing on the publish step. The two review actions exist because the snapshot delta named two inputs, so "what to do about what changed" follows from the delta rather than from a reviewer's judgement. The pair of goal evaluations beside them is what keeps the cluster open: a maintenance goal judged against one good week is Satisfied and UNDETERMINED, and only a Satisfied-and-Completed evaluation licenses a conclusive `gmeow:satisfiedBy` edge. |
| `cc2-capability-present` | the five frontier labels, the ordered walk of the method's `rdf:List` into its steps, the task's expansion and reachability closure, and the approval obstruction on the one gated step. Policy-maximal automation is a consequence of the axis tuples here, not a claim about them. |
| `cc3-capability-absent` | the blocked-on-capability label AND the typed refinement rejection naming the missing capability — the second is the conclusion an operator acts on, because it says WHICH capability is absent. `FrontierWaitingOnEventTimeOrInput` is absent from the golden with all sixteen frontier rules loaded, because a step blocked by a capability the deployment does not have awaits nothing. |
| `cc4-crash-at-effect-boundary` | reconciliation-required from the attempted position, and reconciliation-IMPOSSIBLE from the abandoned one. One axis position apart the licensed action changes from probe to escalate; and with every rule loaded, the golden's silence about retry and compensation is the no-blind-retry law made observable. |
| `cc5-exact-approval-binding` | which of the run's two dispatch intents is gated. The commitment binds one digest and the neighbour intent is present rather than imagined, so "authorizes ONE intent" is checkable against a second candidate. |
| `cc6-contextual-recommendation` | the conflicting classification: the same roster is ready-and-authorized from one standpoint and mutually-exclusive from another. The strict order, the tie and the incomparability are authored with the shipped preference apparatus rather than described in comments. |
| `budget-cut-frontier-not-closed` | two of six labels, and stops. See below. |

## The budget cut

`budget-cut-frontier-not-closed` is the one case whose golden records a run that did
NOT finish. Its frontier carries six entries and no labels; six labels cannot be
reached in the two rule firings its profile allows, so the cut is produced by the
rule set rather than narrated. The golden records `budget_status: exhausted`,
`incomplete: true`, `strata_completed: 0/1`, `logic:entryLabel` absent from the
saturated set, and the two labels the run did reach stamped `exhausted` rather than
`ok` in `quad-status.json`.

What keeps that honest is WHICH predicates the frontier's `logic:SaturationWitness`
names. A witness certifies exactly the predicates it lists; this one lists the roster
membership, the axis positions and the actions — all asserted, hence final — and
deliberately omits `logic:entryLabel`, so nothing in the record claims the labels are
complete. The witness cannot instead record the cut on itself: a frontier whose
witness carries `logic:BudgetExhausted` is the shipped required-negative
(`slices/grounding/logic/tests/counter-examples/frontier-closed-on-a-budget-cut-witness.ttl`),
because a roster claiming closure over a search its own evidence says was truncated
is worse than an unwitnessed one. The cut is recorded where it belongs, on a
`logic:ReasoningResult` for the run.
