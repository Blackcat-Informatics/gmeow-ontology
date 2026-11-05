<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# The Agentic slice — the agent's actions as auditable provenance

The graphrag extension made the *pipeline* auditable; this core slice makes the *agent's
actions* auditable. A tool call is an event in the same provenance graph as
the claims it produces: `gmeow:ToolCall` follows the ModelInvocation idiom exactly — a
`gufo:EventType` under `gmeow:Activity` with one functional agent link and verbatim call
payloads — so "which tool, called by which invocation, with what arguments, at what
time?" is answerable after the fact, with no new provenance machinery (Principle 4).

The named consumer is the agentic runtime's tool-call mode (Principle 15), and the first live
producer ships in this repo: the gmeow memory MCP triad records its own `store_claim`
and `revise_belief` calls as ToolCall provenance, so the P14 memory flagship's actions
are themselves grounded memory.

The core surface covers tool-call provenance, the attention-request lifecycle used by
memetics, and behavioral drive and call-mode specifications. Calls and attention
requests retain their original records and actors rather than collapsing events
into a mutable status; drive specifications retain their actual rule and expression
sources.

## The call event

### gmeow:ToolCall

One invocation of a tool by an agent — the ModelInvocation idiom one level down. A
`gufo:EventType` under `gmeow:Activity`, so the temporal slice's clocks (`gmeow:atTime`)
and the provenance slice's lineage (`gmeow:wasGeneratedBy`, `gmeow:wasAttributedTo`)
apply unchanged. The doctrine that matters most is what ToolCall does **not** have: a
forward output entity property. An entity the call produced — a stored claim, a written
file, a minted node — links *back* via `gmeow:wasGeneratedBy` (Principle 5), exactly as
a generated claim links back to its `gmeow:ModelInvocation`. A multi-tool turn is
several ToolCalls, never one fat one (closed-world `sh:maxCount` twins gate every
functional property).

### gmeow:calledByInvocation

The model invocation that requested the call — the seam joining the action trajectory to
the generation trajectory. Functional, and deliberately **optional**: a recording
harness may not expose the invocation, and a ToolCall without one is still auditable
provenance. The call's result feeding a *later* invocation is temporal ordering
(`gmeow:atTime`), not a TBox link.

### gmeow:usedTool

The tool that was called, mirroring `gmeow:usedModel`. The range is `gmeow:SoftwareAgent`
and there is deliberately **no Tool subclass**: tool-ness is the role the agent plays in
this call event, not an identity (roles classify, never reify — the Persona lesson). An
MCP tool, a search service, and a code runner are SoftwareAgents that happen to be
called. One tool per call, functional.

## The payloads

### gmeow:toolArguments · gmeow:toolResult

The optional opaque literal slots record what the tool received and returned.
A recording profile may supply its explicitly declared digest locator instead of
inline content. Such a profile-specific string is not the generic
`gmeow:contentDigest` contract. Each slot admits at most one value and requires
none. The payload describes event content; an entity produced by the call still
links back through `gmeow:wasGeneratedBy` (Principle 5).

### gmeow:toolArgumentsStructured · gmeow:toolResultStructured

Each optional companion links at most one `StructuredValue` occurrence from
record-metadata. Structured arguments and results can therefore use native RDF
without an opaque JSON object literal. A value is a native scalar term, labelled
map, ordered sequence or explicit null. Typed empty containers, an empty literal,
null and absence remain distinct. Maps retain repeated keys as separate facets;
source map order is optional. Sequence occurrences require numeric ordinals.
Literal lexical form, datatype, language and direction remain intact, as do
IRI, blank-node and triple-term identity.

For opaque media, put the supplied literal in `structuredTerm` and the existing
`mediaType` and `charset` metadata on the linked payload occurrence. The arguments
and result roles bind media metadata separately; one media type on the whole call
would not identify which payload it described. The companion can coexist with an
opaque slot, but neither relation asserts that the representations match or
executes a conversion. These companions describe payloads rather than generated
output artifacts (Principles 2, 4, 5 and 17).

The existing generic RDF query and verification surfaces accept these graph
assertions. This vocabulary does not widen the fixed memory audit DTOs or promise
that their string-based arguments/result fields persist arbitrary structured
subgraphs.

## Cross-slice bridges

- **ai** — `gmeow:calledByInvocation` targets `gmeow:ModelInvocation`; a claim's
  `gmeow:wasGeneratedBy` chain now reaches through the invocation *and* the calls it
  requested.
- **provenance** — produced entities link back via `gmeow:wasGeneratedBy`; agent
  responsibility rides `gmeow:wasAttributedTo`, unchanged.
- **record-metadata** — structured payload occurrences reuse labelled facets,
  native terms and owner-local ordering.
- **documents** — existing media identity and charset describe each payload
  occurrence through the shared record-metadata dependency.
- **temporal** — `gmeow:atTime` orders calls within a turn; no sequence machinery is
  minted here.

## Alignments

`gmeow:ToolCall` is `prov:Activity` by inheritance through `gmeow:Activity` (the
provenance alignment set). Wikidata's nearest stable entity is the *mechanism*, not the
event (`wd:Q62270`, remote procedure call — relatedMatch). OpenAI function-calling,
Anthropic `tool_use`, and MCP tool schemas are JSON specifications with no stable RDF
namespace: bridging them is a projection concern (the Workflow Run Crate worked example and the
OpenLineage precedent), recorded as REFUSED cells in the mapping trailer rather than
papered over.

## Dogfood — the whole MCP tool surface as action schemas

`examples/mcp-action-policy.ttl` models the **real** tools the production native Rust
MCP server (`crates/mcp`) exposes as action schemas under the canonical-process action
theory — the dogfood case where the process model governs the agent's own tool calls
under explicit **capability / precondition / compensation** (rollback).

It is **total**, not a sample: every tool the consumer server advertises has exactly
one row, and every row names exactly one advertised tool. The correspondence is carried
by **`logic:mcpToolName`**, an asserted wire name on each schema, and is checked in both
directions by a gate in `crates/mcp`. A schema's local name is an *ontology* name chosen
for what the action **is** — `ex:persistConjecture` is the tool `store_conjecture`,
`ex:withdrawConjecture` is `refute_conjecture` — so the wire name has to be asserted
rather than mangled out of the local name.

The partition is read off the **asserted** types:

- **6 governed writes** (`logic:McpActionSchema`) — `store_claim` ⇄ `revise_belief`,
  `store_conjecture` ⇄ `refute_conjecture`, `submit_candidate` ⇄ `withdraw_candidate`.
  Each pair is its own compensation: the rollback is supersession, never erasure (P10).
- **31 reads** (plain `logic:ActionSchema`) — capability + precondition, but **no**
  `logic:effect` and **no** `logic:compensation`, because a read changes no state.
  `conjecture_test` is the instructive one: it runs the same isolated-world evaluation
  as `store_conjecture` but never commits, so evaluation is not commitment and it stays
  a plain schema.

Typing the write tools `logic:McpActionSchema` opts them into the stricter
`logic:McpActionPolicyShape` (capability + precondition + compensation each required);
the reads stay plain `logic:ActionSchema`, where the compensation obligation would be
vacuous. `logic:McpActionSchema` is a **subclass** of `logic:ActionSchema`, so the
read/write split is a statement about what is *asserted*, not about disjointness — and
the projection the engine reads (and the `action_policy` tool serves) is an
asserted-quad projection. The companion Rust evaluator
(`crates/logic/src/teleology.rs`, `gate_action`) exercises the same `store_claim`
pattern: **admit** when the precondition holds and the memory-write capability is
available, **deny** (carrying `revise_belief` as the rollback) when either is missing.

## Attention requests and their event history

Memetics records a request raised by a dream report as `AttentionRequest`, an
InformationObject, rather than a special waking-only class. Its words use
`description`, its raising record uses `wasDerivedFrom`, its author uses
`wasAttributedTo`, and its known request instant uses `attentionRequestedAt`.
Do not pad an unknown/coarse time into this instant property. Dream occurrence,
request creation, acknowledgement and resolution are different times.

`AttentionAcknowledgement` and `AttentionResolution` are Activity events, with
`wasAssociatedWith` for their actors and `eventTime` for known instants. They link
back through `acknowledges` and `resolves`. Acknowledgement is receipt, not completion.
A resolution carries `resolutionOutcome`, `resolutionRationale` and links to offered
records through `resolutionEvidence`. All three open outcome values are provided:
`resolutionAddressed`, `resolutionDeferred` and `resolutionDismissed`. Deferral and
dismissal are explicit dispositions, never inferred from missing replies.

The [lifecycle example](examples/attention-lifecycle.ttl) preserves a deferred event
and a later addressed event for the same request, plus a separately dismissed
request. There is no mutable winning status, forced last-write choice, logical
EvaluationStatus alias or new general ticket framework. The rationale and evidence
remain attributable to the resolving actor; recording a disposition does not
certify that the underlying concern was solved or erase the original request.

## Attention request wording

`attentionMessage` preserves the source message on an AttentionRequest as
xsd:string, rdf:langString or rdf:dirLangString. Preserve exact wording and any
supplied language or base direction; do not guess an absent language. Recording
the message does not acknowledge the request, resolve it or perform the
requested work. Existing request acknowledgment and resolution events retain
their own semantics (Principles 2, 3, 4 and 17).

The dreaming extension owns `requestedByDream`, its derivation link to an
actual DreamReport recollection episode. Core agentic has no dependency on that
extension. The extension's ordinary
[`goal-attempt-satisfaction.ttl`](../../extensions/dreaming/examples/goal-attempt-satisfaction.ttl)
scene shows the message alongside a dream-derived request, a separate
intention and source-attributed goal-evaluation evidence.

## A general path for stated rationale

resolutionRationale specializes core record-metadata's statedRationale while
retaining its AttentionResolution Event domain. General record reasons use
statedRationale directly. Neither path proves a cause or turns its Event into
an Entity through description. Source attribution belongs to the original
statement, including when the general relation is inferred from specialization.

## Complete drive specifications and activity call modes

`DriveDefinition` is an InformationObject specifying a behavioral drive. Its
six required source components are one actual `driveAccumulationProgram`, one
or more `relievingActivityDescription` values, one `driveCircadianExpression`,
one `driveModeProgram`, one `driveLevelDimension` and one
`driveCircadianDimension`. The two programs reuse actual
`logic:ReasoningProgram` clauses, query and selected evaluation mode. Productive
rules use Logic's existing Formula/Horn machinery; a name, empty clause set or
integrity Constraint cannot replace a rule body (Principles 4 and 17).

The definition is neither a physiological state nor proof of a computed level.
Sleep pressure or hunger names supply no default scale, initial zero, decay,
reset, saturation, coefficient, stage boundary or medical calibration. The
actual clauses state how history, relief and the circadian quantity combine.
The circadian and level quantities retain their actual Math dimensions; those
dimensions need not match, and a direct addition is not inferred.

`DriveStage` and `CallMode` are open named QualityValues. A stage may have
several legitimate `stageOfDrive` memberships. Every selected invocation must
positively bind its exact definition membership, without equating or capping
other memberships. Neither labels nor equal lexical names infer severity,
ordering, thresholds or identity (Principles 9 and 11). CallMode is separate
from ontological `logic:Mode`, interruptibility and task status.

### Actual history and anticipated activities

The history input identifies an actual InformationObject source record. Its
nonfunctional `driveHistoryClause` links select actual atomic Formula facts.
Order-independent histories stay unordered; partial orders retain their
actual supplied precedence facts. Supplied sequences and repeated references
retain their actual source structures and record identities. Identical RDF
facts do not become repeated event occurrences merely because a reader sees
them twice. A rule needing multiplicity must receive the actual source
occurrence/count structure. No list, total order, snapshot, complete archive or
closed-world absence is invented. An actual empty history supplies no initial
zero; its admitted rule must state the necessary initial semantics.

Relieving activity specifications and the mode-selection activity input are
actual `EventDescription` resources. EventDescription is an InformationObject
disjoint from Event. A description neither asserts that an activity occurred
nor creates a future occurrence. Actual historical Event facts and their
source-provided connection to the relieving descriptions remain distinct.
The clauses must explicitly state that matching and its relief effect; no
permission, automatic subtraction or reset follows from the relief link.

### Structured circadian expressions

The selected circadian expression is a concrete Math AST, using actual
ApplicationExpression/ArgumentSlot order, SymbolReference resolution and
binding/declaration rules where applicable. A formula string, abstract type
or function name alone is insufficient. The reified relation
`driveCircadianInput(definition, evaluationInstant, symbol, quantity)` binds
actual mathematical symbols used by that expression to supplied Quantities.
Bindings are scoped by the actual definition and evaluation instant. Every
selected external symbol requires exactly one binding; unused bindings are
rejected by the selected consumer rather than silently ignored.

The caller supplies the actual evaluation Instant with its temporal frame and
the mathematical time/phase quantities the expression requires. It must retain
the actual source connection between that instant and those quantities. The
vocabulary does not invent a coordinate, epoch, unit, period, amplitude, phase
or daily cosine, or convert an original duration into seconds.
`driveCircadianResult(definition, evaluationInstant, circadianQuantity)`
identifies the result at that same instant. An asserted result atom alone
does not demonstrate evaluation or correct dimensions.

### Exact program-query roles

The result relations are reified `logic:Type` individuals used as
`logic:relation` values on actual Formula atoms, with indexed TermCarriers:

```text
driveAccumulationResult(definition, history, evaluationInstant,
                        circadianQuantity, levelQuantity, stage)
activityCallModeResult(definition, activityDescription, stage, draw, mode)
```

The accumulation query has four inputs and two distinct output-variable
names. The mode query has four inputs and one output-variable name. Definition
position zero is a genuine template variable or an exact ground IRI binding
to the linked definition. Shared reusable programs are therefore possible;
anonymous definitions use a template variable because Logic's IRI-constant
carrier admits IRIs only. Every selected runtime binding must still identify
the actual selected definition. Program clauses and the query have independent
variable scopes; their relation/argument roles establish the connection,
not equal variable-name strings across scopes.

Resource input positions admit actual IRI carriers or template variables.
The draw retains its original supported IRI or literal carrier, or remains an
explicit template variable. Unsupported supplied RDF term forms must be
refused rather than coerced. Output names retain Logic's actual admitted
untagged, directionless string/token lexical identity. Equal accepted lexical
names identify the same parsed variable even across those two datatypes.

The single `driveModeProgram` supplies both the drive's call-mode effect and
the activity/stage/draw mapping. All selected inputs remain bound and
preserved. A branch returning a constant mode is valid; output variation with
the draw is not required. Consuming a supplied draw neither generates it nor
asserts a probability distribution. Separately selected generation or replay
requires its actual algorithm, seed and other declared inputs; no PRNG,
datatype, draw interval, seed width or distribution is assumed.

### Canonical laws and runtime boundaries

The three canonical Constraints inspect actual binary Formula/TermCarrier
metadata: `DriveProgramQueryConstraint` requires the exact relations, argument
positions and carrier roles; `DriveGroundInputConstraint` requires the actual
ground resource types, circadian dimension and positive selected stage
membership; `DriveHistoryClauseConstraint` restricts selected history roots to
actual facts. Required paths and supplied-value closures are authored in
`logic:`. A well-formed signature is a declaration, not an available
caller-input injection executor or a proof that its program was run.

For each selected admitted invocation, completed accumulation must return one
distinct level/stage pair and completed mode selection one distinct named
mode. Multiple proofs of the identical answer are not distinct results.
No answer, multiple distinct answers, undefined well-founded status,
unsupported constructs or incomplete evaluation must fail explicitly. Never
select the first clause, invent a default mode, retry the draw, omit a required
circadian term or use a weaker evaluator. This obligation applies to that
selected invocation; it does not claim static decidability of arbitrary
recursive program totality or termination (Principles 7, 12 and 19).

The current backward ReasoningProgram path evaluates its own authored clauses
and query; these declarations establish no caller-supplied fact/binding
injection API or numeric built-in dispatch for a named guard. The current
exact scalar Math evaluator admits dimensionless binary arithmetic; it does
not establish arbitrary circadian operators or time-dimensional execution.
Actual history application, admitted arithmetic/comparison, expression
evaluation, runtime input binding, dimensions and single-answer completion
remain required consumer acceptance. Source metadata and unexecuted laws are
not runtime receipts, computed scheduler eligibility or frontier witnesses.

Recurring daily windows, minimum useful span and typed-duration bounds have
their own owners and acceptance. This source vocabulary supplies none of
their recurrence expansion, now-inside evaluation, remaining-span comparison
or boundary interpretation. It changes neither existing location requirements
nor interruptibility declarations.
