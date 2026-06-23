"""Tests for the OKF (Open Knowledge Format) bidirectional surface (#780).

Marked ``ci_only``: the OKF bundle is the agent-facing export surface (like the
CSV/Markdown/llms.txt views), so these run in ``make test`` / CI but are excluded
from the fast ``make check`` gate. The ``gts from-okf`` round-trip + lift tests are
additionally gated on a built ``gts`` binary with OKF support (a separate
acceptance lane — set ``GMEOW_GTS_BIN``), mirroring the network/HermiT lanes.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path

import pytest
import yaml
from gmeow_rdf.compat.rdflib import OWL, RDF, RDFS, SKOS, URIRef

from gmeow_tools.export import collect_terms
from gmeow_tools.okf_export import (
    _LOSSY_NOTE,
    OKF_DIR_NAME,
    export_okf_bundle,
    okf_index_records,
)

pytestmark = pytest.mark.ci_only

_OKF_NS = "https://blackcatinformatics.ca/projects/gts/okf#"


def _gts_available() -> bool:
    """Whether an *OKF-capable* gts binary is locatable for the acceptance lane.

    Locating any ``gts`` is not enough: the PyPI ``gts`` wheel is built without
    ``--features okf`` and has no ``from-okf`` subcommand, so it would make the
    acceptance tests *fail* rather than skip. Probe the actual capability so the
    lane skips cleanly on an okf-less binary and activates the moment an
    okf-capable ``gts`` (Rust, built ``--features okf``) is present.
    """
    from gmeow_tools.okf_import import OkfBinaryNotFoundError, find_gts_binary

    try:
        binary = find_gts_binary()
    except OkfBinaryNotFoundError:
        return False
    probe = subprocess.run(
        [str(binary), "from-okf", "--help"],
        capture_output=True,
        text=True,
        check=False,
    )
    return probe.returncode == 0


_requires_gts = pytest.mark.skipif(
    not _gts_available(),
    reason="gts binary with OKF support not found (build it, set GMEOW_GTS_BIN)",
)


def _split_frontmatter(md: str) -> tuple[dict[str, object], str]:
    """Split an OKF document into its YAML frontmatter mapping and body."""
    assert md.startswith("---\n")
    _, fm, body = md.split("---\n", 2)
    return yaml.safe_load(fm), body


@pytest.fixture(scope="module")
def bundle(tmp_path_factory: pytest.TempPathFactory) -> Path:
    """Write the full OKF bundle once for the module."""
    out = tmp_path_factory.mktemp("okf") / OKF_DIR_NAME
    out.mkdir()
    export_okf_bundle(out)
    return out


_CATEGORY_DIR = {
    "class": "classes",
    "property": "properties",
    "individual": "individuals",
}


def test_one_doc_per_term_in_category_dirs(bundle: Path) -> None:
    """Every folded term has exactly one doc under its category directory."""
    terms = collect_terms()
    docs = {p.relative_to(bundle).as_posix() for p in bundle.rglob("*.md")}
    for term in terms:
        stem = term.curie.split(":", 1)[-1]
        expected = f"{_CATEGORY_DIR[term.category]}/{stem}.md"
        assert expected in docs, f"missing OKF doc for {term.curie}"


def test_frontmatter_shape_and_okf_type_is_a_string(bundle: Path) -> None:
    """Frontmatter has the recognized keys; okf:type is a string, not rdf:type."""
    # Exclude the per-directory index.md (type: Index) — pick an actual class doc.
    sample = next(p for p in (bundle / "classes").glob("*.md") if p.name != "index.md")
    fm, body = _split_frontmatter(sample.read_text(encoding="utf-8"))
    # the six recognized keys + the okf:type string literal (NOT rdf:type)
    assert fm["type"] == "Class"
    assert isinstance(fm["type"], str)
    resource = fm["resource"]
    assert isinstance(resource, str) and resource.startswith("https://")
    assert "title" in fm
    assert "curie" in fm  # an okf:<key> extension
    assert body.strip(), "body must be non-empty"


def test_relation_links_point_at_in_bundle_docs(bundle: Path) -> None:
    """Relation links resolve to sibling docs that actually exist in the bundle."""
    # find a class with a gmeow parent that is itself in the bundle
    terms = {t.curie: t for t in collect_terms()}
    target = next(
        t
        for t in terms.values()
        if t.category == "class" and any(p in terms for p in t.parents)
    )
    doc = (bundle / "classes" / f"{target.curie.split(':', 1)[-1]}.md").read_text()
    assert "## Relations" in doc
    parent = next(p for p in target.parents if p in terms)
    parent_stem = parent.split(":", 1)[-1]
    assert f"({parent_stem}.md)" in doc  # a [label](relpath) link to the parent


def test_root_index_declares_lossy(bundle: Path) -> None:
    """The root index carries the in-band LOSSY-projection declaration."""
    root_index = (bundle / "index.md").read_text(encoding="utf-8")
    assert _LOSSY_NOTE in root_index
    assert "LOSSY" in root_index


def test_deterministic_under_hash_seed_variation(tmp_path: Path) -> None:
    """Two exports under different PYTHONHASHSEED must be byte-identical."""
    script = (
        "from pathlib import Path;"
        "from gmeow_tools.okf_export import export_okf_bundle, OKF_DIR_NAME;"
        "out=Path('{out}')/OKF_DIR_NAME; out.mkdir(parents=True);"
        "export_okf_bundle(out)"
    )
    digests: list[str] = []
    for seed in ("0", "1"):
        run_dir = tmp_path / f"seed{seed}"
        env_script = script.format(out=run_dir)
        subprocess.run(
            [sys.executable, "-c", env_script],
            check=True,
            env={"PYTHONHASHSEED": seed, "PATH": "/usr/bin:/bin"},
        )
        files = sorted(
            (run_dir / OKF_DIR_NAME).rglob("*.md"),
            key=lambda p: p.relative_to(run_dir).as_posix(),
        )
        blob = b"".join(
            p.relative_to(run_dir).as_posix().encode() + p.read_bytes() for p in files
        )
        import hashlib

        digests.append(hashlib.blake2b(blob).hexdigest())
    assert digests[0] == digests[1], "OKF export is not deterministic under hash seed"


def test_mcp_okf_index_resource() -> None:
    """The MCP okf-index resource serves the bundle manifest with lossy=True."""
    from gmeow_tools.mcp_server_consumer import gmeow_okf_index

    fn = getattr(gmeow_okf_index, "fn", gmeow_okf_index)
    import json

    payload = json.loads(fn())
    assert payload["ok"] is True
    assert payload["format"] == "okf"
    assert payload["lossy"] is True
    assert payload["count"] == len(collect_terms())
    doc = payload["documents"][0]
    assert doc["path"].startswith(f"{OKF_DIR_NAME}/")
    assert doc["type"] in {"Class", "Property", "Individual"}


def test_index_records_match_bundle(bundle: Path) -> None:
    """Every manifest record points at a document that exists in the bundle."""
    records = okf_index_records(collect_terms())
    for rec in records:
        assert (bundle.parent / rec["path"]).exists()


# --------------------------------------------------------------------------- #
# gts-gated acceptance lane (requires a gts binary built --features okf)
# --------------------------------------------------------------------------- #


@_requires_gts
def test_gts_from_okf_folds_our_bundle(bundle: Path, tmp_path: Path) -> None:
    """The bundle we emit is conformant: ``gts from-okf`` folds it without error."""
    from gmeow_tools.okf_import import find_gts_binary

    out = tmp_path / "folded.gts"
    proc = subprocess.run(
        [str(find_gts_binary()), "from-okf", str(bundle), "-o", str(out)],
        capture_output=True,
        text=True,
        check=False,
    )
    assert proc.returncode == 0, proc.stderr
    assert out.stat().st_size > 0


@_requires_gts
def test_lift_roundtrips_recognized_subset_and_retains_unknown(tmp_path: Path) -> None:
    """Lift maps the recognized okf: subset and retains unknown keys verbatim."""
    from gmeow_tools.okf_import import lift_okf_graph, okf_dir_to_graph

    okf = tmp_path / "hand"
    (okf / "concepts").mkdir(parents=True)
    (okf / "concepts" / "widget.md").write_text(
        "---\n"
        "type: Class\n"
        "title: Widget\n"
        "description: A small UI component.\n"
        "resource: https://example.org/onto/Widget\n"
        "scope_notes:\n  - Use for interactive controls.\n"
        "custom_field: keep-me\n"
        "---\nA small UI component.\n",
        encoding="utf-8",
    )
    graph = okf_dir_to_graph(okf)
    lifted, report = lift_okf_graph(graph)
    widget = URIRef("https://example.org/onto/Widget")
    assert (widget, RDF.type, OWL.Class) in lifted
    assert next(lifted.objects(widget, RDFS.label), None) is not None
    assert next(lifted.objects(widget, SKOS.definition), None) is not None
    assert next(lifted.objects(widget, SKOS.scopeNote), None) is not None
    # the unknown frontmatter key survives as a provenance-bearing okf: annotation
    custom = URIRef(_OKF_NS + "custom_field")
    assert next(lifted.objects(widget, custom), None) is not None
    assert report.lifted >= 4
    assert report.retained >= 1
