# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""The MCP grounded-memory triad (#297, D2).

The D2 gate, in the #282 canary idiom but over the SERVER functions:
**suppression is honored on every recall path** — a revised claim never
surfaces through any default recall, while its control twin always does, so
the conformance can never pass vacuously.
"""

from __future__ import annotations

import asyncio
import json
from pathlib import Path

import pytest

from gmeow_tools import mcp_server
from gmeow_tools.mcp_server import mcp, recall, revise_belief, store_claim

_CANARY = "SUPPRESSED-CANARY belief about the launch window"
_CONTROL = "CONTROL-CANARY belief about the launch window"


@pytest.fixture
def memory_path(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    path = tmp_path / "memory.gts"
    monkeypatch.setenv("GMEOW_MEMORY_PATH", str(path))
    return path


def _store(text: str, **kwargs: object) -> dict[str, object]:
    payload = json.loads(store_claim(text, **kwargs))  # type: ignore[arg-type]
    assert payload["ok"], payload
    claim = payload["claim"]
    assert isinstance(claim, dict)
    return claim


def test_store_recall_revise_round_trip(memory_path: Path) -> None:
    claim = _store(
        "Patrick prefers explicit error handling",
        source="conversation",
        confidence=0.8,
        according_to="claude-fable-5",
    )
    assert claim["confidence"] == 0.8
    assert claim["suppressed"] is False

    recalled = json.loads(recall("error handling"))
    assert recalled["ok"]
    assert [c["id"] for c in recalled["claims"]] == [claim["id"]]

    revised = json.loads(revise_belief(str(claim["id"]), reason="superseded"))
    assert revised["ok"] and revised["suppressed"] == claim["id"]
    assert memory_path.exists()


def test_suppression_honored_on_every_recall_path(memory_path: Path) -> None:
    """The D2 gate: no default recall path ever surfaces a revised claim."""
    canary = _store(_CANARY, confidence=0.9)
    _store(_CONTROL, confidence=0.9)
    json.loads(revise_belief(str(canary["id"]), reason="revised"))

    recall_paths: dict[str, dict[str, object]] = {
        "empty-query": {},
        "matching-query": {"query": "launch window"},
        "exact-words": {"query": "SUPPRESSED-CANARY belief"},
        "confidence-filter": {"query": "launch", "min_confidence": 0.5},
        "high-limit": {"query": "", "limit": 100},
    }
    for name, kwargs in recall_paths.items():
        payload = json.loads(recall(**kwargs))  # type: ignore[arg-type]
        texts = [c["text"] for c in payload["claims"]]
        assert _CANARY not in texts, f"recall path {name!r} leaked a suppressed claim"
        assert _CONTROL in texts, f"recall path {name!r} lost the control (vacuous)"


def test_audit_view_recovers_the_suppressed_claim(memory_path: Path) -> None:
    canary = _store(_CANARY)
    json.loads(revise_belief(str(canary["id"])))
    audit = json.loads(recall("launch window", include_suppressed=True))
    flags = {c["text"]: c["suppressed"] for c in audit["claims"]}
    assert flags[_CANARY] is True  # visible AND labeled — never silently


def test_supersession_links_the_successor(memory_path: Path) -> None:
    old = _store("the launch is in June")
    new = _store("the launch slipped to July")
    revised = json.loads(
        revise_belief(
            str(old["id"]), reason="schedule change", superseded_by=str(new["id"])
        )
    )
    assert revised["superseded_by"] == new["id"]
    remaining = json.loads(recall("launch"))
    assert [c["id"] for c in remaining["claims"]] == [new["id"]]


def test_invalid_input_returns_ok_false_never_raises(memory_path: Path) -> None:
    assert json.loads(store_claim(""))["ok"] is False
    assert json.loads(store_claim("x", confidence=1.5))["ok"] is False
    assert json.loads(store_claim("x", confidence=float("nan")))["ok"] is False


def test_memory_persists_across_server_restarts(memory_path: Path) -> None:
    """A new Memory over the same file sees everything (append-only GTS)."""
    claim = _store("durable belief")
    # Simulate a restart: the helper builds a fresh Memory per call already,
    # so a second read-side call IS a restart; verify against the client too.
    from gmeow import Memory

    direct = Memory(memory_path)
    assert [c.id for c in direct.claims()] == [claim["id"]]
    assert direct.verify() == []


def test_triad_is_registered_on_the_server() -> None:
    tools = {t.name for t in asyncio.run(mcp.list_tools())}
    assert {"store_claim", "recall", "revise_belief"} <= tools


def test_default_memory_path_is_under_home(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    monkeypatch.delenv("GMEOW_MEMORY_PATH", raising=False)
    monkeypatch.setattr(Path, "home", staticmethod(lambda: tmp_path))
    memory = mcp_server._memory()
    assert str(tmp_path / ".gmeow" / "memory.gts") in str(memory._path)
