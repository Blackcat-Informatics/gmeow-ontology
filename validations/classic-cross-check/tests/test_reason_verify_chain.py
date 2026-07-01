"""Tests for the reason → verify chain, including the pre-reasoned fast path."""

from __future__ import annotations

import shutil
from collections.abc import Iterator
from pathlib import Path
from typing import Any
from unittest.mock import patch

import pytest

from gmeow_tools import reason as reason_mod
from gmeow_tools.config import PROJECT_ROOT


class _FakeCompletedProcess:
    """Minimal stand-in for ``subprocess.CompletedProcess``."""

    def __init__(self, stdout: str = "", stderr: str = "") -> None:
        self.stdout = stdout
        self.stderr = stderr


def _capture_robot_calls(calls: list[list[str]]) -> Any:
    """Return a patch target function that records args and returns OK."""

    def _fake(args: list[str], *, timeout: float = 900.0) -> _FakeCompletedProcess:
        calls.append(list(args))
        return _FakeCompletedProcess("ok")

    return _fake


def _dummy_ttl(root: Path, name: str = "merged.ttl") -> Path:
    """Create a dummy Turtle file under *root* and return its path."""
    path = root / name
    path.write_text("# dummy", encoding="utf-8")
    return path


@pytest.fixture
def reason_tmp(tmp_path: Path) -> Iterator[Path]:
    """Provide an isolated temp directory inside the project tree for ROBOT paths."""
    root = PROJECT_ROOT / ".gmeow-tmp-test-reason" / tmp_path.name
    root.mkdir(parents=True, exist_ok=True)
    yield root
    shutil.rmtree(root, ignore_errors=True)


def test_reason_passes_exclude_tautologies(reason_tmp: Path) -> None:
    robot_calls: list[list[str]] = []
    merged = _dummy_ttl(reason_tmp, "merged.ttl")

    fake_robot = _capture_robot_calls(robot_calls)
    with patch.object(reason_mod, "_robot", side_effect=fake_robot):
        reason_mod.reason("ELK", merged=merged, exclude_tautologies="structural")

    reason_call = next(c for c in robot_calls if c[0] == "reason")
    assert "--exclude-tautologies" in reason_call
    assert reason_call[reason_call.index("--exclude-tautologies") + 1] == "structural"


def test_verify_with_reasoned_input_uses_verify_only(reason_tmp: Path) -> None:
    robot_calls: list[list[str]] = []
    reasoned = _dummy_ttl(reason_tmp, "reasoned.ttl")

    fake_robot = _capture_robot_calls(robot_calls)
    with patch.object(reason_mod, "_robot", side_effect=fake_robot):
        reason_mod.verify(reasoner="ELK", reasoned=reasoned)

    assert len(robot_calls) == 1
    assert robot_calls[0][0] == "verify"
    assert "--input" in robot_calls[0]
    assert robot_calls[0][robot_calls[0].index("--input") + 1] == str(
        reasoned.resolve().relative_to(PROJECT_ROOT)
    )
    assert "--exclude-tautologies" not in robot_calls[0]


def test_verify_without_reasoned_chains_reason(reason_tmp: Path) -> None:
    robot_calls: list[list[str]] = []
    merged = _dummy_ttl(reason_tmp, "merged.ttl")

    fake_robot = _capture_robot_calls(robot_calls)
    with patch.object(reason_mod, "_robot", side_effect=fake_robot):
        reason_mod.verify(reasoner="ELK", merged=merged)

    assert robot_calls[0][0] == "reason"
    assert "--exclude-tautologies" in robot_calls[0]
    assert "verify" in robot_calls[0]
