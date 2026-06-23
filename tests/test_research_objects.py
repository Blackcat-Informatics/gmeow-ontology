# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Research-object exports (#58): Croissant / RO-Crate / DataCite / Frictionless.

Marked ``ci_only`` like the other secondary export surfaces.
"""

from __future__ import annotations

import json
import zipfile
from pathlib import Path
from xml.etree import ElementTree as ET

import pytest

from gmeow_tools.research_objects import (
    CROISSANT_CONFORMS_TO,
    DATACITE_NS,
    EXAMPLE_INPUTS,
    PROCESS_RUN_PROFILE,
    RO_CRATE_SPEC,
    DatasetMeta,
    build_croissant,
    build_datacite_xml,
    build_frictionless,
    build_ro_crate_metadata,
    dataset_meta,
    export_research_objects,
    load_instance_graph,
    package_ro_crate,
    validate_croissant,
    validate_frictionless,
    validate_ro_crate,
)

pytestmark = pytest.mark.ci_only


@pytest.fixture(scope="module")
def exports(tmp_path_factory: pytest.TempPathFactory) -> Path:
    out = tmp_path_factory.mktemp("research-objects")
    export_research_objects(EXAMPLE_INPUTS, out, stem="lillith")
    return out


@pytest.fixture(scope="module")
def meta() -> DatasetMeta:
    return dataset_meta(load_instance_graph(EXAMPLE_INPUTS))


def test_dataset_meta_is_read_never_hardcoded(meta: DatasetMeta) -> None:
    assert meta.title == "Lillith GraphRAG benchmark"
    assert meta.license_id == "CC-BY-4.0"
    assert meta.license_url.endswith("/CC-BY-4.0")
    assert meta.creator == "Blackcat Informatics® Inc."
    assert meta.publication_year == "2026"


def test_missing_descriptor_is_an_error(tmp_path: Path) -> None:
    bare = tmp_path / "bare.ttl"
    bare.write_text(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n"
        "<https://example.org/d> a gmeow:Document .\n",
        encoding="utf-8",
    )
    with pytest.raises(ValueError, match="dataset descriptor"):
        dataset_meta(load_instance_graph([bare]))


# --------------------------------------------------------------------------- #
# Croissant
# --------------------------------------------------------------------------- #


def test_croissant_shape_and_validation(exports: Path) -> None:
    doc = json.loads((exports / "lillith.croissant.jsonld").read_text("utf-8"))
    assert doc["@type"] == "sc:Dataset"
    assert doc["conformsTo"] == CROISSANT_CONFORMS_TO
    assert doc["license"].endswith("CC-BY-4.0")
    assert doc["version"] == "1.0.0"
    assert (
        doc["citeAs"]
        == "Blackcat Informatics® Inc. (2026). Lillith GraphRAG benchmark. https://blackcatinformatics.ca/gmeow/examples/graphrag/lillith-benchmark"
    )
    assert {rs["@id"] for rs in doc["recordSet"]} == {
        "chunks",
        "claims",
        "evalScores",
    }
    for dist in doc["distribution"]:
        assert "contentUrl" in dist
        assert "sha256" in dist
        assert "md5" in dist
    assert validate_croissant(doc) == []


def test_croissant_digest_fields_are_well_formed(exports: Path) -> None:
    """sha256 and md5 fields, when present, must be lowercase hex of expected length."""
    doc = json.loads((exports / "lillith.croissant.jsonld").read_text("utf-8"))
    for dist in doc["distribution"]:
        if "sha256" in dist:
            assert len(dist["sha256"]) == 64
            assert all(c in "0123456789abcdef" for c in dist["sha256"])
        if "md5" in dist:
            assert len(dist["md5"]) == 32
            assert all(c in "0123456789abcdef" for c in dist["md5"])
        if "description" in dist:
            assert dist["description"].startswith("content digest: blake3:")
    assert any("blake3" in lim for lim in doc["rai:dataLimitation"])


def test_croissant_validator_catches_mutations(exports: Path) -> None:
    doc = json.loads((exports / "lillith.croissant.jsonld").read_text("utf-8"))
    doc["distribution"][0]["sha256"] = "not-hex"
    doc.pop("license")
    problems = validate_croissant(doc)
    assert any("sha256" in p for p in problems)
    assert any("license" in p for p in problems)


def test_croissant_full_validation(exports: Path) -> None:
    """Full EXTERNAL Croissant validation via mlcroissant — must pass."""
    import mlcroissant as mlc

    dataset = mlc.Dataset(jsonld=str(exports / "lillith.croissant.jsonld"))
    assert dataset.metadata is not None


# --------------------------------------------------------------------------- #
# RO-Crate
# --------------------------------------------------------------------------- #


def test_ro_crate_structure(exports: Path) -> None:
    crate = exports / "ro-crate"
    assert validate_ro_crate(crate) == []
    doc = json.loads((crate / "ro-crate-metadata.json").read_text("utf-8"))
    by_id = {e["@id"]: e for e in doc["@graph"]}
    descriptor = by_id["ro-crate-metadata.json"]
    assert {"@id": RO_CRATE_SPEC} in descriptor["conformsTo"]
    assert {"@id": PROCESS_RUN_PROFILE} in descriptor["conformsTo"]
    actions = [e for e in doc["@graph"] if e["@type"] == "CreateAction"]
    assert actions, "no provenance CreateActions"
    invocation = next(e for e in actions if e["@id"].endswith("invocation-44"))
    assert invocation["instrument"]["@id"].endswith("extractor")
    assert invocation["result"], "the invocation's outputs are missing"


def test_ro_crate_graph_is_flat(exports: Path) -> None:
    """Entity values are scalars or {'@id': …} refs — never nested entities."""
    doc = json.loads(
        (exports / "ro-crate" / "ro-crate-metadata.json").read_text("utf-8")
    )
    for entity in doc["@graph"]:
        for value in entity.values():
            items = value if isinstance(value, list) else [value]
            for item in items:
                if isinstance(item, dict):
                    assert set(item) == {"@id"}, f"nested entity in {entity['@id']}"


def test_ro_crate_zip_is_deterministic(exports: Path, tmp_path: Path) -> None:
    a = package_ro_crate(exports / "ro-crate", tmp_path / "a.zip")
    b = package_ro_crate(exports / "ro-crate", tmp_path / "b.zip")
    assert a.read_bytes() == b.read_bytes()
    with zipfile.ZipFile(a) as zf:
        assert zf.testzip() is None
        names = zf.namelist()
    assert "ro-crate-metadata.json" in names
    assert "ro-crate-preview.html" in names


def test_ro_crate_preview_names_the_dataset(exports: Path) -> None:
    html = (exports / "ro-crate" / "ro-crate-preview.html").read_text("utf-8")
    assert "Lillith GraphRAG benchmark" in html


def test_ro_crate_validator_catches_missing_part(tmp_path: Path) -> None:
    crate = tmp_path / "crate"
    crate.mkdir()
    (crate / "ro-crate-metadata.json").write_text(
        json.dumps(
            {
                "@context": "https://w3id.org/ro/crate/1.1/context",
                "@graph": [
                    {
                        "@id": "ro-crate-metadata.json",
                        "@type": "CreativeWork",
                        "conformsTo": [{"@id": RO_CRATE_SPEC}],
                        "about": {"@id": "./"},
                    },
                    {
                        "@id": "./",
                        "@type": "Dataset",
                        "name": "x",
                        "description": "x",
                        "datePublished": "2026",
                        "license": {"@id": "https://example.org/l"},
                        "hasPart": [{"@id": "ghost.ttl"}],
                    },
                ],
            }
        ),
        encoding="utf-8",
    )
    problems = validate_ro_crate(crate)
    assert any("ghost.ttl" in p for p in problems)


# --------------------------------------------------------------------------- #
# Frictionless + DataCite
# --------------------------------------------------------------------------- #


def test_frictionless_is_schema_valid_with_verbatim_hashes(exports: Path) -> None:
    doc = json.loads((exports / "datapackage.json").read_text("utf-8"))
    assert validate_frictionless(doc) == []
    hashes = [r["hash"] for r in doc["resources"] if "hash" in r]
    assert hashes and all(h.startswith("blake3:") for h in hashes)
    assert "drops" in doc["notes"]


def test_datacite_xml_structure(exports: Path) -> None:
    tree = ET.fromstring((exports / "lillith.datacite.xml").read_text("utf-8"))
    ns = {"d": DATACITE_NS}
    identifier = tree.find("d:identifier", ns)
    assert identifier is not None and identifier.text is not None
    assert identifier.text.startswith("10.5072/")  # the TEST-prefix placeholder
    assert tree.find("d:creators/d:creator/d:creatorName", ns) is not None
    resource_type = tree.find("d:resourceType", ns)
    assert resource_type is not None
    assert resource_type.get("resourceTypeGeneral") == "Dataset"
    rights = tree.find("d:rightsList/d:rights", ns)
    assert rights is not None and rights.get("rightsIdentifier") == "CC-BY-4.0"
    descriptions = tree.findall("d:descriptions/d:description", ns)
    assert any(d.get("descriptionType") == "TechnicalInfo" for d in descriptions)


def test_datacite_is_deterministic(meta: DatasetMeta) -> None:
    g = load_instance_graph(EXAMPLE_INPUTS)
    assert build_datacite_xml(g, meta) == build_datacite_xml(g, meta)


def test_builders_share_one_meta_path(meta: DatasetMeta) -> None:
    """All four documents agree on the descriptor — one source of truth."""
    g = load_instance_graph(EXAMPLE_INPUTS)
    croissant = build_croissant(g, meta)
    crate = build_ro_crate_metadata(g, meta)
    frictionless = build_frictionless(g, meta)
    graph = crate["@graph"]
    assert isinstance(graph, list)
    root = next(e for e in graph if e["@id"] == "./")
    assert croissant["name"] == root["name"] == frictionless["title"]


# --------------------------------------------------------------------------- #
# Workflow Run Crate (the #47 model: BuildActivity + buildConfigUri)
# --------------------------------------------------------------------------- #


def test_workflow_run_crate_tier_is_earned(exports: Path) -> None:
    """A #47 workflow run upgrades the crate to Workflow Run Crate."""
    from gmeow_tools.research_objects import WORKFLOW_RUN_PROFILE

    doc = json.loads(
        (exports / "ro-crate" / "ro-crate-metadata.json").read_text("utf-8")
    )
    by_id = {e["@id"]: e for e in doc["@graph"]}
    descriptor = by_id["ro-crate-metadata.json"]
    assert {"@id": WORKFLOW_RUN_PROFILE} in descriptor["conformsTo"]
    root = by_id["./"]
    workflow = by_id[root["mainEntity"]["@id"]]
    assert "ComputationalWorkflow" in workflow["@type"]
    run = next(
        e
        for e in doc["@graph"]
        if e.get("@type") == "CreateAction" and e["@id"].endswith("pipeline-run")
    )
    assert run["instrument"]["@id"] == workflow["@id"]
    assert run["agent"]["@id"].endswith("pipeline-runner")  # the Builder
    # The build output (the Distribution) resolves and keeps its digest.
    result = by_id[run["result"][0]["@id"]]
    assert str(result.get("identifier", "")).startswith("blake3:")


def test_without_a_workflow_run_the_crate_stays_process_tier(
    tmp_path: Path,
) -> None:
    """No BuildActivity ⇒ Process Run Crate only — the tier is never claimed."""
    from gmeow_tools.research_objects import WORKFLOW_RUN_PROFILE

    inputs = [p for p in EXAMPLE_INPUTS if p.name != "lillith-dataset.ttl"]
    descriptor_only = tmp_path / "descriptor.ttl"
    descriptor_only.write_text(
        (
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n"
            "@prefix ex: <https://example.org/min/> .\n"
            "@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n"
            "ex:d a gmeow:Dataset ; gmeow:title 'Minimal' ;\n"
            "  gmeow:description 'minimal' ;\n"
            "  gmeow:hasLicense ex:l ; gmeow:wasAttributedTo ex:o ;\n"
            "  gmeow:datePublished '2026-01-01T00:00:00Z'^^xsd:dateTime .\n"
            "ex:l a gmeow:License ; gmeow:spdxLicenseId 'CC0-1.0' .\n"
            "ex:o a gmeow:Organization .\n"
        ),
        encoding="utf-8",
    )
    out = tmp_path / "out"
    export_research_objects(
        [descriptor_only, *inputs], out, profiles=("ro-crate",), stem="min"
    )
    doc = json.loads((out / "ro-crate" / "ro-crate-metadata.json").read_text("utf-8"))
    descriptor = next(e for e in doc["@graph"] if e["@id"] == "ro-crate-metadata.json")
    assert {"@id": WORKFLOW_RUN_PROFILE} not in descriptor["conformsTo"]
    root = next(e for e in doc["@graph"] if e["@id"] == "./")
    assert "mainEntity" not in root
