# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""The agentic extension (#390): tool-call provenance, gated.

TBox structural assertions (ToolCall idiom, property ends/functionality, and
the no-forward-output closed-set sweep) have been migrated to the declarative
slicetest DSL in slices/extensions/agentic/tests/structural.ttl (#867).

Retained here: the example competency query, and all runtime/Memory/MCP
integration tests that are not expressible as module-scoped SPARQL ASK cells.

Migrated to crates/validate/tests/conformance_agentic.rs (#867):
  - test_double_valued_toolcall_violates_the_closed_world_twins
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest
from gmeow_rdf.compat.rdflib import Graph, Namespace

from gmeow_tools.graph import load_merged_graph

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
EX = Namespace("https://blackcatinformatics.ca/gmeow/examples/")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# Competency — the example trajectory answers the issue's question
# --------------------------------------------------------------------------- #


def test_example_answers_which_tool_under_which_invocation() -> None:
    """'Which tool produced this entity, called by which invocation, with
    what arguments?' — answerable from the worked example alone (#390)."""
    from gmeow_tools.config import PROJECT_ROOT

    g = _graph()
    example = (
        PROJECT_ROOT / "slices" / "extensions" / "agentic" / "examples"
    ) / "agent-trajectory.ttl"
    g.parse(example, format="turtle")
    rows = list(
        g.query(
            """
            PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
            SELECT ?tool ?invocation ?args WHERE {
                ?entity gmeow:wasGeneratedBy ?call .
                ?call a gmeow:ToolCall ;
                      gmeow:usedTool ?tool ;
                      gmeow:calledByInvocation ?invocation ;
                      gmeow:toolArguments ?args .
            }
            """,
            initBindings={"entity": EX["note-2200"]},
        )
    )
    assert len(rows) == 1
    tool, invocation, args = rows[0]
    assert tool == EX.storeClaim
    assert invocation == EX["invocation-7"]
    assert "GTS spec" in str(args)


# --------------------------------------------------------------------------- #
# The first live producer — the memory layer and the MCP triad dogfood
# --------------------------------------------------------------------------- #


def test_memory_records_and_reads_tool_calls(tmp_path: Path) -> None:
    from gts.examples.agent_memory import Memory

    mem = Memory(tmp_path / "m.gts")
    claim = mem.store("the spec mandates deterministic encoding")
    record = mem.record_tool_call(
        "urn:gmeow:tool:store_claim",
        arguments='{"text": "the spec mandates deterministic encoding"}',
        result=claim.id,
        invocation="urn:gmeow:invocation:turn-7",
        generated=(claim.id,),
    )
    calls = mem.tool_calls()
    assert [c.id for c in calls] == [record.id]
    assert calls[0].tool == "urn:gmeow:tool:store_claim"
    assert calls[0].invocation == "urn:gmeow:invocation:turn-7"
    assert calls[0].generated == (claim.id,)
    # Claims and recall are untouched by the provenance records.
    assert [c.text for c in mem.claims()] == [claim.text]
    assert mem.verify() == []


def test_memory_applies_the_verbatim_or_digest_doctrine(tmp_path: Path) -> None:
    from gts.examples.agent_memory import Memory

    mem = Memory(tmp_path / "m.gts")
    big = "x" * 5000
    record = mem.record_tool_call("urn:gmeow:tool:write_file", arguments=big)
    assert record.arguments is not None
    assert record.arguments.startswith("blake3:")
    small = mem.record_tool_call("urn:gmeow:tool:write_file", arguments="tiny")
    assert small.arguments == "tiny"


def test_mcp_triad_is_the_first_live_producer(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    """Dogfood (#390): store/revise record themselves; recall records
    nothing (read path); the stored claim links back via wasGeneratedBy."""
    from gts.examples.agent_memory import Memory

    from gmeow_tools.mcp_server import recall, revise_belief, store_claim

    monkeypatch.setenv("GMEOW_MEMORY_PATH", str(tmp_path / "memory.gts"))
    stored = json.loads(store_claim("tool calls are provenance"))
    assert stored["ok"], stored
    claim_id = stored["claim"]["id"]

    assert json.loads(recall("provenance"))["ok"]
    assert json.loads(revise_belief(claim_id, reason="superseded test"))["ok"]

    calls = Memory(tmp_path / "memory.gts").tool_calls()
    assert [c.tool for c in calls] == [
        "urn:gmeow:tool:store_claim",
        "urn:gmeow:tool:revise_belief",
    ]
    assert calls[0].generated == (claim_id,)
    # toolResult is the VERBATIM payload the tool returned, byte-faithful.
    result = json.loads(calls[0].result or "{}")
    assert result["ok"] is True
    assert result["claim"]["id"] == claim_id
    args = json.loads(calls[0].arguments or "{}")
    assert args["text"] == "tool calls are provenance"
