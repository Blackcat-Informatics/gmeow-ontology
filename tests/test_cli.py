"""Tests for CLI command behaviour."""

from __future__ import annotations

import tomllib
from pathlib import Path
from typing import Any
from unittest.mock import MagicMock, patch

import httpx
import pytest
from rdflib import Graph, Literal, Namespace, URIRef
from rdflib.namespace import DCTERMS, OWL, RDF, RDFS, SKOS
from typer.testing import CliRunner

from gmeow_tools.cli import app as public_app
from gmeow_tools.cli_dev import app as dev_app
from gmeow_tools.config import GTS_SNAPSHOT_FILE, NAMESPACE, ONTOLOGY_IRI
from gmeow_tools.gts_producer import compile_gts


@pytest.fixture
def runner() -> CliRunner:
    return CliRunner()


def _multilingual_gts(
    tmp_path: Path, *, include_fr: bool = True, include_zh: bool = True
) -> Path:
    """Build a controlled, byte-deterministic multilingual GTS fixture."""
    graph = Graph()
    gm = Namespace(NAMESPACE)
    ontology = URIRef(ONTOLOGY_IRI)
    graph.add((ontology, RDF.type, OWL.Ontology))
    graph.add(
        (ontology, DCTERMS.title, Literal("Fixture ontology", lang="x-gmeow-english"))
    )
    graph.add((ontology, OWL.versionInfo, Literal("0.0.0")))
    term = gm.SampleTerm
    graph.add((term, RDF.type, OWL.Class))
    graph.add((term, RDFS.label, Literal("sample label", lang="x-gmeow-english")))
    graph.add(
        (
            term,
            SKOS.definition,
            Literal("English definition text.", lang="x-gmeow-english"),
        )
    )
    if include_fr:
        graph.add(
            (
                term,
                RDFS.label,
                Literal("étiquette échantillon", lang="x-gmeow-french"),
            )
        )
        graph.add(
            (
                term,
                SKOS.definition,
                Literal("Définition en français.", lang="x-gmeow-french"),
            )
        )
    if include_zh:
        graph.add((term, RDFS.label, Literal("样本标签", lang="x-gmeow-mandarin")))
        graph.add(
            (
                term,
                SKOS.definition,
                Literal("中文定义。", lang="x-gmeow-mandarin"),
            )
        )
    graph.add((term, RDFS.isDefinedBy, URIRef(NAMESPACE + "slices/lifecycle")))
    graph.add((gm.langEnglish, RDF.type, gm.Language))
    graph.add((gm.langEnglish, gm.languageTag, Literal("x-gmeow-english")))
    graph.add((gm.langEnglish, gm.bcp47Tag, Literal("en")))
    graph.add((gm.langEnglish, RDFS.label, Literal("English", lang="x-gmeow-english")))
    graph.add((gm.langFrench, RDF.type, gm.Language))
    graph.add((gm.langFrench, gm.languageTag, Literal("x-gmeow-french")))
    graph.add((gm.langFrench, gm.bcp47Tag, Literal("fr")))
    graph.add((gm.langFrench, RDFS.label, Literal("français", lang="x-gmeow-french")))
    graph.add((gm.langMandarin, RDF.type, gm.Language))
    graph.add((gm.langMandarin, gm.languageTag, Literal("x-gmeow-mandarin")))
    graph.add((gm.langMandarin, gm.bcp47Tag, Literal("zh")))
    if include_zh:
        graph.add(
            (gm.langMandarin, RDFS.label, Literal("中文", lang="x-gmeow-mandarin"))
        )
    fixture = tmp_path / "fixture.gts"
    fixture.write_bytes(compile_gts(graph))
    return fixture


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


def test_describe_renders_french(runner: CliRunner, tmp_path: Path) -> None:
    """A fixture with French labels renders them without an English fallback marker."""
    fixture = _multilingual_gts(tmp_path)
    result = runner.invoke(
        public_app, ["describe", "SampleTerm", "--lang", "fr", "--gts", str(fixture)]
    )
    assert result.exit_code == 0, result.output
    assert "Définition en français" in result.output
    assert "fallback: en" not in result.output


def test_describe_renders_mandarin(runner: CliRunner, tmp_path: Path) -> None:
    """A fixture with Mandarin labels renders them without a fallback marker."""
    fixture = _multilingual_gts(tmp_path)
    result = runner.invoke(
        public_app, ["describe", "SampleTerm", "--lang", "zh", "--gts", str(fixture)]
    )
    assert result.exit_code == 0, result.output
    assert "中文定义" in result.output
    assert "fallback: en" not in result.output


def test_describe_unknown_language_error_is_content_aware(
    runner: CliRunner, tmp_path: Path
) -> None:
    """When content is limited, the error list does not advertise the full catalog."""
    fixture = _multilingual_gts(tmp_path, include_fr=True, include_zh=False)
    result = runner.invoke(
        public_app,
        ["describe", "SampleTerm", "--lang", "notatag", "--gts", str(fixture)],
    )
    assert result.exit_code != 0
    assert "Available languages: en, fr" in result.output
    assert "zh" not in result.output


def test_describe_fallback_marker_for_missing_language(
    runner: CliRunner, tmp_path: Path
) -> None:
    """An English-only fixture falls back when French is requested."""
    fixture = _multilingual_gts(tmp_path, include_fr=False, include_zh=False)
    result = runner.invoke(
        public_app,
        ["describe", "SampleTerm", "--lang", "fr", "--gts", str(fixture)],
    )
    assert result.exit_code == 0, result.output
    assert "fallback: en" in result.output


def test_describe_env_language_rejected_if_unknown(runner: CliRunner) -> None:
    with patch.dict("os.environ", {"GMEOW_LANG": "notatag"}):
        result = runner.invoke(public_app, ["describe", "Person"])
    assert result.exit_code != 0
    assert "unknown language tag" in result.output.lower()


def test_describe_explicit_empty_lang_overrides_env(
    runner: CliRunner, tmp_path: Path
) -> None:
    """--lang '' wins over GMEOW_LANG and selects the default English carrier."""
    fixture = _multilingual_gts(tmp_path)
    with patch.dict("os.environ", {"GMEOW_LANG": "fr"}):
        result = runner.invoke(
            public_app,
            ["describe", "SampleTerm", "--lang", "", "--gts", str(fixture)],
        )
    assert result.exit_code == 0, result.output
    assert "fallback: en" not in result.output
    assert "English definition text" in result.output


def test_describe_env_empty_lang_defaults_to_english(
    runner: CliRunner, tmp_path: Path
) -> None:
    """An empty GMEOW_LANG env value maps to the default English carrier."""
    fixture = _multilingual_gts(tmp_path)
    with patch.dict("os.environ", {"GMEOW_LANG": ""}):
        result = runner.invoke(
            public_app, ["describe", "SampleTerm", "--gts", str(fixture)]
        )
    assert result.exit_code == 0, result.output
    assert "English definition text" in result.output


def test_export_respects_language_selector(runner: CliRunner, tmp_path: Path) -> None:
    out = tmp_path / "export"
    result = runner.invoke(public_app, ["export", "--out", str(out), "--lang", "fr"])
    assert result.exit_code == 0, result.output
    classes_csv = out / "gmeow-classes.csv"
    assert classes_csv.exists()
    text = classes_csv.read_text(encoding="utf-8")
    assert "label_fr" in text
    assert "label_fallback" in text


def test_export_lang_flag_wins_over_env(runner: CliRunner, tmp_path: Path) -> None:
    """--lang wins over GMEOW_LANG when exporting CSVs."""
    out = tmp_path / "export"
    with patch.dict("os.environ", {"GMEOW_LANG": "en"}):
        result = runner.invoke(
            public_app, ["export", "--out", str(out), "--lang", "fr"]
        )
    assert result.exit_code == 0, result.output
    classes_csv = out / "gmeow-classes.csv"
    assert classes_csv.exists()
    header = classes_csv.read_text(encoding="utf-8").splitlines()[0]
    assert "label_fr" in header
    assert "label_en" not in header


def test_create_docs_language_fallback(runner: CliRunner, tmp_path: Path) -> None:
    fixture = _multilingual_gts(tmp_path, include_fr=False, include_zh=False)
    out = tmp_path / "docs-tree"
    result = runner.invoke(
        public_app,
        ["docs", "--directory", str(out), "--lang", "fr", str(fixture)],
    )
    assert result.exit_code == 0, result.output
    sample_file = out / "terms" / "classes" / "gmeow-SampleTerm.md"
    assert sample_file.exists()
    text = sample_file.read_text(encoding="utf-8")
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
    assert "Graph Transport Substrate" in result.output


@patch("gmeow_tools.cli.shutil.which", return_value=None)
def test_gts_shim_fails_when_binary_missing(_mock: Any, runner: CliRunner) -> None:
    result = runner.invoke(public_app, ["gts", "info"])
    assert result.exit_code != 0
    assert "gts binary not found" in result.output


@patch("gmeow_tools.cli.subprocess.run")
@patch("gmeow_tools.cli.shutil.which", return_value="/fake/gts")
def test_gts_shim_injects_snapshot_for_default_subcommands(
    _which: Any, mock_run: Any, runner: CliRunner
) -> None:
    mock_run.return_value.returncode = 0
    result = runner.invoke(public_app, ["gts", "info"])
    assert result.exit_code == 0
    mock_run.assert_called_once_with(
        ["/fake/gts", "info", str(GTS_SNAPSHOT_FILE)], check=False
    )


@patch("gmeow_tools.cli.subprocess.run")
@patch("gmeow_tools.cli.shutil.which", return_value="/fake/gts")
def test_gts_shim_forwards_explicit_file(
    _which: Any, mock_run: Any, runner: CliRunner
) -> None:
    mock_run.return_value.returncode = 0
    result = runner.invoke(public_app, ["gts", "info", "myfile.gts"])
    assert result.exit_code == 0
    mock_run.assert_called_once_with(["/fake/gts", "info", "myfile.gts"], check=False)


@patch("gmeow_tools.cli.subprocess.run")
@patch("gmeow_tools.cli.shutil.which", return_value="/fake/gts")
def test_gts_shim_forwards_non_default_command(
    _which: Any, mock_run: Any, runner: CliRunner
) -> None:
    mock_run.return_value.returncode = 0
    result = runner.invoke(public_app, ["gts", "compile"])
    assert result.exit_code == 0
    mock_run.assert_called_once_with(["/fake/gts", "compile"], check=False)


@patch("gmeow_tools.cli.subprocess.run")
@patch("gmeow_tools.cli.shutil.which", return_value="/fake/gts")
def test_gts_shim_runs_help_when_no_args(
    _which: Any, mock_run: Any, runner: CliRunner
) -> None:
    mock_run.return_value.returncode = 0
    result = runner.invoke(public_app, ["gts"])
    assert result.exit_code == 0
    mock_run.assert_called_once_with(["/fake/gts", "--help"], check=False)


@patch("gmeow_tools.cli.subprocess.run")
@patch("gmeow_tools.cli.shutil.which", return_value="/fake/gts")
def test_gts_shim_injects_snapshot_before_flags(
    _which: Any, mock_run: Any, runner: CliRunner
) -> None:
    mock_run.return_value.returncode = 0
    result = runner.invoke(public_app, ["gts", "info", "--json"])
    assert result.exit_code == 0
    mock_run.assert_called_once_with(
        ["/fake/gts", "info", str(GTS_SNAPSHOT_FILE), "--json"], check=False
    )


@patch("gmeow_tools.cli.subprocess.run")
@patch("gmeow_tools.cli.shutil.which", return_value="/fake/gts")
def test_gts_shim_does_not_inject_when_file_follows_flags(
    _which: Any, mock_run: Any, runner: CliRunner
) -> None:
    mock_run.return_value.returncode = 0
    result = runner.invoke(public_app, ["gts", "info", "--json", "myfile.gts"])
    assert result.exit_code == 0
    mock_run.assert_called_once_with(
        ["/fake/gts", "info", "--json", "myfile.gts"], check=False
    )


@patch("gmeow_tools.cli.subprocess.run")
@patch("gmeow_tools.cli.shutil.which", return_value="/fake/gts")
def test_gts_shim_does_not_inject_after_double_dash(
    _which: Any, mock_run: Any, runner: CliRunner
) -> None:
    mock_run.return_value.returncode = 0
    # Typer strips the "--" separator before it reaches ctx.args, so the
    # forwarded call does not contain it; the important behaviour is that the
    # file after the separator is recognised and no snapshot is injected.
    result = runner.invoke(public_app, ["gts", "info", "--", "myfile.gts"])
    assert result.exit_code == 0
    mock_run.assert_called_once_with(["/fake/gts", "info", "myfile.gts"], check=False)


@patch("gmeow_tools.cli.subprocess.run")
@patch("gmeow_tools.cli.shutil.which", return_value="/fake/gts")
def test_gts_shim_injects_snapshot_for_extract_key(
    _which: Any, mock_run: Any, runner: CliRunner
) -> None:
    mock_run.return_value.returncode = 0
    result = runner.invoke(public_app, ["gts", "extract-key"])
    assert result.exit_code == 0
    mock_run.assert_called_once_with(
        ["/fake/gts", "extract-key", str(GTS_SNAPSHOT_FILE)], check=False
    )


@patch("gmeow_tools.cli.subprocess.run")
@patch("gmeow_tools.cli.shutil.which", return_value="/fake/gts")
def test_gts_shim_does_not_inject_for_extract_key_with_file(
    _which: Any, mock_run: Any, runner: CliRunner
) -> None:
    mock_run.return_value.returncode = 0
    result = runner.invoke(public_app, ["gts", "extract-key", "myfile.gts"])
    assert result.exit_code == 0
    mock_run.assert_called_once_with(
        ["/fake/gts", "extract-key", "myfile.gts"], check=False
    )


@patch("gmeow_tools.cli.subprocess.run", side_effect=OSError("permission denied"))
@patch("gmeow_tools.cli.shutil.which", return_value="/fake/gts")
def test_gts_shim_handles_os_error(
    _which: Any, _mock_run: Any, runner: CliRunner
) -> None:
    result = runner.invoke(public_app, ["gts", "info"])
    assert result.exit_code != 0
    assert "failed to run gts" in result.output


def test_dev_cli_keeps_checkout_commands(runner: CliRunner) -> None:
    result = runner.invoke(dev_app, ["--help"])
    assert result.exit_code == 0
    assert "regenerate" in result.output
    assert "quality" in result.output
    assert "validate" in result.output


def test_dev_cli_has_compile_gts_commands(runner: CliRunner) -> None:
    result = runner.invoke(dev_app, ["--help"])
    assert result.exit_code == 0
    assert "compile-gts" in result.output
    assert "compile-gts-full" in result.output


def test_dev_i18n_help_lists_sync_english(runner: CliRunner) -> None:
    result = runner.invoke(dev_app, ["i18n", "--help"])
    assert result.exit_code == 0, result.output
    assert "sync-english" in result.output


def test_dev_i18n_sync_english_dry_run(runner: CliRunner) -> None:
    result = runner.invoke(dev_app, ["i18n", "sync-english", "--dry-run"])
    assert result.exit_code == 0, result.output


def test_dev_i18n_extract(runner: CliRunner, tmp_path: Path) -> None:
    out = tmp_path / "i18n"
    result = runner.invoke(dev_app, ["i18n", "extract", "--output-dir", str(out)])
    assert result.exit_code == 0, result.output
    pot = out / "slices" / "core" / "lifecycle.pot"
    assert pot.exists(), result.output
    text = pot.read_text(encoding="utf-8")
    assert (
        'msgctxt "https://blackcatinformatics.ca/gmeow/hasCreationEvent|'
        'http://www.w3.org/2000/01/rdf-schema#label"'
    ) in text


def test_dev_i18n_extract_produces_docs_pot_files(
    runner: CliRunner, tmp_path: Path
) -> None:
    out = tmp_path / "i18n"
    result = runner.invoke(dev_app, ["i18n", "extract", "--output-dir", str(out)])
    assert result.exit_code == 0, result.output
    assert (out / "ontology-docs-templates.pot").exists(), result.output
    readme_pot = out / "docs" / "README.md.pot"
    assert readme_pot.exists(), result.output
    assert 'msgctxt "README.md|' in readme_pot.read_text(encoding="utf-8")


def test_dev_i18n_extract_lang_includes_language_tag_in_paths(
    runner: CliRunner, tmp_path: Path
) -> None:
    out = tmp_path / "i18n"
    result = runner.invoke(
        dev_app, ["i18n", "extract", "--output-dir", str(out), "--lang", "fr"]
    )
    assert result.exit_code == 0, result.output
    po = out / "slices" / "core" / "lifecycle" / "i18n" / "fr.po"
    assert po.exists(), result.output
    assert '"Language: fr\\n"' in po.read_text(encoding="utf-8")
    assert (out / "ontology-docs-templates.fr.po").exists(), result.output
    readme_po = out / "docs" / "README.md.fr.po"
    assert readme_po.exists(), result.output
    assert 'msgctxt "README.md|' in readme_po.read_text(encoding="utf-8")


def test_dev_i18n_extract_terms_only_skips_docs(
    runner: CliRunner, tmp_path: Path
) -> None:
    out = tmp_path / "i18n"
    result = runner.invoke(
        dev_app, ["i18n", "extract", "--output-dir", str(out), "--terms-only"]
    )
    assert result.exit_code == 0, result.output
    assert not (out / "docs").exists()
    assert not (out / "ontology-docs-templates.pot").exists()


def test_dev_i18n_merge_outputs_multilingual_graph(
    runner: CliRunner, tmp_path: Path
) -> None:
    out = tmp_path / "merged.ttl"
    result = runner.invoke(dev_app, ["i18n", "merge", "--output", str(out)])
    assert result.exit_code == 0, result.output
    assert out.exists()
    text = out.read_text(encoding="utf-8")
    assert "Existence d'entité" in text
    assert "PO file(s)" in result.output


def _write_test_po(
    path: Path,
    language: str,
    entries: list[tuple[str, str, str, bool]],
) -> None:
    """Write a minimal PO catalog for export tests."""
    lines = [
        'msgid ""',
        'msgstr ""',
        f'"Language: {language}\\n"',
        '"MIME-Version: 1.0\\n"',
        '"Content-Type: text/plain; charset=UTF-8\\n"',
        '"Content-Transfer-Encoding: 8bit\\n"',
        "",
    ]
    for msgctxt, msgid, msgstr, fuzzy in entries:
        if fuzzy:
            lines.append("#, fuzzy")
        lines.append(f'msgctxt "{msgctxt}"')
        lines.append(f'msgid "{msgid}"')
        lines.append(f'msgstr "{msgstr}"')
        lines.append("")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines), encoding="utf-8")


def test_dev_i18n_help_lists_export_commands(runner: CliRunner) -> None:
    result = runner.invoke(dev_app, ["i18n", "--help"])
    assert result.exit_code == 0, result.output
    assert "export-csv" in result.output
    assert "export-xliff" in result.output


def test_dev_i18n_export_csv_shape(runner: CliRunner, tmp_path: Path) -> None:
    po = tmp_path / "slices" / "core" / "testslice" / "i18n" / "fr.po"
    _write_test_po(
        po,
        "fr",
        [
            ("http://example.org/Term|rdfs:label", "Term", "Terme", False),
            (
                "http://example.org/Term|skos:definition",
                "A term.",
                "Un terme.",
                True,
            ),
        ],
    )
    result = runner.invoke(dev_app, ["i18n", "export-csv", "--root", str(tmp_path)])
    assert result.exit_code == 0, result.output
    lines = result.output.strip().splitlines()
    assert lines[0] == "slice,term_iri,predicate,language,msgid,msgstr,fuzzy"
    assert "testslice,http://example.org/Term,rdfs:label,fr,Term,Terme,false" in lines
    assert (
        "testslice,http://example.org/Term,skos:definition,fr,A term.,Un terme.,true"
        in lines
    )


def test_dev_i18n_export_csv_to_file(runner: CliRunner, tmp_path: Path) -> None:
    po = tmp_path / "slices" / "core" / "testslice" / "i18n" / "fr.po"
    _write_test_po(
        po,
        "fr",
        [("http://example.org/Term|rdfs:label", "Term", "Terme", False)],
    )
    out = tmp_path / "export.csv"
    result = runner.invoke(
        dev_app, ["i18n", "export-csv", "--root", str(tmp_path), "-o", str(out)]
    )
    assert result.exit_code == 0, result.output
    assert out.exists()
    text = out.read_text(encoding="utf-8")
    assert "slice,term_iri,predicate,language,msgid,msgstr,fuzzy" in text


def test_dev_i18n_export_xliff_shape(runner: CliRunner, tmp_path: Path) -> None:
    po = tmp_path / "slices" / "core" / "testslice" / "i18n" / "fr.po"
    _write_test_po(
        po,
        "fr",
        [("http://example.org/Term|rdfs:label", "Term", "Terme", False)],
    )
    result = runner.invoke(dev_app, ["i18n", "export-xliff", "--root", str(tmp_path)])
    assert result.exit_code == 0, result.output
    assert '<xliff version="1.2"' in result.output
    assert 'source-language="en"' in result.output
    assert 'target-language="fr"' in result.output
    assert '<file original="slices/core/testslice"' in result.output
    assert '<trans-unit id="http://example.org/Term|rdfs:label"' in result.output
    assert "<source>Term</source>" in result.output
    assert "<target>Terme</target>" in result.output
    assert "Term: http://example.org/Term Predicate: rdfs:label" in result.output


def test_dev_i18n_export_xliff_escapes_xml(runner: CliRunner, tmp_path: Path) -> None:
    po = tmp_path / "slices" / "core" / "testslice" / "i18n" / "fr.po"
    _write_test_po(
        po,
        "fr",
        [("http://example.org/Term|rdfs:label", "A & B", "A et B", False)],
    )
    result = runner.invoke(dev_app, ["i18n", "export-xliff", "--root", str(tmp_path)])
    assert result.exit_code == 0, result.output
    assert "<source>A &amp; B</source>" in result.output


def test_dev_i18n_export_xliff_to_file(runner: CliRunner, tmp_path: Path) -> None:
    po = tmp_path / "slices" / "core" / "testslice" / "i18n" / "fr.po"
    _write_test_po(
        po,
        "fr",
        [("http://example.org/Term|rdfs:label", "Term", "Terme", False)],
    )
    out = tmp_path / "export.xlf"
    result = runner.invoke(
        dev_app, ["i18n", "export-xliff", "--root", str(tmp_path), "-o", str(out)]
    )
    assert result.exit_code == 0, result.output
    assert out.exists()
    text = out.read_text(encoding="utf-8")
    assert '<trans-unit id="http://example.org/Term|rdfs:label"' in text


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
