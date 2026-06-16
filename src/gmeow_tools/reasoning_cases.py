"""Docker-backed reasoning case checks kept out of pytest.

These are the end-to-end ROBOT/HermiT/ELK checks that prove selected
inconsistency and fixture-coherence cases against the real ontology. Pytest
keeps the pure-Python entailment tests; this module owns the live Docker calls
so Make/CI can schedule them independently.
"""

from __future__ import annotations

from rdflib import RDF, Graph, Namespace

from gmeow_tools.config import (
    DIST_DIR,
    EXTERNAL_FIXTURES_DIR,
    FIXTURES_DIR,
    PROJECT_ROOT,
)
from gmeow_tools.reason import MERGED_FILE, merge_release, reason
from gmeow_tools.runner import ToolExecutionError

GMEOW = Namespace("https://blackcatinformatics.ca/gmeow/")
EX = Namespace("https://example.org/test/")

#: Music extension stress-corpus fixtures (#320): authored in the slice but
#: reasoned as an extension of the core ontology.
MUSIC_FIXTURES_DIR = PROJECT_ROOT / "slices" / "extensions" / "music" / "fixtures"


def _is_consistent(extra: Graph, name: str, *, reasoner: str = "hermit") -> bool:
    """Return whether merged ontology plus ``extra`` is consistent."""
    graph = Graph()
    graph.parse(MERGED_FILE, format="turtle")
    graph += extra
    out = DIST_DIR / f"test-{name}.ttl"
    out.parent.mkdir(parents=True, exist_ok=True)
    graph.serialize(destination=out, format="turtle")
    try:
        reason(reasoner=reasoner, merged=out)
        return True
    except ToolExecutionError as exc:
        text = str(exc).lower()
        if "inconsist" in text or "unsatisf" in text:
            return False
        raise
    finally:
        out.unlink(missing_ok=True)


def assert_two_axis_individual_is_inconsistent() -> None:
    """A single individual cannot inhabit two disjoint identity axes."""
    bad = Graph()
    bad.add((EX.x, RDF.type, GMEOW.GenderIdentity))
    bad.add((EX.x, RDF.type, GMEOW.GenderExpression))
    if _is_consistent(bad, "two-axis"):
        raise AssertionError(
            "a GenderIdentity that is also a GenderExpression must be inconsistent"
        )


def assert_two_kind_individual_is_inconsistent() -> None:
    """A single individual cannot inhabit two disjoint ultimate Kinds."""
    bad = Graph()
    bad.add((EX.y, RDF.type, GMEOW.Person))
    bad.add((EX.y, RDF.type, GMEOW.Organization))
    if _is_consistent(bad, "two-kind"):
        raise AssertionError(
            "a Person that is also an Organization must be inconsistent"
        )


def assert_worked_fixtures_stay_coherent_under_disjointness() -> None:
    """GMEOW-authored worked examples stay coherent under broad disjointness."""
    fixtures = Graph()
    fixture_files = [
        p for p in FIXTURES_DIR.rglob("*.ttl") if EXTERNAL_FIXTURES_DIR not in p.parents
    ]
    # Music extension stress corpus (#320) lives in the slice fixtures directory.
    if MUSIC_FIXTURES_DIR.exists():
        fixture_files += sorted(MUSIC_FIXTURES_DIR.glob("*.ttl"))
    if not fixture_files:
        raise AssertionError(
            f"no fixtures found in {FIXTURES_DIR} or {MUSIC_FIXTURES_DIR}"
        )
    for ttl in sorted(fixture_files):
        fixtures.parse(ttl, format="turtle")
    if not _is_consistent(fixtures, "fixtures", reasoner="ELK"):
        raise AssertionError("worked fixtures must stay coherent under disjointness")


def run_all() -> list[str]:
    """Run the Docker-backed reasoning cases and return completed case names."""
    merge_release(MERGED_FILE)
    cases = [
        ("two-axis inconsistency", assert_two_axis_individual_is_inconsistent),
        ("two-kind inconsistency", assert_two_kind_individual_is_inconsistent),
        (
            "worked-fixture coherence",
            assert_worked_fixtures_stay_coherent_under_disjointness,
        ),
    ]
    completed: list[str] = []
    for name, check in cases:
        check()
        completed.append(name)
    return completed
