"""Root pytest configuration for the GMEOW test suite.

Lives at the repository root so the session fixtures and the collection
hook apply to every test path — tests/ and the slice-local tests
(slices/*/*/tests/) alike.

``network``-marked tests reach live external endpoints and are skipped in
automated gates unless explicitly opted in (see below).
"""

from __future__ import annotations

import os

import purrdf
import pytest
from rdflib import Graph

from gmeow_tools import sparql
from gmeow_tools.graph import shared_merged_graph


@pytest.fixture(scope="session")
def merged_graph() -> Graph:
    """The merged ontology as a shared, read-only rdflib graph (no per-test copy).

    Built once per session. Tests MUST NOT mutate it; use ``load_merged_graph()``
    when a mutable graph is needed.
    """
    return shared_merged_graph(include_imports=False)


@pytest.fixture(scope="session")
def merged_store() -> purrdf.Store:
    """The merged ontology as a shared, read-only purrdf store (query only)."""
    return sparql.merged_store(include_imports=False)


#: Opt-in environment variable for the ``network``-marked tests. They reach LIVE
#: external endpoints (Wikidata, BFO, OOPS!/FOOPS!), so they MUST NOT run in any
#: automated gate or CI — a third-party endpoint being slow or down would hang or
#: fail the build for reasons unrelated to our code. They stay available for
#: manual runs: ``GMEOW_RUN_NETWORK=1 uv run pytest -m network`` (or ``make
#: test-network``): network is FORBIDDEN in automation (skip unless explicitly
#: opted in).
_RUN_NETWORK_ENV = "GMEOW_RUN_NETWORK"


def pytest_collection_modifyitems(
    config: pytest.Config, items: list[pytest.Item]
) -> None:
    """Skip network tests unless explicitly opted in."""
    # Explicit truthy check: GMEOW_RUN_NETWORK=0 / =false must NOT opt in (a bare
    # `bool(os.environ.get(...))` treats "0" and "false" as True).
    run_network = os.environ.get(_RUN_NETWORK_ENV, "").lower() in ("1", "true", "yes")
    skip_network = pytest.mark.skip(
        reason=(
            "network test — never run in automated gates/CI; "
            f"opt in manually with {_RUN_NETWORK_ENV}=1"
        )
    )
    for item in items:
        if not run_network and "network" in item.keywords:
            item.add_marker(skip_network)
