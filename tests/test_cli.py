"""Tests for CLI command behaviour."""

from __future__ import annotations

import tomllib
from pathlib import Path
from unittest.mock import MagicMock, patch

import httpx
import pytest
from typer.testing import CliRunner

from gmeow_tools.cli import app as public_app
from gmeow_tools.cli_dev import app as dev_app


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
        result = runner.invoke(dev_app, ["quality", "--strict"])
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
        result = runner.invoke(dev_app, ["quality"])
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
            dev_app, ["quality", "--foops-url", "http://example.org/onto", "--strict"]
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
            dev_app, ["quality", "--foops-url", "http://example.org/onto"]
        )
    assert result.exit_code == 0
    assert "FOOPS! skipped" in result.output


def test_create_docs_from_bundled_snapshot(runner: CliRunner, tmp_path: Path) -> None:
    out = tmp_path / "docs-tree"
    result = runner.invoke(public_app, ["docs", "--directory", str(out)])
    assert result.exit_code == 0, result.output
    assert (out / "index.md").exists()
    assert (out / "terms" / "classes").is_dir()
    assert (out / "terms" / "properties").is_dir()
    assert (out / "alignments.md").exists()
    assert (out / "statements.md").exists()


def test_describe_unknown_language_fails(runner: CliRunner) -> None:
    result = runner.invoke(public_app, ["describe", "Person", "--lang", "notatag"])
    assert result.exit_code != 0
    assert "unknown language tag" in result.output.lower()
    assert "Available languages" in result.output


def test_describe_fallback_marker_for_missing_language(runner: CliRunner) -> None:
    """The bundled snapshot only carries English, so a French request falls back."""
    result = runner.invoke(public_app, ["describe", "Person", "--lang", "fr"])
    assert result.exit_code == 0, result.output
    assert "fallback: en" in result.output


def test_describe_env_language_rejected_if_unknown(runner: CliRunner) -> None:
    with patch.dict("os.environ", {"GMEOW_LANG": "notatag"}):
        result = runner.invoke(public_app, ["describe", "Person"])
    assert result.exit_code != 0
    assert "unknown language tag" in result.output.lower()


def test_export_respects_language_selector(runner: CliRunner, tmp_path: Path) -> None:
    out = tmp_path / "export"
    result = runner.invoke(public_app, ["export", "--out", str(out), "--lang", "fr"])
    assert result.exit_code == 0, result.output
    classes_csv = out / "gmeow-classes.csv"
    assert classes_csv.exists()
    text = classes_csv.read_text(encoding="utf-8")
    assert "label_fr" in text
    assert "label_fallback" in text


def test_create_docs_language_fallback(runner: CliRunner, tmp_path: Path) -> None:
    out = tmp_path / "docs-tree"
    result = runner.invoke(
        public_app, ["docs", "--directory", str(out), "--lang", "fr"]
    )
    assert result.exit_code == 0, result.output
    person_file = out / "terms" / "classes" / "gmeow-Person.md"
    assert person_file.exists()
    text = person_file.read_text(encoding="utf-8")
    assert "[fallback: en]" in text


def test_public_cli_excludes_checkout_commands(runner: CliRunner) -> None:
    result = runner.invoke(public_app, ["--help"])
    assert result.exit_code == 0
    assert "verify" in result.output
    assert "regenerate" not in result.output
    assert "quality" not in result.output
    assert "validate" not in result.output


def test_public_gts_cli_excludes_compile_commands(runner: CliRunner) -> None:
    result = runner.invoke(public_app, ["gts", "--help"])
    assert result.exit_code == 0
    assert "compile-full" not in result.output
    assert "compile" not in result.output
    assert "verify" in result.output


def test_dev_cli_keeps_checkout_commands(runner: CliRunner) -> None:
    result = runner.invoke(dev_app, ["--help"])
    assert result.exit_code == 0
    assert "regenerate" in result.output
    assert "quality" in result.output
    assert "validate" in result.output


def test_dev_gts_cli_keeps_compile_commands(runner: CliRunner) -> None:
    result = runner.invoke(dev_app, ["gts", "--help"])
    assert result.exit_code == 0
    assert "compile" in result.output
    assert "compile-full" in result.output


def test_workspace_declares_separate_dev_package() -> None:
    root = Path(__file__).resolve().parents[1]
    main = tomllib.loads((root / "pyproject.toml").read_text(encoding="utf-8"))
    dev = tomllib.loads(
        (root / "packages" / "gmeow-dev" / "pyproject.toml").read_text(encoding="utf-8")
    )
    assert main["project"]["scripts"] == {
        "gmeow": "gmeow_tools.cli:app",
        "gmeow-music": "gmeow_tools.ext.music.cli:app",
    }
    assert "packages/gmeow-dev" in main["tool"]["uv"]["workspace"]["members"]
    assert dev["project"]["scripts"] == {"gmeow-dev": "gmeow_dev.cli:app"}
