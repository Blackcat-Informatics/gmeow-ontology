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
