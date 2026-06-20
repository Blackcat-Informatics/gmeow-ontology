# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""Tests for the shared diagnostics output config (#662)."""

from __future__ import annotations

from pathlib import Path

import pytest

from gmeow_tools.config import DIST_DIR
from gmeow_tools.diagnostics_config import ConsoleMode, DiagnosticsConfig


def test_defaults_resolve_with_no_flags_or_env() -> None:
    config = DiagnosticsConfig.resolve(env={}, is_tty=True)

    assert config.console is ConsoleMode.PRETTY
    assert config.artifacts == frozenset({"json", "sarif", "html"})
    assert config.directory == DIST_DIR
    assert config.stem == "gmeow-feedback"
    assert config.category == "gmeow"


@pytest.mark.parametrize(
    ("is_tty", "expected"),
    [(True, ConsoleMode.PRETTY), (False, ConsoleMode.TEXT)],
)
def test_auto_console_resolves_by_tty(is_tty: bool, expected: ConsoleMode) -> None:
    config = DiagnosticsConfig.resolve(console="auto", env={}, is_tty=is_tty)
    assert config.console is expected


def test_flag_beats_env_for_console() -> None:
    config = DiagnosticsConfig.resolve(
        console="pretty",
        env={"GMEOW_DIAGNOSTICS_CONSOLE": "silent"},
        is_tty=False,
    )
    assert config.console is ConsoleMode.PRETTY


def test_env_honored_when_no_flag() -> None:
    config = DiagnosticsConfig.resolve(
        env={"GMEOW_DIAGNOSTICS_CONSOLE": "jsonl"}, is_tty=True
    )
    assert config.console is ConsoleMode.JSONL


@pytest.mark.parametrize(
    ("flag", "env", "expected"),
    [
        ("foo", {}, "foo"),
        (None, {"GMEOW_DIAGNOSTICS_STEM": "bar"}, "bar"),
        ("foo", {"GMEOW_DIAGNOSTICS_STEM": "bar"}, "foo"),
        (None, {}, "gmeow-feedback"),
    ],
)
def test_stem_precedence(flag: str | None, env: dict[str, str], expected: str) -> None:
    config = DiagnosticsConfig.resolve(stem=flag, env=env, is_tty=True)
    assert config.stem == expected


@pytest.mark.parametrize(
    ("flag", "env", "expected"),
    [
        ("lint", {}, "lint"),
        (None, {"GMEOW_DIAGNOSTICS_CATEGORY": "rust"}, "rust"),
        ("lint", {"GMEOW_DIAGNOSTICS_CATEGORY": "rust"}, "lint"),
        (None, {}, "gmeow"),
    ],
)
def test_category_precedence(
    flag: str | None, env: dict[str, str], expected: str
) -> None:
    config = DiagnosticsConfig.resolve(category=flag, env=env, is_tty=True)
    assert config.category == expected


@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        ("all", frozenset({"json", "sarif", "html"})),
        ("none", frozenset()),
        ("json,sarif", frozenset({"json", "sarif"})),
        ("sarif,json", frozenset({"json", "sarif"})),  # order-independent
        ("HTML", frozenset({"html"})),  # case-insensitive
    ],
)
def test_artifacts_parsing(raw: str, expected: frozenset[str]) -> None:
    config = DiagnosticsConfig.resolve(artifacts=raw, env={}, is_tty=True)
    assert config.artifacts == expected


@pytest.mark.parametrize("raw", ["json,xml", "pdf", "json,,sarif,bogus"])
def test_unknown_artifact_token_hard_fails(raw: str) -> None:
    with pytest.raises(ValueError, match="artifact kind"):
        DiagnosticsConfig.resolve(artifacts=raw, env={}, is_tty=True)


def test_invalid_console_token_hard_fails() -> None:
    with pytest.raises(ValueError):
        DiagnosticsConfig.resolve(console="loud", env={}, is_tty=True)


def test_directory_default_is_flat_dist_on_tty() -> None:
    config = DiagnosticsConfig.resolve(category="lint", env={}, is_tty=True)
    assert config.directory == DIST_DIR


def test_directory_default_is_category_scoped_off_tty() -> None:
    config = DiagnosticsConfig.resolve(category="lint", env={}, is_tty=False)
    assert config.directory == DIST_DIR / "diagnostics" / "lint"


def test_explicit_directory_flag_wins_in_both_modes(tmp_path: Path) -> None:
    for is_tty in (True, False):
        config = DiagnosticsConfig.resolve(
            directory=tmp_path, category="lint", env={}, is_tty=is_tty
        )
        assert config.directory == tmp_path


def test_env_directory_wins_over_default_off_tty(tmp_path: Path) -> None:
    config = DiagnosticsConfig.resolve(
        env={"GMEOW_DIAGNOSTICS_DIR": str(tmp_path)}, is_tty=False
    )
    assert config.directory == tmp_path


def test_is_tty_none_falls_back_to_stderr_isatty(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    import sys

    monkeypatch.setattr(sys.stderr, "isatty", lambda: True, raising=False)
    config = DiagnosticsConfig.resolve(console="auto", env={})
    assert config.console is ConsoleMode.PRETTY

    monkeypatch.setattr(sys.stderr, "isatty", lambda: False, raising=False)
    config = DiagnosticsConfig.resolve(console="auto", env={})
    assert config.console is ConsoleMode.TEXT
