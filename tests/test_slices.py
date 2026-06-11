# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0

"""Slice discovery + manifest loading (Principles 15-16; issue #287).

Covers the structural contract: manifest-only identity (the path derives
nothing), tier as the sole core/extension distinction, the P15 consumer
field, duplicate-IRI rejection, and the extension dependency DAG rule.
"""

from pathlib import Path

import pytest

from gmeow_tools.config import SLICES_DIR
from gmeow_tools.slices import (
    Slice,
    SliceError,
    discover_slices,
    extension_dependency_violations,
)

_PREFIXES = """\
@prefix gmeow:   <https://blackcatinformatics.ca/gmeow/> .
@prefix rdfs:    <http://www.w3.org/2000/01/rdf-schema#> .
@prefix dcterms: <http://purl.org/dc/terms/> .
"""


def _write_manifest(
    root: Path,
    group: str,
    name: str,
    iri: str,
    tier: str = "gmeow:tierCore",
    deps: str = "",
) -> Path:
    slice_dir = root / group / name
    slice_dir.mkdir(parents=True)
    body = f"""{_PREFIXES}
<{iri}> a gmeow:Slice ;
    rdfs:label "{name}"@x-gmeow-english ;
    dcterms:title "Test slice {name}"@x-gmeow-english ;
    dcterms:creator "Test Author" ;
    gmeow:sliceTier {tier} ;
    {deps}
    gmeow:sliceConsumer "The test suite, via the loader contract."@x-gmeow-english .
"""
    (slice_dir / "manifest.ttl").write_text(body, encoding="utf-8")
    return slice_dir


class TestDiscovery:
    def test_repo_exemplar_loads(self) -> None:
        slices = discover_slices(SLICES_DIR)
        temporal = slices.get("https://blackcatinformatics.ca/gmeow/slices/temporal")
        assert temporal is not None
        assert temporal.is_core
        assert temporal.consumers, "P15: the consumer field is mandatory content"
        assert temporal.creators == ("Blackcat Informatics® Inc.",)

    def test_identity_is_manifest_only_not_path(self, tmp_path: Path) -> None:
        """slices/<group>/ carries no semantics: same name, two groups, two IRIs."""
        _write_manifest(
            tmp_path, "core", "languages", "https://example.org/slices/lang-core"
        )
        _write_manifest(
            tmp_path,
            "mycorp",
            "languages",
            "https://mycorp.example/slices/languages",
            tier="gmeow:tierExtension",
        )
        slices = discover_slices(tmp_path)
        assert set(slices) == {
            "https://example.org/slices/lang-core",
            "https://mycorp.example/slices/languages",
        }
        third_party = slices["https://mycorp.example/slices/languages"]
        assert third_party.group == "mycorp"
        assert not third_party.is_core

    def test_duplicate_iri_rejected(self, tmp_path: Path) -> None:
        iri = "https://example.org/slices/dup"
        _write_manifest(tmp_path, "core", "one", iri)
        _write_manifest(tmp_path, "extensions", "two", iri)
        with pytest.raises(SliceError, match="duplicate slice IRI"):
            discover_slices(tmp_path)

    def test_missing_tier_rejected(self, tmp_path: Path) -> None:
        slice_dir = tmp_path / "core" / "broken"
        slice_dir.mkdir(parents=True)
        (slice_dir / "manifest.ttl").write_text(
            f'{_PREFIXES}\n<https://example.org/x> a gmeow:Slice ; rdfs:label "x" .\n',
            encoding="utf-8",
        )
        with pytest.raises(SliceError, match="sliceTier"):
            discover_slices(tmp_path)

    def test_empty_root_is_empty(self, tmp_path: Path) -> None:
        assert discover_slices(tmp_path / "absent") == {}


class TestDependencyRule:
    def _slices(self, tmp_path: Path) -> dict[str, Slice]:
        _write_manifest(tmp_path, "core", "base", "https://example.org/s/base")
        _write_manifest(
            tmp_path,
            "extensions",
            "music",
            "https://example.org/s/music",
            tier="gmeow:tierExtension",
            deps="gmeow:sliceDependsOn <https://example.org/s/base> ;",
        )
        _write_manifest(
            tmp_path,
            "extensions",
            "images",
            "https://example.org/s/images",
            tier="gmeow:tierExtension",
            deps="gmeow:sliceDependsOn <https://example.org/s/music> ;",
        )
        return discover_slices(tmp_path)

    def test_extension_to_core_ok_extension_to_extension_rejected(
        self, tmp_path: Path
    ) -> None:
        problems = extension_dependency_violations(self._slices(tmp_path))
        assert len(problems) == 1
        assert "extension→extension" in problems[0]
        assert "images" in problems[0]

    def test_unknown_dependency_reported(self, tmp_path: Path) -> None:
        _write_manifest(
            tmp_path,
            "core",
            "solo",
            "https://example.org/s/solo",
            deps="gmeow:sliceDependsOn <https://example.org/s/ghost> ;",
        )
        problems = extension_dependency_violations(discover_slices(tmp_path))
        assert problems == [
            "https://example.org/s/solo: depends on unknown slice https://example.org/s/ghost"
        ]
