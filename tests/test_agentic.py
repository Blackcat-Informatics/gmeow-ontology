# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""The agentic extension (#390): tool-call provenance, gated.

The agent's ACTIONS join the same provenance graph as its claims: ToolCall
follows the ModelInvocation idiom (EventType ⊑ Activity, functional agent
link, closed-world twins), produced entities link BACK via wasGeneratedBy
(P5), and the gmeow memory MCP triad is the first live producer — its own
store/revise calls are recorded and auditable.
"""

from __future__ import annotations

import json
from pathlib import Path

import pytest
from rdflib import OWL, RDF, RDFS, Graph, Namespace

from gmeow_tools.graph import load_merged_graph
from gmeow_tools.validate import run_shacl

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
GUFO = Namespace("http://purl.org/nemo/gufo#")
EX = Namespace("https://blackcatinformatics.ca/gmeow/examples/")


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


# --------------------------------------------------------------------------- #
# TBox — the ModelInvocation idiom one level down
# --------------------------------------------------------------------------- #


def test_toolcall_follows_the_modelinvocation_idiom() -> None:
    g = _graph()
    assert (GMEOW.ToolCall, RDF.type, OWL.Class) in g
    assert (GMEOW.ToolCall, RDF.type, GUFO.EventType) in g
    assert (GMEOW.ToolCall, RDFS.subClassOf, GMEOW.Activity) in g


def test_agentic_properties_have_declared_ends_and_functionality() -> None:
    g = _graph()
    expected = {
        GMEOW.calledByInvocation: (GMEOW.ToolCall, GMEOW.ModelInvocation),
        GMEOW.usedTool: (GMEOW.ToolCall, GMEOW.SoftwareAgent),
        GMEOW.toolArguments: (GMEOW.ToolCall, RDFS.Literal),
        GMEOW.toolResult: (GMEOW.ToolCall, RDFS.Literal),
    }
    for prop, (domain, range_) in expected.items():
        assert (prop, RDFS.domain, domain) in g, prop
        assert (prop, RDFS.range, range_) in g, prop
        # Every OWL-functional carries a closed-world twin (tested below).
        assert (prop, RDF.type, OWL.FunctionalProperty) in g, prop


def test_no_forward_output_entity_property_exists() -> None:
    """P5: produced entities link BACK via wasGeneratedBy — the agentic
    slice must never mint a forward output object property."""
    g = _graph()
    agentic = "https://blackcatinformatics.ca/gmeow/slices/agentic"
    for prop in g.subjects(RDFS.isDefinedBy, Namespace(agentic)[""]):
        if (prop, RDF.type, OWL.ObjectProperty) in g:
            assert str(prop) in (
                str(GMEOW.calledByInvocation),
                str(GMEOW.usedTool),
            ), f"unexpected object property in agentic slice: {prop}"


def test_double_valued_toolcall_violates_the_closed_world_twins() -> None:
    """The maxCount twins make a double-valued record a VIOLATION."""
    data = Graph()
    data.parse(
        data="""
        @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
        @prefix ex: <https://example.org/bad/> .
        ex:t1 a gmeow:SoftwareAgent . ex:t2 a gmeow:SoftwareAgent .
        ex:fat a gmeow:ToolCall ;
            gmeow:usedTool ex:t1, ex:t2 ;
            gmeow:toolArguments "a", "b" .
        """,
        format="turtle",
    )
    result = run_shacl(_graph() + data)
    text = "\n".join(result.errors)
    assert "several ToolCalls" in text or "usedTool" in text
    assert "arguments" in text.lower()


# --------------------------------------------------------------------------- #
# Competency — the example trajectory answers the issue's question
# --------------------------------------------------------------------------- #


def test_example_answers_which_tool_under_which_invocation() -> None:
    """'Which tool produced this entity, called by which invocation, with
    what arguments?' — answerable from the worked example alone (#390)."""
    g = _graph()
    example = Path("slices/extensions/agentic/examples/agent-trajectory.ttl")
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
    tool, invocation, args = rows[0]  # type: ignore[misc]
    assert tool == EX.storeClaim
    assert invocation == EX["invocation-7"]
    assert "GTS spec" in str(args)


# --------------------------------------------------------------------------- #
# The first live producer — the memory layer and the MCP triad dogfood
# --------------------------------------------------------------------------- #


def test_memory_records_and_reads_tool_calls(tmp_path: Path) -> None:
    from gmeow import Memory

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
    from gmeow import Memory

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
    from gmeow import Memory
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
    assert calls[0].result == claim_id
    args = json.loads(calls[0].arguments or "{}")
    assert args["text"] == "tool calls are provenance"
