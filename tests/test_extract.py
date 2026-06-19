"""Tests for the license-policy guard on module extraction."""

from __future__ import annotations

from pathlib import Path

import pytest

from gmeow_tools.config import ALIGNMENT_TARGETS
from gmeow_tools.extract import LicensePolicyError, extract_terms, guard_importable


def test_guard_allows_import_ok() -> None:
    # Import-ok targets do not raise.
    guard_importable("gufo")
    guard_importable("foaf")
    guard_importable("umbel")


@pytest.mark.parametrize("target", ["dolce", "schema"])
def test_guard_refuses_reference_only(target: str) -> None:
    with pytest.raises(LicensePolicyError):
        guard_importable(target)


def test_guard_refuses_unknown_target() -> None:
    with pytest.raises(LicensePolicyError):
        guard_importable("definitely-not-a-target")


def test_extract_emits_slme_provenance(tmp_path: Path) -> None:
    # umbel is IMPORT_OK; extract a tiny module and assert the provenance block.
    umbel_ns = ALIGNMENT_TARGETS["umbel"].namespace
    source = tmp_path / "umbel-tiny.ttl"
    source.write_text(
        "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n"
        "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n"
        f"@prefix ex: <{umbel_ns}> .\n"
        "ex:Animal a owl:Class .\n"
        "ex:Dog a owl:Class ; rdfs:subClassOf ex:Animal .\n",
        encoding="utf-8",
    )
    output = tmp_path / "module.ttl"
    result = extract_terms(
        "umbel",
        source=source,
        terms=[f"{umbel_ns}Dog"],
        output=output,
        method="STAR",
    )
    text = result.read_text(encoding="utf-8")

    # The extracted module is present.
    assert f"{umbel_ns}Dog" in text
    # The provenance triples (reused vocab only) are present, deterministic.
    assert "gmeow:activity/slme-extract a gmeow:Activity" in text
    assert "gmeow:wasGeneratedBy gmeow:activity/slme-extract" in text
    assert f"gmeow:wasDerivedFrom <{umbel_ns}>" in text
    assert "gmeow:wasAssociatedWith gmeow:agent/native-slme" in text
    assert "SLME method STAR" in text
    # No timestamps — determinism.
    assert "wasGeneratedAtTime" not in text
    # Re-running yields byte-identical output (pure function of inputs).
    output2 = tmp_path / "module2.ttl"
    extract_terms(
        "umbel",
        source=source,
        terms=[f"{umbel_ns}Dog"],
        output=output2,
        method="STAR",
    )
    assert output2.read_text(encoding="utf-8") == text
