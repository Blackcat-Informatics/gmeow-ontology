"""The universal determinacy vocabulary (#71).

Ontic indeterminacy (crisp, vague, fuzzy, probabilistic, disputed) is held
distinct from epistemic confidence (Principle 9). This module pins that as a
structural invariant: Determinacy is a universal logic:QualityValue, hasDeterminacy
is a domain-free non-functional ObjectProperty orthogonal to confidence, and the
five seeds span the determinacy space with no privileged winner.
"""

from __future__ import annotations

from purrdf.compat.rdflib import Graph, Namespace, URIRef

from gmeow_tools.graph import load_merged_graph
import pytest
pytestmark = pytest.mark.maintainer

GMEOW = "https://blackcatinformatics.ca/gmeow/"
GM = Namespace(GMEOW)


def _graph() -> Graph:
    return load_merged_graph(include_imports=False)


def test_no_preferred_or_primary_term_is_declared() -> None:
    """No GMEOW vocabulary term is a preferred/primary selector (Principle 9)."""
    g = _graph()
    offenders = []
    for s in set(g.subjects()):
        if not isinstance(s, URIRef) or not str(s).startswith(GMEOW):
            continue
        local = str(s)[len(GMEOW) :].lower()
        if "/" not in local and local.startswith(("primary", "preferred")):
            offenders.append(str(s))
    assert offenders == [], f"preferred/primary terms must not exist: {offenders}"
