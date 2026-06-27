"""Tests for CLI command behaviour."""

from __future__ import annotations

import base64
import re
import tomllib
from pathlib import Path
from typing import Any
from unittest.mock import MagicMock, patch

import gts
import httpx
import pytest
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from gmeow_rdf.compat.rdflib import Graph, Literal, Namespace, URIRef
from gmeow_rdf.compat.rdflib.namespace import DCTERMS, OWL, RDF, RDFS, SKOS
from typer.testing import CliRunner

from gmeow_tools.cli import app as public_app
from gmeow_tools.cli_dev import app as dev_app
from gmeow_tools.config import GTS_SNAPSHOT_FILE, NAMESPACE, ONTOLOGY_IRI
from gmeow_tools.gts_producer import compile_gts

_GUFO = Namespace("http://purl.org/nemo/gufo#")

# Rich emits colour when CI forces it (FORCE_COLOR/CI), and its option highlighter
# styles each ``-`` run of a flag separately — so ``--gts`` is rendered as
# ``\x1b[..m-\x1b[0m\x1b[..m-gts\x1b[0m`` with an escape sequence *between the two
# dashes*. That makes the literal substring ``--gts`` absent under colour even
# though the flag is plainly there. Normalise the escapes before substring checks
# of flag names (CI forces colour; local dev shells usually do not).
_ANSI_RE = re.compile(r"\x1b\[[0-9;]*m")


def _strip_ansi(text: str) -> str:
    """Remove SGR colour escapes so flag-name substring checks are colour-stable."""
    return _ANSI_RE.sub("", text)


def _armor_public_key(raw_public: bytes) -> str:
    """Build an ASCII-armored OpenPGP v4 Ed25519 public-key certificate.

    The wire format matches the one GPG emits and the Rust ``gmeow_gts``
    OpenPGP parser accepts, so the same armor works for the Python tests and
    the Rust pre-gate.
    """
    ed25519_algo = 22
    ed25519_oid = bytes.fromhex("2b06010401da470f01")
    body = bytearray()
    body.append(0x04)  # OpenPGP v4
    body.extend((0).to_bytes(4, "big"))  # creation time
    body.append(ed25519_algo)
    body.append(len(ed25519_oid))
    body.extend(ed25519_oid)
    mpi_len = 1 + len(raw_public)
    body.extend((mpi_len * 8).to_bytes(2, "big"))
    body.append(0x40)
    body.extend(raw_public)

    # Old-format packet: tag 6, one-octet length.
    packet = bytearray()
    packet.append(0x98)
    packet.append(len(body))
    packet.extend(body)

    b64 = base64.b64encode(packet).decode("ascii")
    wrapped = "\n".join(b64[i : i + 64] for i in range(0, len(b64), 64))
    return (
        "-----BEGIN PGP PUBLIC KEY BLOCK-----\n\n"
        f"{wrapped}\n"
        "-----END PGP PUBLIC KEY BLOCK-----\n"
    )


def _make_signer() -> tuple[gts.Signer, str, str]:
    """Return a fresh signer, its public-key armor, and fingerprint."""
    private = Ed25519PrivateKey.generate()
    raw = private.public_key().public_bytes(
        serialization.Encoding.Raw, serialization.PublicFormat.Raw
    )
    armor = _armor_public_key(raw)
    fingerprint = gts.openpgp.public_key_fingerprint(armor)
    return gts.Signer(fingerprint, private), armor, fingerprint


def _rdflib_node_to_term(node: URIRef | Literal) -> gts.Term:
    if isinstance(node, URIRef):
        return gts.Term(
            kind=gts.TermKind.IRI,
            value=str(node),
            datatype=None,
            lang=None,
            reifier=None,
        )
    if isinstance(node, Literal):
        return gts.Term(
            kind=gts.TermKind.LITERAL,
            value=str(node),
            datatype=None,
            lang=str(node.language) if node.language else None,
            reifier=None,
        )
    raise TypeError(f"unsupported RDF term {node!r}")


def _build_gts_bytes(graph: Graph, signer: gts.Signer | None = None) -> bytes:
    """Serialize a small rdflib Graph into a (possibly signed) GTS bundle."""
    nodes: list[URIRef | Literal] = []
    node_to_idx: dict[URIRef | Literal, int] = {}

    def add_node(node: URIRef | Literal) -> None:
        if node not in node_to_idx:
            node_to_idx[node] = len(nodes)
            nodes.append(node)

    quads: list[tuple[int, int, int, int | None]] = []
    for subject, predicate, obj in graph:
        assert isinstance(subject, URIRef | Literal)
        assert isinstance(predicate, URIRef | Literal)
        assert isinstance(obj, URIRef | Literal)
        add_node(subject)
        add_node(predicate)
        add_node(obj)
        quads.append(
            (node_to_idx[subject], node_to_idx[predicate], node_to_idx[obj], None)
        )

    terms = [_rdflib_node_to_term(n) for n in nodes]

    writer = gts.Writer(profile="dist", signer=signer)
    writer.add_terms(terms)
    writer.add_quads(quads)
    return writer.to_bytes()


def _valid_ontology_graph() -> Graph:
    """A tiny but validation-clean GMEOW ontology graph."""
    graph = Graph()
    graph.bind("gufo", _GUFO)
    gm = Namespace(NAMESPACE)
    ontology = URIRef(ONTOLOGY_IRI)

    graph.add((ontology, RDF.type, OWL.Ontology))
    graph.add(
        (ontology, RDFS.label, Literal("Fixture ontology", lang="x-gmeow-english"))
    )
    graph.add(
        (
            ontology,
            SKOS.definition,
            Literal("A fixture ontology for tests.", lang="x-gmeow-english"),
        )
    )
    graph.add((ontology, RDFS.isDefinedBy, ontology))

    ontology_role = URIRef("https://example.org/ontology-role")
    graph.add((ontology_role, RDF.type, gm.GraphBoxRole))
    graph.add((ontology, gm.graphBoxRole, ontology_role))

    term = gm.SampleTerm
    graph.add((term, RDF.type, OWL.Class))
    graph.add((term, RDF.type, _GUFO.Kind))
    graph.add((term, RDFS.label, Literal("sample label", lang="x-gmeow-english")))
    graph.add(
        (
            term,
            SKOS.definition,
            Literal("English definition text.", lang="x-gmeow-english"),
        )
    )
    graph.add((term, RDFS.isDefinedBy, URIRef(NAMESPACE + "slices/lifecycle")))
    graph.add((term, gm.howToUse, Literal("Use it like this.", lang="x-gmeow-english")))
    graph.add(
        (term, gm.useWhen, Literal("Use it when testing.", lang="x-gmeow-english"))
    )
    graph.add((term, SKOS.example, Literal("Example usage.", lang="x-gmeow-english")))

    term_role = URIRef("https://example.org/term-role")
    graph.add((term_role, RDF.type, gm.GraphBoxRole))
    graph.add((term, gm.graphBoxRole, term_role))

    return graph


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


def test_extract_docs_unpacks_site_from_bundled_snapshot(
    runner: CliRunner, tmp_path: Path
) -> None:
    out = tmp_path / "docs-tree"
    result = runner.invoke(public_app, ["extract-docs", "--directory", str(out)])
    assert result.exit_code == 0, result.output
    # `extract-docs` is now a pure unpack of the Rust-rendered ontology-docs site
    # (#1019): the embedded ``ontology-docs`` blob (#897) is the docs tree, with
    # the internal language prefix stripped, so the site lands at the root.
    assert (out / "index.html").exists()
    assert (out / "index.md").exists()
    assert (out / "assets" / "gmeow.css").exists()
    assert (out / "terms").is_dir()
    assert (out / "linkages" / "index.html").exists()
    assert (out / "search-index.json").exists()
    # A per-term reference page is unpacked (carrying the enriched Usage Advice /
    # Alignments sections rendered natively by `md_term`).
    assert list((out / "terms").glob("*/index.md")), "per-term Markdown pages present"


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


def test_public_cli_excludes_checkout_commands(runner: CliRunner) -> None:
    result = runner.invoke(public_app, ["--help"])
    assert result.exit_code == 0
    assert "verify" in result.output
    assert "regenerate" not in result.output
    assert "quality" not in result.output
    assert "check-generated" not in result.output
    assert "Validate RDF data" in result.output


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


def test_dev_i18n_merge_writes_stdout(runner: CliRunner) -> None:
    result = runner.invoke(dev_app, ["i18n", "merge", "--lang", "fr"])
    assert result.exit_code == 0, result.output
    assert "Existence d'entité" in result.output
    assert "PO file(s)" in result.stderr


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


# --------------------------------------------------------------------------- #
# GTS signature/trust verification in gmeow-dev validate (#646)
# --------------------------------------------------------------------------- #


def _write_signed_fixture(
    tmp_path: Path,
    *,
    signer: gts.Signer,
    armor: str,
    trusted_signers: list[str],
) -> tuple[Path, Path]:
    """Write a signed .gts, its public-key armor, and a matching policy file."""
    graph = _valid_ontology_graph()
    bundle_path = tmp_path / "signed.gts"
    bundle_path.write_bytes(_build_gts_bytes(graph, signer=signer))

    key_path = tmp_path / "key.asc"
    key_path.write_text(armor, encoding="utf-8")

    policy_path = tmp_path / "policy.toml"
    lines = [
        f"trusted_signers = {trusted_signers!r}",
        "require_trusted_signer = true",
        'trusted_key = "key.asc"',
    ]
    policy_path.write_text("\n".join(lines), encoding="utf-8")
    return bundle_path, policy_path


def test_dev_validate_unsigned_gts_passes(runner: CliRunner, tmp_path: Path) -> None:
    """An unsigned, ontologically valid bundle validates normally."""
    bundle_path = tmp_path / "unsigned.gts"
    bundle_path.write_bytes(_build_gts_bytes(_valid_ontology_graph()))

    result = runner.invoke(dev_app, ["validate", "--gts", str(bundle_path)])
    assert result.exit_code == 0, result.output
    assert "validation passed" in result.output


def test_dev_validate_unsigned_gts_require_signed_fails(
    runner: CliRunner, tmp_path: Path
) -> None:
    """--require-signed aborts an unsigned bundle with signature.missing."""
    bundle_path = tmp_path / "unsigned.gts"
    bundle_path.write_bytes(_build_gts_bytes(_valid_ontology_graph()))

    result = runner.invoke(
        dev_app, ["validate", "--gts", str(bundle_path), "--require-signed"]
    )
    assert result.exit_code != 0, result.output
    assert "no signed frames found" in result.output


def test_dev_validate_signed_trusted_gts_passes(
    runner: CliRunner, tmp_path: Path
) -> None:
    """A signed bundle whose signer is in the trust policy passes."""
    signer, armor, fingerprint = _make_signer()
    bundle_path, policy_path = _write_signed_fixture(
        tmp_path,
        signer=signer,
        armor=armor,
        trusted_signers=[fingerprint],
    )

    result = runner.invoke(
        dev_app,
        ["validate", "--gts", str(bundle_path), "--trust-policy", str(policy_path)],
    )
    assert result.exit_code == 0, result.output
    assert "validation passed" in result.output


def test_dev_validate_signed_untrusted_gts_fails(
    runner: CliRunner, tmp_path: Path
) -> None:
    """A signed bundle whose signer is not trusted fails with signature.untrusted."""
    signer, armor, _fingerprint = _make_signer()
    bundle_path, policy_path = _write_signed_fixture(
        tmp_path,
        signer=signer,
        armor=armor,
        trusted_signers=["AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"],
    )

    result = runner.invoke(
        dev_app,
        ["validate", "--gts", str(bundle_path), "--trust-policy", str(policy_path)],
    )
    assert result.exit_code != 0, result.output
    assert (
        "no cryptographically valid signature from a deployment-trusted signer"
        in result.output
    )


def test_dev_validate_require_signed_without_gts_errors(runner: CliRunner) -> None:
    """Signature flags are only meaningful together with --gts."""
    result = runner.invoke(dev_app, ["validate", "--require-signed"])
    assert result.exit_code != 0
    assert "--gts" in _strip_ansi(result.output)


def test_dev_validate_gts_with_trusted_key_cli_flag(
    runner: CliRunner, tmp_path: Path
) -> None:
    """A signed bundle validates when the signer key is passed via --trusted-key."""
    signer, armor, _fingerprint = _make_signer()
    bundle_path = tmp_path / "signed.gts"
    bundle_path.write_bytes(_build_gts_bytes(_valid_ontology_graph(), signer=signer))

    key_path = tmp_path / "key.asc"
    key_path.write_text(armor, encoding="utf-8")

    result = runner.invoke(
        dev_app,
        ["validate", "--gts", str(bundle_path), "--trusted-key", str(key_path)],
    )
    assert result.exit_code == 0, result.output
    assert "validation passed" in result.output


def test_dev_validate_gts_with_untrusted_key_cli_flag_fails(
    runner: CliRunner, tmp_path: Path
) -> None:
    """A wrong --trusted-key cannot verify the signature, so validation fails.

    Gap 2 promoted unresolved signatures from warnings to errors; a mismatched
    --trusted-key means the signature cannot be resolved and the run hard-fails.
    """
    signer, _armor, _fingerprint = _make_signer()
    _untrusted_signer, untrusted_armor, _fingerprint2 = _make_signer()
    bundle_path = tmp_path / "signed.gts"
    bundle_path.write_bytes(_build_gts_bytes(_valid_ontology_graph(), signer=signer))

    key_path = tmp_path / "key.asc"
    key_path.write_text(untrusted_armor, encoding="utf-8")

    result = runner.invoke(
        dev_app,
        ["validate", "--gts", str(bundle_path), "--trusted-key", str(key_path)],
    )
    assert result.exit_code == 1, result.output
    assert "signature(s) unverified" in result.output
    assert "error(s)" in result.output
