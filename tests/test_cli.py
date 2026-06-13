"""Tests for CLI command behaviour."""

from __future__ import annotations

from pathlib import Path
from unittest.mock import MagicMock, patch

import httpx
import pytest
from typer.testing import CliRunner

from gmeow_tools.cli import app


@pytest.fixture
def runner() -> CliRunner:
    return CliRunner()


def test_quality_strict_fails_when_oops_raises(runner: CliRunner) -> None:
    mock_path = MagicMock(spec=Path)
    mock_path.read_text.return_value = ""
    with (
        patch("gmeow_tools.reason.merge_release", return_value=mock_path),
        patch(
            "gmeow_tools.quality.run_oops",
            side_effect=httpx.ConnectError("network down"),
        ),
    ):
        result = runner.invoke(app, ["quality", "--strict"])
    assert result.exit_code != 0
    assert "OOPS! failed" in result.output


def test_quality_best_effort_skips_when_oops_raises(runner: CliRunner) -> None:
    mock_path = MagicMock(spec=Path)
    mock_path.read_text.return_value = ""
    with (
        patch("gmeow_tools.reason.merge_release", return_value=mock_path),
        patch(
            "gmeow_tools.quality.run_oops",
            side_effect=httpx.ConnectError("network down"),
        ),
    ):
        result = runner.invoke(app, ["quality"])
    assert result.exit_code == 0
    assert "OOPS! skipped" in result.output


def test_quality_foops_strict_fails_when_foops_raises(runner: CliRunner) -> None:
    mock_path = MagicMock(spec=Path)
    mock_path.read_text.return_value = ""
    with (
        patch("gmeow_tools.reason.merge_release", return_value=mock_path),
        patch("gmeow_tools.quality.run_oops", return_value=""),
        patch(
            "gmeow_tools.quality.run_foops",
            side_effect=httpx.ConnectError("network down"),
        ),
    ):
        result = runner.invoke(
            app, ["quality", "--foops-url", "http://example.org/onto", "--strict"]
        )
    assert result.exit_code != 0
    assert "FOOPS! failed" in result.output


def test_quality_foops_best_effort_skips_when_foops_raises(
    runner: CliRunner,
) -> None:
    mock_path = MagicMock(spec=Path)
    mock_path.read_text.return_value = ""
    with (
        patch("gmeow_tools.reason.merge_release", return_value=mock_path),
        patch("gmeow_tools.quality.run_oops", return_value=""),
        patch(
            "gmeow_tools.quality.run_foops",
            side_effect=httpx.ConnectError("network down"),
        ),
    ):
        result = runner.invoke(
            app, ["quality", "--foops-url", "http://example.org/onto"]
        )
    assert result.exit_code == 0
    assert "FOOPS! skipped" in result.output


def test_create_docs_from_bundled_snapshot(runner: CliRunner, tmp_path: Path) -> None:
    out = tmp_path / "docs-tree"
    result = runner.invoke(app, ["create-docs", "--directory", str(out)])
    assert result.exit_code == 0, result.output
    assert (out / "index.md").exists()
    assert (out / "terms" / "classes").is_dir()
    assert (out / "terms" / "properties").is_dir()
    assert (out / "alignments.md").exists()
    assert (out / "statements.md").exists()
