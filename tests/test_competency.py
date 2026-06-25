"""Clock-relative competency retain (the rest migrated to native slice-test cells).

Under #867 every competency question in this module was migrated to a declarative
``gmeow:CompetencyQuestion`` cell executed by the native Rust slice-test harness
(``crates/slicetest``); see ``dsl/tests/MIGRATION-LEDGER.md`` for the per-test
accounting. The TBox-vocabulary questions became per-slice ``competency.ttl`` cells,
and the instance classifiers (deception, expertise) became overlay cells via the new
``gmeow:cqDataFile`` mechanism. The QC ``missing-definitions`` check became the
``cqMissingDefinitions`` cell in the quality slice.

ONE test is deliberately retained here: ``expertise-expiring-credentials``. Its query
selects credentials whose ``gmeow:validUntil`` falls within one year of ``NOW()`` — a
clock-RELATIVE window. No static fixture date can satisfy "within a year of now"
perpetually: a far-future literal falls outside the window, and any fixed near date
becomes a time-bomb that silently reds once wall-clock time passes it. A faithful
native cell would need clock-relative date templating the test-DSL deliberately does
not have, so this stays a pytest retain that builds its data relative to the current
clock at run time (the verification-honesty doctrine: never author a test that
silently breaks later).
"""

from __future__ import annotations

from datetime import UTC, datetime, timedelta

from gmeow_rdf.compat.rdflib import RDF, Graph, Literal, Namespace
from gmeow_rdf.compat.rdflib.namespace import XSD
from gmeow_rdf.compat.rdflib.query import ResultRow

from gmeow_tools.config import COMPETENCY_DIR, NAMESPACE

GMEOW = Namespace(NAMESPACE)
EX = Namespace("https://example.org/test/")


def _query_terms_on_graph(filename: str, graph: Graph) -> set[str]:
    """Run a competency query against a specific graph (for inline-data tests)."""
    query = (COMPETENCY_DIR / filename).read_text(encoding="utf-8")
    terms: set[str] = set()
    for row in graph.query(query):
        assert isinstance(row, ResultRow)
        terms.add(str(row[0]))
    return terms


def test_competency_expertise_expiring_credentials_query() -> None:
    """Expiring-credentials query returns credentials with a near-future expiry.

    Clock-relative: the query window is [NOW(), ~NOW()+1yr], so the fixture must be
    built relative to the current clock. This is why the test is retained in pytest
    rather than migrated to a static slice-test cell (issue #867).
    """
    g = Graph()
    g.add((EX.cred1, RDF.type, GMEOW.Credential))
    g.add((EX.cred1, GMEOW.credentialIssuer, EX.amazon))
    g.add((EX.amazon, RDF.type, GMEOW.Organization))
    # Use a timezone-aware future date so rdflib can compare with NOW().
    expires_soon = datetime.now(UTC) + timedelta(days=180)
    expires_str = expires_soon.isoformat().replace("+00:00", "Z")
    g.add(
        (
            EX.cred1,
            GMEOW.validUntil,
            Literal(expires_str, datatype=XSD.dateTime),
        )
    )
    terms = _query_terms_on_graph("expertise-expiring-credentials.rq", g)
    assert str(EX.cred1) in terms
