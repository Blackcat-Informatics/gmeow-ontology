"""SSSOM alignment-direction gates (issue #25).

PR #24's review caught ``gmeow:subOrganizationOf`` (child→parent) mapped via
``skos:closeMatch`` to ``schema:subOrganization`` (parent→child) — a direction
error that GMEOW-internal domain/range cannot see. These gates reproduce that
class of bug as deterministic checks against the *target* vocabularies' axioms.

The committed-data gate (:func:`test_committed_mappings_have_no_direction_errors`)
runs offline and must stay green. The synthetic gate
(:func:`test_detects_self_contradicting_inverse_mapping`) proves the detector
still works even after the real data is fixed, by re-introducing the bug in a
temporary mapping table.
"""

from __future__ import annotations

from pathlib import Path

import httpx
import pytest

from gmeow_tools.alignment_lint import (
    TARGET_FIXTURE_DIR,
    AlignmentFinding,
    Severity,
    lint_alignment_directions,
)
from gmeow_tools.config import ALIGNMENT_TARGETS, TARGET_SNAPSHOT_DIR, LinkPolicy
from gmeow_tools.graph import iter_source_files

_SSSOM_HEADER = (
    "subject_id\tpredicate_id\tobject_id\tmapping_justification\tconfidence\n"
)


def _errors(findings: list[AlignmentFinding]) -> list[str]:
    return [f.render() for f in findings if f.severity is Severity.ERROR]


def test_committed_mappings_have_no_direction_errors() -> None:
    """The committed SSSOM mappings must contain no direction/character ERRORs."""
    findings = lint_alignment_directions(allow_network=False)
    errors = _errors(findings)
    assert not errors, "alignment-direction errors:\n" + "\n".join(errors)


def test_warnings_are_collected_but_not_fatal() -> None:
    """Warnings (e.g. transitivity nuances) are reported without failing the gate."""
    findings = lint_alignment_directions(allow_network=False)
    warnings = [f for f in findings if f.severity is Severity.WARNING]
    # No assertion on count — warnings are informational. Just confirm the lint
    # produces structured, renderable findings.
    for finding in warnings:
        assert finding.render()


def test_detects_self_contradicting_inverse_mapping(tmp_path: Path) -> None:
    """A property mapped to both a term and its inverse is flagged as an ERROR.

    This re-creates the issue #25 bug in a temp mapping table so the regression
    survives the fix to the real data.
    """
    just = "semapv:ManualMappingCuration"
    rows = [
        (
            "gmeow:subOrganizationOf",
            "owl:equivalentProperty",
            "schema:parentOrganization",
            "0.9",
        ),
        ("gmeow:subOrganizationOf", "skos:closeMatch", "schema:subOrganization", "0.6"),
    ]
    table = tmp_path / "bug.sssom.tsv"
    table.write_text(
        _SSSOM_HEADER + "".join(f"{s}\t{p}\t{o}\t{just}\t{c}\n" for s, p, o, c in rows),
        encoding="utf-8",
    )
    findings = lint_alignment_directions(
        mappings_dir=tmp_path,
        fixture_dir=TARGET_FIXTURE_DIR,
        allow_network=False,
    )
    direction_errors = [
        f
        for f in findings
        if f.severity is Severity.ERROR and f.check == "inverse-direction"
    ]
    assert direction_errors, "self-contradicting inverse mapping was not flagged"
    flagged = direction_errors[0]
    assert flagged.object_id == "schema:subOrganization"
    assert flagged.suggestion == "schema:parentOrganization"


def test_self_inverse_target_is_not_flagged(tmp_path: Path) -> None:
    """A symmetric target (T owl:inverseOf T) must not self-contradict.

    e.g. ``foaf:knows owl:inverseOf foaf:knows`` — the term is its own inverse,
    so a single mapping to it is not a direction conflict.
    """
    snapshots = tmp_path / "snapshots"
    snapshots.mkdir()
    fixtures = tmp_path / "fixtures"
    fixtures.mkdir()
    (fixtures / "foaf.ttl").write_text(
        "@prefix foaf: <http://xmlns.com/foaf/0.1/> .\n"
        "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n"
        "foaf:knows a owl:ObjectProperty ; owl:inverseOf foaf:knows .\n",
        encoding="utf-8",
    )
    cells = [
        "gmeow:hasMet",
        "skos:closeMatch",
        "foaf:knows",
        "semapv:ManualMappingCuration",
        "0.8",
    ]
    table = tmp_path / "m.sssom.tsv"
    table.write_text(_SSSOM_HEADER + "\t".join(cells) + "\n", encoding="utf-8")
    findings = lint_alignment_directions(
        mappings_dir=tmp_path,
        snapshot_dir=snapshots,
        fixture_dir=fixtures,
        allow_network=False,
    )
    inverse = [f for f in findings if f.check == "inverse-direction"]
    assert not inverse, (
        f"self-inverse target wrongly flagged: {[f.render() for f in inverse]}"
    )


def test_refresh_snapshot_refuses_target_without_fetch_source() -> None:
    """An IMPORT_OK target with no fetch source fails with a clear policy error."""
    from gmeow_tools.extract import LicensePolicyError
    from gmeow_tools.target_axioms import TARGET_SOURCES, refresh_snapshot

    # `rel` is CC-BY (IMPORT_OK) but has no entry in TARGET_SOURCES.
    assert "rel" not in TARGET_SOURCES
    with pytest.raises(LicensePolicyError):
        refresh_snapshot("rel")


def test_target_snapshots_stay_out_of_the_published_artifact() -> None:
    """Vendored target snapshots must never enter the published import closure."""
    snapshot_root = TARGET_SNAPSHOT_DIR.resolve()
    for source in iter_source_files(include_imports=True):
        assert snapshot_root not in source.resolve().parents, (
            f"{source} is under imports/targets/ but is in the publish path"
        )


def test_no_reference_only_target_is_vendored() -> None:
    """License guard: only IMPORT_OK targets may be committed under imports/targets/."""
    if not TARGET_SNAPSHOT_DIR.exists():
        pytest.skip("no vendored snapshots present")
    for snapshot in TARGET_SNAPSHOT_DIR.glob("*.ttl"):
        prefix = snapshot.stem
        target = ALIGNMENT_TARGETS.get(prefix)
        assert target is not None, f"snapshot for unknown target {prefix!r}"
        assert target.policy is LinkPolicy.IMPORT_OK, (
            f"{prefix} is {target.policy.value}; its axioms must not be vendored "
            f"(fetch live under --network instead)"
        )


@pytest.mark.network
def test_live_target_axioms_have_no_direction_errors() -> None:
    """Full sweep incl. reference-only targets fetched live — no ERRORs allowed."""
    try:
        findings = lint_alignment_directions(allow_network=True)
    except httpx.HTTPError as exc:  # offline / transient — don't fail CI
        pytest.skip(f"target vocabulary fetch unavailable: {exc}")
    errors = _errors(findings)
    assert not errors, "alignment-direction errors (live):\n" + "\n".join(errors)


def test_dc_refinement_lint_runs() -> None:
    """The DC refinement lint must run without exception and produce findings."""
    from gmeow_tools.alignment_lint import lint_dc_refinement

    findings = lint_dc_refinement()
    # The lint may produce warnings for missing broader elements or hand-authored
    # dc: alignments; we only assert that it runs and returns structured findings.
    for finding in findings:
        assert finding.render()


# --------------------------------------------------------------------------- #
# Equivalence-closure collapse (Principle 5, #284)
# --------------------------------------------------------------------------- #


def _collapses(findings: list[AlignmentFinding]) -> list[AlignmentFinding]:
    return [f for f in findings if f.check == "equivalence-collapse"]


def test_committed_mappings_have_no_equivalence_collapse() -> None:
    """No equivalence chain in the real data connects a disjoint GMEOW pair."""
    findings = lint_alignment_directions(allow_network=False)
    collapsed = _collapses(findings)
    assert not collapsed, "equivalence collapse:\n" + "\n".join(
        f.render() for f in collapsed
    )


def test_detects_collapse_through_external_intermediate(tmp_path: Path) -> None:
    """Two disjoint identity axes bridged via one external term is an ERROR.

    Each cell is individually plausible — only the CLOSURE reveals that the
    two mappings merge gmeow:GenderIdentity with gmeow:GenderExpression,
    which the gender module's owl:AllDisjointClasses matrix forbids.
    """
    just = "semapv:ManualMappingCuration"
    rows = [
        ("gmeow:GenderIdentity", "skos:exactMatch", "schema:GenderType", "0.9"),
        ("gmeow:GenderExpression", "owl:equivalentClass", "schema:GenderType", "0.9"),
    ]
    table = tmp_path / "collapse.sssom.tsv"
    table.write_text(
        _SSSOM_HEADER + "".join(f"{s}\t{p}\t{o}\t{just}\t{c}\n" for s, p, o, c in rows),
        encoding="utf-8",
    )
    findings = lint_alignment_directions(
        mappings_dir=tmp_path,
        fixture_dir=TARGET_FIXTURE_DIR,
        allow_network=False,
    )
    collapsed = _collapses(findings)
    assert collapsed, "equivalence collapse through schema:GenderType not flagged"
    flagged = collapsed[0]
    assert flagged.severity is Severity.ERROR
    assert {flagged.subject_id, flagged.object_id} == {
        "gmeow:GenderIdentity",
        "gmeow:GenderExpression",
    }
    assert "schema:GenderType" in flagged.message  # the chain is named


def test_detects_property_axis_collapse(tmp_path: Path) -> None:
    """The property analog: disjoint RCC-8 relations bridged by equivalence."""
    just = "semapv:ManualMappingCuration"
    rows = [
        ("gmeow:rcc8dc", "owl:equivalentProperty", "schema:geoDisjoint", "0.9"),
        ("gmeow:rcc8ec", "owl:equivalentProperty", "schema:geoDisjoint", "0.9"),
    ]
    table = tmp_path / "collapse-props.sssom.tsv"
    table.write_text(
        _SSSOM_HEADER + "".join(f"{s}\t{p}\t{o}\t{just}\t{c}\n" for s, p, o, c in rows),
        encoding="utf-8",
    )
    findings = lint_alignment_directions(
        mappings_dir=tmp_path,
        fixture_dir=TARGET_FIXTURE_DIR,
        allow_network=False,
    )
    collapsed = _collapses(findings)
    assert collapsed, "property-axis collapse through schema:geoDisjoint not flagged"
    assert collapsed[0].predicate_id == "owl:propertyDisjointWith"


def test_close_match_does_not_bridge_the_collapse_closure(tmp_path: Path) -> None:
    """skos:closeMatch is approximate, not coreference — it must not collapse."""
    just = "semapv:ManualMappingCuration"
    rows = [
        ("gmeow:GenderIdentity", "skos:closeMatch", "schema:GenderType", "0.6"),
        ("gmeow:GenderExpression", "skos:closeMatch", "schema:GenderType", "0.6"),
    ]
    table = tmp_path / "no-collapse.sssom.tsv"
    table.write_text(
        _SSSOM_HEADER + "".join(f"{s}\t{p}\t{o}\t{just}\t{c}\n" for s, p, o, c in rows),
        encoding="utf-8",
    )
    findings = lint_alignment_directions(
        mappings_dir=tmp_path,
        fixture_dir=TARGET_FIXTURE_DIR,
        allow_network=False,
    )
    assert not _collapses(findings)
