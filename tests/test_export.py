"""Tests for the flattened export views (tabular + LLM)."""

from __future__ import annotations

import csv
import json
from pathlib import Path

from gmeow_tools.export import collect_terms, curie, export_all

NAMESPACE = "https://blackcatinformatics.ca/gmeow/"


def test_curie_compacts_known_namespaces() -> None:
    assert curie(NAMESPACE + "Person") == "gmeow:Person"
    assert curie("https://schema.org/Person") == "schema:Person"
    # Unknown namespace is returned unchanged.
    assert curie("https://example.org/x") == "https://example.org/x"


def test_collect_terms_covers_classes_properties_individuals() -> None:
    terms = collect_terms()
    by_cat: dict[str, set[str]] = {}
    for t in terms:
        by_cat.setdefault(t.category, set()).add(t.curie)
    # Classes from several slices.
    for cls in ("gmeow:Person", "gmeow:EmailMessage", "gmeow:CryptographicKey"):
        assert cls in by_cat["class"]
    # Properties.
    for prop in ("gmeow:from", "gmeow:signingKey", "gmeow:trustor"):
        assert prop in by_cat["property"]
    # Value-vocabulary individuals (the scheme-as-value pattern).
    assert "gmeow:keySchemePGP" in by_cat["individual"]


def test_term_attributes_are_populated() -> None:
    terms = {t.curie: t for t in collect_terms()}
    person = terms["gmeow:Person"]
    assert person.label == "Person"
    assert person.definition
    assert "gmeow:Agent" in person.parents
    assert "equivalentClass=foaf:Person" in person.alignments

    signing_key = terms["gmeow:signingKey"]
    assert signing_key.prop_kind == "object"
    assert signing_key.domain == "gmeow:CryptographicSignature"
    assert signing_key.range == "gmeow:CryptographicKey"
    assert signing_key.functional is True


def test_export_all_writes_every_view(tmp_path: Path) -> None:
    written = export_all(dist_dir=tmp_path)
    names = {p.name for p in written}
    assert names == {
        "gmeow-classes.csv",
        "gmeow-properties.csv",
        "gmeow-individuals.csv",
        "gmeow-terms.csvw.json",
        "gmeow-terms.jsonl",
        "gmeow-terms.md",
        "llms.txt",
    }
    for path in written:
        assert path.exists() and path.stat().st_size > 0


def test_classes_csv_is_well_formed(tmp_path: Path) -> None:
    export_all(dist_dir=tmp_path)
    with (tmp_path / "gmeow-classes.csv").open(encoding="utf-8") as handle:
        rows = list(csv.DictReader(handle))
    by_curie = {r["curie"]: r for r in rows}
    assert "gmeow:Person" in by_curie
    assert by_curie["gmeow:Person"]["iri"] == NAMESPACE + "Person"
    assert by_curie["gmeow:Person"]["subClassOf"] == "gmeow:Agent"


def test_properties_csv_records_functionality(tmp_path: Path) -> None:
    export_all(dist_dir=tmp_path)
    with (tmp_path / "gmeow-properties.csv").open(encoding="utf-8") as handle:
        rows = {r["curie"]: r for r in csv.DictReader(handle)}
    assert rows["gmeow:signingKey"]["functional"] == "true"
    assert rows["gmeow:from"]["functional"] == "false"


def test_jsonl_catalog_parses(tmp_path: Path) -> None:
    export_all(dist_dir=tmp_path)
    lines = (tmp_path / "gmeow-terms.jsonl").read_text(encoding="utf-8").splitlines()
    records = [json.loads(line) for line in lines]
    assert records, "catalog should not be empty"
    for rec in records:
        assert {"category", "curie", "iri", "label", "definition"} <= rec.keys()
    categories = {r["category"] for r in records}
    assert categories == {"class", "property", "individual"}


def test_csvw_descriptor_is_valid(tmp_path: Path) -> None:
    export_all(dist_dir=tmp_path)
    descriptor = json.loads((tmp_path / "gmeow-terms.csvw.json").read_text("utf-8"))
    assert descriptor["@context"] == "http://www.w3.org/ns/csvw"
    assert {t["url"] for t in descriptor["tables"]} == {
        "gmeow-classes.csv",
        "gmeow-properties.csv",
        "gmeow-individuals.csv",
    }


def test_markdown_reference_has_all_sections(tmp_path: Path) -> None:
    export_all(dist_dir=tmp_path)
    md = (tmp_path / "gmeow-terms.md").read_text(encoding="utf-8")
    # The header counts individuals, so the section must actually be emitted.
    for section in ("## Classes", "## Properties", "## Individuals"):
        assert section in md
    assert "gmeow:keySchemePGP" in md


def test_llms_txt_bundle(tmp_path: Path) -> None:
    export_all(dist_dir=tmp_path)
    text = (tmp_path / "llms.txt").read_text(encoding="utf-8")
    assert text.startswith("# GMEOW")
    assert "## Classes" in text and "## Properties" in text
    assert "gmeow:EmailMessage" in text


def test_llms_txt_has_no_blank_nodes(tmp_path: Path) -> None:
    export_all(dist_dir=tmp_path)
    fresh_content = (tmp_path / "llms.txt").read_text(encoding="utf-8")
    assert "_:" not in fresh_content, (
        "Found raw blank node ID in freshly generated llms.txt"
    )
