# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""End-to-end test for `gmeow-dev slice-fix-deps` over the native binding (#820 S8).

These tests exercise the real `gmeow_slice` PyO3 binding (the authoritative
native ``SliceCatalog`` + ``OwnershipAnalyzer``): S7's ``compute_fix_deps`` no
longer hard-fails on a missing extension — it discovers slices natively, runs the
ownership analyzer, and proposes ``gmeow:sliceDependsOn`` edits as a reviewable
unified diff. The dry-run path must write nothing.

The behavior under test (the native analyzer's undeclared/stale edge
classification) has its own hermetic Rust acceptance tests in
``crates/slice/tests/ownership_tests.rs`` and ``crates/slice/tests/py_tests.rs``;
this file pins the Python orchestration (diff emission + two-pass write contract).
"""

from __future__ import annotations

from pathlib import Path

from gmeow_tools.slice_fix_deps import compute_fix_deps

_NS = "https://blackcatinformatics.ca/gmeow/"


def _write(root: Path, rel: str, content: str) -> None:
    path = root / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")


def _manifest(slice_local: str, depends_on: tuple[str, ...] = ()) -> str:
    deps = "".join(
        f"    gmeow:sliceDependsOn <{_NS}slices/{d}> ;\n" for d in depends_on
    )
    return (
        f"@prefix gmeow: <{_NS}> .\n"
        "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n"
        "@prefix dcterms: <http://purl.org/dc/terms/> .\n\n"
        f"<{_NS}slices/{slice_local}> a gmeow:Slice ;\n"
        f"{deps}"
        "    gmeow:sliceTier gmeow:tierCore ;\n"
        f'    rdfs:label "{slice_local}"@x-gmeow-english ;\n'
        f'    dcterms:title "{slice_local}"@x-gmeow-english .\n'
    )


def _module(slice_local: str, *, defines: str, subclass_of: str | None = None) -> str:
    extra = f"    rdfs:subClassOf gmeow:{subclass_of} ;\n" if subclass_of else ""
    return (
        f"@prefix gmeow: <{_NS}> .\n"
        "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n"
        "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n\n"
        f"gmeow:{defines} a owl:Class ;\n"
        f"{extra}"
        f"    rdfs:isDefinedBy <{_NS}slices/{slice_local}> ;\n"
        f'    rdfs:label "{defines}"@x-gmeow-english .\n'
    )


def _build_two_slices(root: Path) -> None:
    """sliceB's module references sliceA's term but declares NO dependency."""
    _write(root, "core/sliceA/manifest.ttl", _manifest("sliceA"))
    _write(root, "core/sliceA/module.ttl", _module("sliceA", defines="termA"))
    _write(root, "core/sliceB/manifest.ttl", _manifest("sliceB"))
    _write(
        root,
        "core/sliceB/module.ttl",
        _module("sliceB", defines="termB", subclass_of="termA"),
    )


def test_fix_deps_runs_against_native_binding(tmp_path: Path) -> None:
    """The native binding exists, so compute_fix_deps runs (no hard-fail) and
    proposes the undeclared sliceB → sliceA dependency as a diff."""
    _build_two_slices(tmp_path)
    diffs = compute_fix_deps(tmp_path, apply=False)
    assert diffs, "expected an undeclared-dependency proposal"
    joined = "\n".join(diffs)
    # The undeclared semantic edge sliceB → sliceA must be proposed for addition.
    assert "sliceDependsOn" in joined
    assert "sliceA" in joined
    # The proposal targets sliceB's manifest (the depending slice).
    assert "sliceB/manifest.ttl" in joined


def test_fix_deps_dry_run_writes_nothing(tmp_path: Path) -> None:
    """A dry-run (apply=False) emits a diff but never mutates the manifests."""
    _build_two_slices(tmp_path)
    before = (tmp_path / "core/sliceB/manifest.ttl").read_text(encoding="utf-8")
    diffs = compute_fix_deps(tmp_path, apply=False)
    assert diffs
    after = (tmp_path / "core/sliceB/manifest.ttl").read_text(encoding="utf-8")
    assert before == after, "dry-run must not write to manifests"


def test_fix_deps_clean_set_has_no_proposals(tmp_path: Path) -> None:
    """When sliceB already declares its dependency, no proposal is emitted."""
    _write(tmp_path, "core/sliceA/manifest.ttl", _manifest("sliceA"))
    _write(tmp_path, "core/sliceA/module.ttl", _module("sliceA", defines="termA"))
    _write(
        tmp_path,
        "core/sliceB/manifest.ttl",
        _manifest("sliceB", depends_on=("sliceA",)),
    )
    _write(
        tmp_path,
        "core/sliceB/module.ttl",
        _module("sliceB", defines="termB", subclass_of="termA"),
    )
    diffs = compute_fix_deps(tmp_path, apply=False)
    assert diffs == [], "a fully-declared slice set needs no dependency edits"


def test_native_binding_catalog_and_analyzer(tmp_path: Path) -> None:
    """Drive the gmeow_slice binding directly: discovery + ownership analysis.

    Pins the authoritative-binding surface that #820 S8 wires
    (``SliceCatalog.discover`` → records/manifest/artifacts;
    ``OwnershipAnalyzer.analyze`` → edges + ownership_errors). The native
    ownership analyzer is cross-slice (physical-origin based), so a term defined
    by two slices is a Conflict — strictly stronger than the per-module
    path-derived ``slice_ownership_lint`` it supersedes for fix-deps.
    """
    import gmeow_slice

    _build_two_slices(tmp_path)
    catalog = gmeow_slice.SliceCatalog.discover(str(tmp_path))
    records = catalog.records()
    assert {r.manifest.slice_iri for r in records} == {
        f"{_NS}slices/sliceA",
        f"{_NS}slices/sliceB",
    }
    # Every slice carries a Manifest + Module artifact in its inventory.
    rec_b = next(r for r in records if r.manifest.slice_iri.endswith("sliceB"))
    roles = {a.role for a in rec_b.artifacts}
    assert "Manifest" in roles and "Module" in roles

    report = gmeow_slice.OwnershipAnalyzer(catalog).analyze()
    # A clean (no-conflict) two-slice set has no ownership defect.
    assert not report.has_ownership_defect()
    assert report.ownership_errors() == []
    # The undeclared semantic edge sliceB → sliceA is present.
    edge = next(
        e
        for e in report.edges
        if e.from_slice.endswith("sliceB") and e.to_slice.endswith("sliceA")
    )
    assert edge.is_semantic
    assert edge.reconciliation == "undeclared"


def _apply_unified_diffs(root: Path, diffs: list[str]) -> None:
    """Re-derive patched files by running compute_fix_deps(apply=True) again.

    (Applying difflib output by hand is brittle; the apply path writes the native
    patched_text directly, so this helper just re-runs with apply=True.)
    """
    compute_fix_deps(root, apply=True)


def _native_discovers(root: Path) -> bool:
    """Return True iff the native catalog re-discovers (re-parses) every manifest
    under `root` without error — the authoritative well-formedness check.

    The native Turtle parser is the one that matters (it is lenient on
    `@x-gmeow-*` language tags, unlike rdflib), and discovery parses each
    manifest.ttl. A malformed terminator would make discovery raise.
    """
    import gmeow_slice

    try:
        catalog = gmeow_slice.SliceCatalog.discover(str(root))
    except Exception:
        return False
    return len(catalog.records()) > 0


def test_fix_deps_removes_terminal_dot_stale_dependency(tmp_path: Path) -> None:
    """Removing a STALE dependency that is the LAST predicate (terminal `.`) must
    leave well-formed Turtle (the CodeRabbit terminal-`.` regression).

    sliceB declares a dependency on sliceA as its terminal predicate, but its
    module references NO sliceA term → the declaration is stale and must be
    removed without breaking Turtle termination.
    """
    # sliceA defines termA; sliceB declares sliceA dep but never uses it.
    _write(tmp_path, "core/sliceA/manifest.ttl", _manifest("sliceA"))
    _write(tmp_path, "core/sliceA/module.ttl", _module("sliceA", defines="termA"))
    # Manifest where sliceDependsOn is the LAST predicate (terminal `.`).
    stale_manifest = (
        f"@prefix gmeow: <{_NS}> .\n"
        "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n"
        "@prefix dcterms: <http://purl.org/dc/terms/> .\n\n"
        f"<{_NS}slices/sliceB> a gmeow:Slice ;\n"
        "    gmeow:sliceTier gmeow:tierCore ;\n"
        f'    rdfs:label "sliceB"@x-gmeow-english ;\n'
        f'    dcterms:title "sliceB"@x-gmeow-english ;\n'
        f"    gmeow:sliceDependsOn <{_NS}slices/sliceA> .\n"
    )
    _write(tmp_path, "core/sliceB/manifest.ttl", stale_manifest)
    # sliceB's module defines termB but does NOT reference termA → dep is stale.
    _write(tmp_path, "core/sliceB/module.ttl", _module("sliceB", defines="termB"))

    diffs = compute_fix_deps(tmp_path, apply=False)
    assert diffs, "expected a stale-dependency removal proposal"
    # Apply and confirm the patched manifest is well-formed Turtle with the dep gone.
    _apply_unified_diffs(tmp_path, diffs)
    patched = (tmp_path / "core/sliceB/manifest.ttl").read_text(encoding="utf-8")
    assert _native_discovers(tmp_path), (
        f"patched manifest must be well-formed:\n{patched}"
    )
    assert "sliceDependsOn" not in patched, "stale dependency must be removed"
    # The previous predicate must now carry the terminal `.`.
    assert patched.rstrip().endswith("."), "manifest must remain terminated"


def test_fix_deps_add_produces_wellformed_parseable_turtle(tmp_path: Path) -> None:
    """An UNDECLARED edge add yields well-formed Turtle parseable by oxigraph that
    declares the new dependency on the correct (depending) manifest."""
    _build_two_slices(tmp_path)
    diffs = compute_fix_deps(tmp_path, apply=False)
    assert diffs
    _apply_unified_diffs(tmp_path, diffs)
    patched = (tmp_path / "core/sliceB/manifest.ttl").read_text(encoding="utf-8")
    assert _native_discovers(tmp_path), (
        f"patched manifest must be well-formed:\n{patched}"
    )
    assert f"<{_NS}slices/sliceA>" in patched
    # Re-running fix-deps is now idempotent: the dependency is declared.
    assert compute_fix_deps(tmp_path, apply=False) == [], (
        "after applying, the slice set is fully declared — no further edits"
    )
    # No duplicate sliceDependsOn lines were introduced.
    assert patched.count("sliceDependsOn") == 1


def test_native_binding_detects_cross_slice_conflict(tmp_path: Path) -> None:
    """Two slices each declaring isDefinedBy for the SAME term is a Conflict.

    This is the strictly-stronger check the path-derived per-module lint cannot
    see (each module's isDefinedBy correctly matches its own directory).
    """
    import gmeow_slice

    _write(tmp_path, "core/sliceA/manifest.ttl", _manifest("sliceA"))
    _write(tmp_path, "core/sliceA/module.ttl", _module("sliceA", defines="shared"))
    _write(tmp_path, "core/sliceB/manifest.ttl", _manifest("sliceB"))
    _write(tmp_path, "core/sliceB/module.ttl", _module("sliceB", defines="shared"))

    catalog = gmeow_slice.SliceCatalog.discover(str(tmp_path))
    report = gmeow_slice.OwnershipAnalyzer(catalog).analyze()
    assert report.has_ownership_defect()
    errors = report.ownership_errors()
    assert any("shared" in e and "multiple slices" in e for e in errors)
