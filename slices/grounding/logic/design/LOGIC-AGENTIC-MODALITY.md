<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Agentic modality

This design composes the typed context algebra in [LOGIC-SEMANTICS.md](./LOGIC-SEMANTICS.md)
with [LOGIC-TELEOLOGY.md](./LOGIC-TELEOLOGY.md) and the finite journals in
[LOGIC-ENACTMENT.md](./LOGIC-ENACTMENT.md). Its semantic owner is the grounding kernel.
Domain slices name applications; OWL, SHACL, Datalog, and external protocol
notations are projections with explicit preservation or loss judgments
(Principles 4, 9, 17, 19).

## Evaluation context

An evaluation names its expression and an attributed context. The context pins
the world, standpoint, enactment occurrence, and the journal position being
examined. Selected deontic and protocol operations additionally require their
issuer, bearer, policy, protocol, and role bindings. A missing selected binding
is invalid input. It cannot become a universal standpoint, an implicit issuer,
an empty protocol, or permission by default.

These coordinates are distinct. Two enactments of the same prescription and
input snapshot remain distinct occurrences. Two assessments of one occurrence
under different issuers or standpoints remain distinct assessments. Evaluation
identities include the expression, all selected coordinates, the exact observed
journal prefix, and the reasoning contract. Execution timestamps are observations,
not identities of reproducible prescription artifacts.

## Expression and scope

The finite ground fragment contains atomic propositions, strong negation,
conjunction, disjunction, and necessity/possibility over one named typed
accessibility relation. Telic and protocol bindings scope an expression; they
neither assert that the goal occurred nor manufacture a protocol message.
Temporal expressions extend this same syntax with strong next, until,
eventually, and globally over a selected finite journal.

Nesting is ordered. Evaluating an outer context operator passes its resulting
context to the entire inner expression. It does not evaluate inner subexpressions
at the outermost world and then combine their labels. In particular, obligation
to eventually achieve a goal and eventual acquisition of an obligation are
different expressions.

The bare `logic:accessibleFrom` relation is never an inference relation. Every
modal step names one of the admitted typed relations. A world reached through
an epistemic edge does not become a deontically ideal world by virtue of being
reachable. A deontic frame with no admitted ideal world is incomplete; it cannot
establish every obligation by vacuity.

No cross-axis commutation is implicit. For two universal modalities, equality
of their finite composed accessibility relations can witness commutation for
that exact frame. It is not a context-independent law. Equality of the two
relation compositions alone does not license exchanging universal and
existential quantifiers. A projection that reorders modalities must carry a
preservation proof for the exact operators and scope it changes.

## Information and computation

Atomic evidence has independent positive and explicit negative support in its
selected context. The four information states are the existing supported,
opposed, both, and neither states. Missing positive evidence is not negative
evidence. Strong negation swaps the two support coordinates. Conjunction takes
the conjunction of positive support and the disjunction of negative support;
disjunction takes their dual. Contradiction remains local and disclosed.

Finite universal and existential evaluation lifts those operations over the
admitted context set. Claims of absence or completeness require the selected
context inventory to be closed. A missing or incomplete inventory cannot be
used to prove an absence. Unsupported syntax, exhausted execution budgets, and
cancellation remain computation outcomes, distinct from semantic disagreement
and missing evidence.

The RDF frontend admits at most 128 source-carrier nodes on a nested path and 65,536
expanded carrier occurrences for one formula before constructing the shared
boxed formula IR. Formula links, arguments, quantified-variable carriers, and
function applications all participate. A shared source child counts along every
incoming path. An iterative size pass measures this envelope without expanding
the source DAG. Exceeding it produces the typed
`logic-compile.formula-admission` incomplete-check diagnostic; compilation carries
it as `FORMULA_ADMISSION_EXHAUSTED`. No syntax, truth, or negation judgment follows
from this operational refusal. These compiled limits participate in the engine
contract separately from the formula/context evaluation step allowance.

Context declarations, formula ownership, and successor inventories are explicit
source inputs. Each typed evaluation request belongs to exactly one source
graph. Its formula carriers, contexts, and successor inventories are read from
that same graph; declarations in other graphs cannot fill missing fields or
introduce additional constructors. An IRI declared as a request in multiple
source graphs is ambiguous and is rejected. This source-graph identity enters
the assessment basis separately from the evidence worlds named by its contexts.

The native post-pass also reads already-derived `accordingTo`
and `standpointSupportStatus` metadata on the selected world's existing RDF 1.2
claims. It borrows the closure and retains only reached attribution receipts.
Every such receipt preserves its native rule, immediate premises, derivation
identifier, and world. A bare derived proposition supplies no attributed support.

Each conclusion carries its expression/context anchor, the rule that derived
it, and its actual antecedents through the existing provenance machinery.
Diagnostics enter `DiagLedger` with the assessment's standpoint. A diagnostic
about a norm violation does not assert an issuer-independent ought.

## Supplied-data assessment

`gmeow logic evaluate assessment.trig --request urn:example:query` selects one
request from caller-supplied RDF. For example, `assessment.trig` can contain:

```trig
@prefix logic: <https://blackcatinformatics.ca/logic/> .
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix ex: <urn:example:> .

ex:metadata {
  ex:view a logic:AttributedContext ; logic:contextWorld ex:world ;
    logic:contextStandpoint ex:author ; logic:evidenceClosure logic:ClosedWorldClosure .
  ex:query a logic:ContextualEvaluationRequest ;
    logic:queryFormula ex:formula ; logic:queryContext ex:view .
  ex:formula a logic:Formula ; logic:relation ex:ready ;
    logic:argument [ logic:termIndex 0 ; logic:termIri ex:task ],
                   [ logic:termIndex 1 ; logic:termIri ex:execution ] .
}
ex:world {
  ex:claim rdf:reifies <<( ex:task ex:ready ex:execution )>> ;
    gmeow:accordingTo ex:author ;
    gmeow:standpointSupportStatus gmeow:supportSupported .
}
```

The command emits an RDF 1.2 N-Quads assessment and proof in `graph/reasoning`,
with any fragment-refusal findings in `graph/diagnostics`. This input produces
supported information under `ex:view`; the output does not assert the quoted
proposition as an unconditional fact. `--max-steps N` bounds formula/context
evaluation steps. Exhaustion and unsupported syntax return a nonzero exit status;
an opposed or undetermined semantic result can still be a completed computation.

The repository's [nested contextual example](../examples/contextual-assessment.ttl)
is a canonical producer input. The producer assigns its statements to
`graph/examples`; that graph placement supplies its declared evidence world.
Its nested formula selects the reviewer with `inContext` before following the
reviewer's epistemic edges. Those edges retain the selected standpoint.
Reading the raw Turtle file alone does not reproduce the produced dataset's
named-graph placement.

## Finite temporal interpretation

A journal supplies a hash-verified order of committed operations. Attributed
context records supply the state observations at those positions; the transition
hash does not authenticate fields outside its defined recipe. At a position in
a finalized, nonempty journal:

| Expression | Finite interpretation |
| --- | --- |
| strong next φ | φ at the next position; false at the final position |
| φ until ψ | ψ at some current or later position, with φ at every earlier position |
| eventually φ | true until φ |
| globally φ | φ at every current or later position |

The classical two-valued restriction follows the formal clauses in
[De Giacomo and Vardi (2013), section 2](https://www.ijcai.org/Proceedings/13/Papers/132.pdf).
The independent evidence coordinates and open-prefix treatment below are this
kernel's declared extension. The general interpretation retains
independent support and counter-support. Temporal recurrence is evaluated over
the finite position order, without enumerating possible schedules or extending
the journal with invented events.

An unfinalized prefix has a pending future boundary. A witness already observed
can establish eventuality; an observed counterexample can refute a global
condition. Failure to observe an eventuality so far does not refute it.
Pending temporal judgments have their own trace-status evidence and are never
reported as budget exhaustion. Finalization is explicit evidence, not an
inference from the current absence of a next entry.

Achievement, maintenance, and avoidance goals are required to use this same temporal evaluator.
Deadline windows select positions using their declared time domain and boundary
inclusivity. Missing time observations do not silently become zero or wall time.
Optimization goals retain their measure/degree semantics; a degree never becomes
a Boolean truth value. Infinite-trace interpretation is outside this finite
journal contract. The retained boundary `logic:finite-journal-infinite-trace`
records that limit in the ontology and the native fragment registry. Its
`FirstOrder` classification describes the relational translation's expressibility;
it does not turn a finite observation into evidence about an infinite domain.

### Journal admission and observation identity

A selected journal declares `journalInitialHead`, every selected `journalEntry`,
its `journalHead` entry IRI, and `journalBoundary`. A nonempty inventory contains
at most 65,536 committed entries. Every entry retains its predecessor IRI and the
existing runtime tuple: previous head, delta identity, outcome tag, and new head.
Digests use `blake3:` followed by exactly 64 lowercase hexadecimal digits; outcome
tags use the canonical runtime outcome individuals. Admission rejects mismatched
hashes, missing links, cycles, forks, duplicate heads, disconnected entries, and
ambiguous enactment ownership. The admission envelope is separate from the
formula judgment-step budget.

Position zero identifies the first committed entry after the declared genesis
anchor. A context binds that position, its journal and enactment, world, and
standpoint. Each observed adjacency uses a closed, unique `ContextSuccessorSet`
over `temporallySucceeds`, advances exactly one journal position, and preserves
the remaining context coordinates. A context describing a planned future state
beyond the selected committed head cannot become an observed successor.

`ObservedTemporalPrefix` records the exact journal, enactment, genesis digest,
selected head entry and digest, and boundary consumed by an assessment. Its
content identity changes when the prefix or finalization evidence changes. The
proof cites both that record and the separately attributed state/transition
witnesses. An `ObservedPathPrefix` has a different kind and identity: it describes
an ordered state observation and carries no fabricated journal operations. The
supplied-RDF contextual adapter requires a committed journal for temporal
operators; a path observation cannot satisfy that journal admission contract.

The native path adapter admits one immutable observation and shares its attributed
evidence index and order witnesses across goal formulas. A prepared formula binds
that observation and its shared physical program; evaluation neither reparses the
dataset nor lowers the formula again. Each execution has fresh budget and proof
state. An altered world, standpoint, membership, closure or claim inventory needs
a separately admitted observation, so reuse cannot carry an old verdict into a
different evidential context.

The shared first-order translation uses `finiteNext`, `finiteAtOrAfter`, and
`finiteStrictlyBefore` as guarded position relations. Physical evaluation shares
the modal evidence algebra, judgment memo, cancellation signal, and budget.
Suffixes are evaluated backwards without recursion proportional to journal
length; a nested temporal node walks only as far as an already memoized suffix.
An incremental append monitor must additionally authenticate each extension and
preserve immutable past observations; memoization within one fixed input is not
that append contract.

## Norms, intentions, and protocols

A step norm binds a prescription, its issuer and bearer, its scope, and its
modality. Permission, prohibition, and obligation are independent declarations.
Contrary-to-duty activation requires the attributed triggering violation and
the declared reparation relationship. It never follows from an unrelated failure
or from absence of permission.

Intent composition retains every constituent commitment and its attribution.
Contextual priority is an explicitly scoped order; it is not a universal winner.
Revision creates a successor intention record and retains the prior record.
Delegation does not erase the delegator's obligations or transfer authority that
the protocol has not granted.

A choreography describes the interaction contract across roles. Role projection
must preserve each role's observable prerequisites and commitment transitions.
A role cannot be instructed to branch on an event it cannot observe. Unsupported
or unprojectable interactions produce a typed refusal and a loss record.
Hohfeldian powers change the normative state only when their declared authority
and triggering message evidence are present.

## Production boundary

Repository production carries contextual assessment records and their RDF 1.2
evidence directly in `graph/reasoning`, alongside the aggregate native result.
The aggregate also retains each record's exact derivation receipt. Its reader
uses the request-to-result links to distinguish contextual children from the
single aggregate handle, and refuses ambiguous result inventories. Typed literals,
recursive triple terms, reifiers and annotations survive artifact serialization
and named-graph relocation; the assessed proposition remains attributed.
The reasoned-graph verifier and validation-stage advisory union consume these
same typed derived terms through the shared closure converter. Neither consumer
interprets a literal or a quoted triple as an IRI.

The native reasoner evaluates the admitted expression program. Consumer commands
use the same evaluator over supplied RDF and embedded contracts. Production
scheduler adoption must compile its work plan from authored sources before
generated artifacts exist, then evaluate readiness and policy over observed
task completions. Scheduler adoption also requires execution monitoring, norm
evaluation, intent decisions, and protocol role checks.

Planning and reasoning never derive external-effect attempts, receipts, journal
observations, or host-lock ownership. The executing component records those
observations. Completed action receipts may be reused when their exact inputs
match; a test process only authenticates producer-selected receipts and cannot
construct the corpus.
