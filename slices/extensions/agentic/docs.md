<!--
SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
SPDX-License-Identifier: CC-BY-4.0
-->

# The Agentic extension — the agent's actions as auditable provenance

The graphrag extension made the *pipeline* auditable; this slice makes the *agent's
actions* auditable. A tool call is an event in the same provenance graph as
the claims it produces: `gmeow:ToolCall` follows the ModelInvocation idiom exactly — a
`gufo:EventType` under `gmeow:Activity` with one functional agent link and verbatim call
payloads — so "which tool, called by which invocation, with what arguments, at what
time?" is answerable after the fact, with no new provenance machinery (Principle 4).

The named consumer is Project Lillith's agentic mode (Principle 15), and the first live
producer ships in this repo: the gmeow memory MCP triad records its own `store_claim`
and `revise_belief` calls as ToolCall provenance, so the P14 memory flagship's actions
are themselves grounded memory.

Deliberately thin: five terms. Trajectory aggregates (runs, episodes, plans) wait for a
consumer that needs them; growth requires a new consumer, not modeling pleasure.

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

The verbatim, byte-faithful payloads of the call — what was asked and what came back.
These record the **payload of the event**, never an entity link; `gmeow:toolResult` is
the JSON the tool returned, not the thing it created. For large payloads the
verbatim-or-digest doctrine applies: store a content digest literal (`"blake3:…"` — the
`gmeow:contentDigest` convention from the sources slice) instead of the bytes; the
digest is the value, resolvable from a content-addressed store, and the self-describing
prefix keeps the two forms distinguishable. At most one arguments record and at most one
result record per call — both optional (functional, with closed-world twins).

## Cross-slice bridges

- **ai** — `gmeow:calledByInvocation` targets `gmeow:ModelInvocation`; a claim's
  `gmeow:wasGeneratedBy` chain now reaches through the invocation *and* the calls it
  requested.
- **provenance** — produced entities link back via `gmeow:wasGeneratedBy`; agent
  responsibility rides `gmeow:wasAttributedTo`, unchanged.
- **sources** — the digest-literal convention for large payloads is
  `gmeow:contentDigest`'s (`"blake3:…"`, `"sha256:…"`), reused verbatim.
- **temporal** — `gmeow:atTime` orders calls within a turn; no sequence machinery is
  minted here.

## Alignments

`gmeow:ToolCall` is `prov:Activity` by inheritance through `gmeow:Activity` (the
provenance alignment set). Wikidata's nearest stable entity is the *mechanism*, not the
event (`wd:Q62270`, remote procedure call — relatedMatch). OpenAI function-calling,
Anthropic `tool_use`, and MCP tool schemas are JSON specifications with no stable RDF
namespace: bridging them is a projection concern (Lillith worked example Workflow Run Crate, the
OpenLineage precedent), recorded as REFUSED cells in the mapping trailer rather than
papered over.
