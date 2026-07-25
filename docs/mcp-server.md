<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# The GMEOW MCP server

Native grounded agent memory as MCP tools — **store / recall / revise** (the
flagship, CONSTITUTION P14) — plus bundled ontology lookup. One config knob, no
checkout, no Docker, no RDF knowledge required (P13).

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

### `store_claim(text, source?, confidence?, according_to?, dry_run?)`

An LLM output is a **claim, not a truth** (P14): the text is appended as a
reified RDF 1.2 statement, attributed, optionally confidence-weighted
(`[0, 1]`) and standpoint-indexed (`according_to`). Two contradictory claims
coexist — store both; nothing adjudicates. The write **executes as a
Transaction-Logic transaction** (see *Transaction-Logic execution* below): the
executional-entailment verdict gates the commit, and the response carries the
`transaction` outcome.

```json
→ store_claim("Patrick prefers explicit error handling",
              source="conversation 2026-06-10",
              confidence=0.8, according_to="claude-fable-5")
← {"ok": true, "claim": {"id": "urn:gmeow:assertion:…", "text": "Patrick prefers explicit error handling",
   "confidence": 0.8, "according_to": "claude-fable-5",
   "source": "conversation 2026-06-10", "created": "2026-06-12T…", "suppressed": false},
   "transaction": {"committed": true, "succeeded": true, "path_len": 2}}
```

### `recall(query?, min_confidence?, limit?, include_suppressed?)`

Empty query returns the most recent claims; otherwise case-insensitive
token-overlap ranking. **Suppression is honored on every recall path**:
revised claims never surface by default. `include_suppressed=true` is the
audit view — each claim's `suppressed` flag says what you are looking at.

```json
→ recall("error handling", min_confidence=0.5)
← {"ok": true, "claims": [{"id": "urn:gmeow:assertion:…", "text": "Patrick prefers explicit error handling", …}]}
```

### `revise_belief(claim_id, reason?, superseded_by?, dry_run?)`

Belief revision is **suppression, never deletion** (P10): the claim is
retained under a suppression frame — the audit trail of what the agent
believed *when* survives — and recall stops returning it. `superseded_by`
links the successor claim into the derivation chain. `revise_belief` is
`store_claim`'s **compensation** — the rollback action — and it likewise
executes as a transaction whose precondition is that the target claim exists.

```json
→ revise_belief("urn:gmeow:assertion:…", reason="Patrick now prefers Result types")
← {"ok": true, "suppressed": "urn:gmeow:assertion:…", "superseded_by": null,
   "transaction": {"committed": true, "succeeded": true, "path_len": 2}}
```

The pattern in one breath: `store_claim` when you learn, `recall` before you
answer, `revise_belief` when you learn better — and the memory file remains a
portable, signed-able, independently verifiable record (`Memory.verify()` in
[`gts.examples.agent_memory`](https://github.com/Blackcat-Informatics/gmeow-gts/blob/main/python/src/gts/examples/agent_memory.py) reads the same file).

## Transaction-Logic execution

Each memory **write** (`store_claim`, `revise_belief`) runs through the native
Transaction-Logic engine before anything is persisted. The tool's action theory
— its precondition and effect — is the single authority; the engine's
**executional-entailment verdict** (does a path exist from the current state
along which the action holds?) is the commit gate, and the response's
`transaction` field reports it.

- **Commit.** When the precondition obtains, the action runs under
  *committed execution*: the effect is materialized and the claim is written.
  `store_claim`'s precondition is that the input is a well-formed claim;
  `revise_belief`'s is that the target claim exists — so an unknown `claim_id`
  fails the verdict and **nothing is written** (the pre-flight check, expressed
  as the transaction's own gate).
- **Rollback is compensation, not deletion.** `revise_belief` is
  `store_claim`'s compensation: a committed revise *suppresses* the stored claim
  (its support is retired by supersession) and the bytes are never erased
  (P10). Re-storing re-asserts it.
- **Dry run (sandbox).** Pass `dry_run=true` to any write to run the
  *hypothetical* operator: the same verdict is computed, the effect is
  **discarded**, and the response carries `{"dry_run": true, "transaction":
  {"committed": false, "succeeded": …, "witness": "…"}}`. Nothing is written —
  a content-addressed witness records that the sandbox run happened.

Every committed write also records its **audit context** onto the persisted
`gmeow:ToolCall` (the action schema it instantiated, its turn anchor and start
state, its timestamp and temporal frame), so a committed turn is afterwards
verifiable by the read-only trajectory audit over the same `memory.gts`.

## The bundled ontology tools

The public `gmeow mcp` server reads only the bundled `gmeow.gts` snapshot:

| Tool | What it does |
|---|---|
| `lookup_term(term, lang?)` | Resolve a CURIE, IRI, local name, or unambiguous prefix to label/definition/parents/alignments |
| `llms_txt(lang?)` | Return the standard bundled vocabulary index |
| `llms_full(lang?)` | Return the complete inlined bundled vocabulary index |
| `doc_card(term, lang?)` | Return a prompt-ready Markdown card for one term |
| `okf_index(lang?)` | Return the OKF manifest JSON envelope |
| `gmn_validate(gmn)` | Validate a GMN-1 document against the shipped codebook + validator tier; returns `{ok, conformant}` or the typed `lang:Gmn*Failure` class — the external LLM's entry to the GMN `@err` repair loop |
| `gmn_expand(gmn)` | Expand a GMN-1 document to its GMN-0 normal form (alias/glyph → full IRI) as canonical N-Quads, with an internal round-trip witness |
| `gmn_explain(glyph)` | Resolve a GMN operator glyph to its `lang:Denotation` + `gmnFixity`/`gmnPrecedence`/`gmnArity` and its controlled-NL gloss; an unknown glyph returns an honest typed miss |
| `store_claim(text, source?, confidence?, according_to?, dry_run?)` | Append one attributed memory claim (executed as a transaction; `dry_run` for a non-committing sandbox run) |
| `recall(query?, min_confidence?, limit?, include_suppressed?)` | Recall memory claims |
| `revise_belief(claim_id, reason?, superseded_by?, dry_run?)` | Suppress a stale claim without deletion (the `store_claim` compensation; `dry_run` for a non-committing sandbox run) |

Resources: `gmeow://ontology/llms.txt`, `gmeow://ontology/llms-full.txt`,
`gmeow://ontology/okf-index`, and `gmeow://ontology/gmn1-primer` (the ~500-token
graph-derived GMN-1 teachability primer, served off the bundle alone — the same
bytes folded into the `llms.txt` / `llms-full.txt` surfaces).

## Developer MCP

From a checkout, `cargo run -p gmeow-dev-cli -- mcp` exposes the
repo-maintenance server:

| Tool | What it does |
|---|---|
| `validate()` | Run the native validation/check surface |
| `sync()` | Update or strictly check generated artifacts |
| `reason()` | Run native reasoning over the bundled snapshot |
| `constitution()` | Read the checked-out GMEOW Constitution |
| `lookup_term(term, lang?)` | Resolve a term to label/definition/parents/alignments |
| `llms_txt(lang?)` | Return the standard bundled vocabulary index |
| `llms_full(lang?)` | Return the complete inlined bundled vocabulary index |
| `doc_card(term, lang?)` | Return a prompt-ready Markdown card for one term |
| `okf_index(lang?)` | Return the OKF manifest JSON envelope |

Resources: `gmeow://ontology/llms.txt`, `gmeow://ontology/llms-full.txt`,
`gmeow://ontology/okf-index`, and `gmeow://ontology/constitution`.

## Doctrine pointers

- Claims and their epistemics: [the AI claim module](../slices/core/ai/docs.md)
  and [the hallucination-resistant pattern](./hallucination-resistant-kg.md).
- The memory file format: [GTS-SPEC § 13 (`ai-package`)](./GTS-SPEC.md).
