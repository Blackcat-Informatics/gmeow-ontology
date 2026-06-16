<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The GMEOW MCP server

Grounded agent memory as MCP tools — **store / recall / revise** (the
flagship, CONSTITUTION P14) — plus bundled ontology lookup. One config knob,
no checkout, no Docker, no RDF knowledge required (P13).

## Install (one line)

```bash
claude mcp add gmeow -e GMEOW_MEMORY_PATH="$HOME/.gmeow/memory.gts" -- uv run gmeow mcp
```

or the equivalent `mcpServers` block (any MCP client):

```json
{
  "mcpServers": {
    "gmeow": {
      "command": "uv",
      "args": ["run", "gmeow", "mcp"],
      "env": { "GMEOW_MEMORY_PATH": "/home/you/.gmeow/memory.gts" }
    }
  }
}
```

`GMEOW_MEMORY_PATH` is the only configuration: the agent's memory lives there
as a **GTS ai-package** — a content-addressed, append-only, verifiable
single file that survives across sessions, models, and vendors. Default:
`~/.gmeow/memory.gts`.

## The grounded-memory triad

### `store_claim(text, source?, confidence?, according_to?)`

An LLM output is a **claim, not a truth** (P14): the text is appended as a
reified RDF 1.2 statement, attributed, optionally confidence-weighted
(`[0, 1]`) and standpoint-indexed (`according_to`). Two contradictory claims
coexist — store both; nothing adjudicates.

```json
→ store_claim("Patrick prefers explicit error handling",
              source="conversation 2026-06-10",
              confidence=0.8, according_to="claude-fable-5")
← {"ok": true, "claim": {"id": "urn:gmeow:assertion:…", "text": "Patrick prefers explicit error handling",
   "confidence": 0.8, "according_to": "claude-fable-5",
   "source": "conversation 2026-06-10", "created": "2026-06-12T…", "suppressed": false}}
```

### `recall_claims(query?, min_confidence?, limit?, include_suppressed?)`

Empty query returns the most recent claims; otherwise case-insensitive
token-overlap ranking. **Suppression is honored on every recall path**:
revised claims never surface by default. `include_suppressed=true` is the
audit view — each claim's `suppressed` flag says what you are looking at.

```json
→ recall_claims("error handling", min_confidence=0.5)
← {"ok": true, "claims": [{"id": "urn:gmeow:assertion:…", "text": "Patrick prefers explicit error handling", …}]}
```

### `revise_belief(claim_id, reason?, superseded_by?)`

Belief revision is **suppression, never deletion** (P10): the claim is
retained under a suppression frame — the audit trail of what the agent
believed *when* survives — and recall stops returning it. `superseded_by`
links the successor claim into the derivation chain.

```json
→ revise_belief("urn:gmeow:assertion:…", reason="Patrick now prefers Result types")
← {"ok": true, "suppressed": "urn:gmeow:assertion:…", "superseded_by": null}
```

The pattern in one breath: `store_claim` when you learn, `recall_claims` before
you answer, `revise_belief` when you learn better — and the memory file remains a
portable, signed-able, independently verifiable record (`Memory.verify()` in
[`gts.examples.agent_memory`](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/python/src/gts/examples/agent_memory.py) reads the same file).

## The bundled ontology tools

The public `gmeow mcp` server reads only the bundled `gmeow.gts` snapshot:

| Tool | What it does |
|---|---|
| `gmeow_lookup_term(term)` | Resolve a CURIE, IRI, local name, or unambiguous prefix to label/definition/parents/alignments |
| `store_claim(text, source?, confidence?, according_to?)` | Append one attributed memory claim |
| `recall_claims(query?, min_confidence?, limit?, include_suppressed?)` | Recall memory claims |
| `revise_belief(claim_id, reason?, superseded_by?)` | Suppress a stale claim without deletion |

Resources: `gmeow://ontology/llms.txt` (the flat vocabulary index).

## Developer MCP

From a checkout, `uv run --package gmeow-dev gmeow-dev mcp` exposes the
repo-maintenance server:

| Tool | What it does |
|---|---|
| `gmeow_validate()` | Turtle syntax + term annotations + SHACL over the ontology |
| `gmeow_regenerate(names?)` | Rebuild generated artifacts (dependency order) |
| `gmeow_check_generated(names?)` | Drift + orphan check for every registered generator |
| `gmeow_reason(reasoner?, profile?)` | ELK/HermiT consistency over the merged ontology |
| `gmeow_lookup_term(curie)` | Resolve a CURIE to label/definition/parents/alignments |

Resources: `gmeow://ontology/llms.txt` (the flat vocabulary index) and
`gmeow://ontology/constitution` (the sixteen principles).

## Doctrine pointers

- Claims and their epistemics: [the AI claim module](../slices/core/ai/docs.md)
  and [the hallucination-resistant pattern](./hallucination-resistant-kg.md).
- The memory file format: [GTS-SPEC § 13 (`ai-package`)](./GTS-SPEC.md).
