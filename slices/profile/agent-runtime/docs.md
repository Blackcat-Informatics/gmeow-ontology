<!-- SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca> -->
<!-- SPDX-License-Identifier: CC-BY-4.0 -->

# Agent-runtime — the AI-agent / MCP profile (a pure selection)

> **Slice:** `https://blackcatinformatics.ca/gmeow/slices/agent-runtime` · **tier: profile**
> A pure selection that **mints nothing** (Principle 16). No `module.ttl`, no minted terms.

The agent-runtime profile composes existing slices into one dependency-closed sub-ontology for the
AI-agent / MCP-developer audience — a durable subject inhabiting a served model, with its runtime
provenance, tool use, serving modes, and claims/memory, loaded as a unit. It selects via
`gmeow:sliceDependsOn`; the profiles stage emits the dependency closure as a dereferenceable, citable
composition at `generated/profiles/agent-runtime.ttl`.

## Why a profile TIER, not a new mint surface (Principle 16)

Principle 16 frames a small-core / extensions dichotomy; the profile tier is compliant precisely because
a profile **mints nothing** — it is a *projection view* (a Principle-4 selection over existing slices),
adding zero ontology mass to the core-plus-extensions surface. It uses the sanctioned tier machinery
(the manifest is the sole tier truth: a `gmeow:tierProfile` `SliceTier` individual + the relaxed manifest
shape), and the dependency DAG gate already permits `profile → core` and `profile → extension` while
forbidding the reverse. If you ask where this profile's minted terms live, the answer is *nowhere* — it
is mis-designed the moment it mints (`INHABITED-CONSUMER.md`).

## The selection

- **`core/inhabitation`** — the durable-subject topology (subject · host · tenure · continuity), the
  `TransferManifest` / `MemoryView` **memory** substrate, and (transitively) the `epistemics` /
  `standpoint` **claims** layer.
- **`extensions/model-serving`** — the served artifact · deployment · runtime execution · session, and
  the computed upper-projections that trace an output to its subject.
- **`extensions/agentic`** — the tool-call / trajectory layer.
- **`core/awareness`** — the serving modes (`modeOnlineInference` / `modeOfflineReplay` / `modeTraining`).

The design's "memory" and "claims" constituents are **not** separate slices — they are provided
transitively through the closure of the four selected slices, so the profile stays a pure selection
rather than inventing profile-local terms.

## What the profile proves (the cross-slice corpus)

Model-serving is standalone: its `gmeow:sessionSubjectStage` / `gmeow:sessionConfiguration` are
open-range, and its projection-agreement gate collapses only to the session's *subject stage*. It is
here, where `core/inhabitation` is also present, that the **true end-to-end** resolves: an output traces
through the de-conflation chain and onward via `gmeow:stageOfLineage` to the durable
`gmeow:SubjectLineage`, and the flat `gmeow:generatedForSubject` shortcut is proven equal to that full
collapse (`tests/competency.ttl`). The composition is not merely declared — it is exercised.

The same is true across the two SIBLING extensions. CQ5 — *was a tool call made through a passive
capability or delegated to another agent?* — is discriminated structurally, with no wrapper class: a
`gmeow:usedCapability` edge from an invocation / execution to a `logic:ActionSchema` is **passive** use,
while a `gmeow:ToolCall` whose `gmeow:usedTool` points to a distinct `gmeow:SoftwareAgent` is
**delegation**. The passive half is `extensions/model-serving`'s and the delegation half is
`extensions/agentic`'s, and Principle 16 forbids either from depending on the other — a competency query
that named the sibling's terms would BE that dependency, whichever lane it runs in. So the question is
askable only from here (`queries/competency/tool-usage.rq` over `examples/tool-usage.ttl`), which is
precisely a profile earning its selection.
