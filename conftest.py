"""Root pytest configuration for the GMEOW test suite.

Lives at the repository root so the session fixtures and the docker
hard-fail hook apply to every test path — tests/ and the slice-local
tests (slices/*/*/tests/, #287) alike.

Docker-marked tests that require pinned images (ROBOT, Jena) fail hard when
the image is absent rather than silently skipping.  Missing infrastructure is
a CI failure, not a skip.
"""

from __future__ import annotations

import os

import pyoxigraph
import pytest
from rdflib import Graph

from gmeow_tools import sparql
from gmeow_tools.config import JENA_IMAGE, ROBOT_IMAGE
from gmeow_tools.graph import shared_merged_graph
from gmeow_tools.runner import image_available


@pytest.fixture(scope="session")
def merged_graph() -> Graph:
    """The merged ontology as a shared, read-only rdflib graph (no per-test copy).

    Built once per session. Tests MUST NOT mutate it; use ``load_merged_graph()``
    when a mutable graph is needed.
    """
    return shared_merged_graph(include_imports=False)


@pytest.fixture(scope="session")
def merged_store() -> pyoxigraph.Store:
    """The merged ontology as a shared, read-only pyoxigraph store (query only)."""
    return sparql.merged_store(include_imports=False)


#: Reasons stripped from docker skipif markers, stashed per-item so the
#: hard-fail fixture still knows which image to verify after the marker is
#: removed (removing it is what makes the test RUN instead of skip).
_DOCKER_REASONS = pytest.StashKey[list[str]]()


@pytest.fixture
def _docker_images(request: pytest.FixtureRequest) -> None:
    """Fail hard when a docker test runs but the required image is missing."""
    for reason in request.node.stash.get(_DOCKER_REASONS, []):
        if "ROBOT" in reason and not image_available(ROBOT_IMAGE):
            pytest.fail(f"Hard fail: {reason}", pytrace=False)
        if "Jena" in reason and not image_available(JENA_IMAGE):
            pytest.fail(f"Hard fail: {reason}", pytrace=False)


#: Opt-in environment variable for the ``network``-marked tests. They reach LIVE
#: external endpoints (Wikidata, BFO, OOPS!/FOOPS!), so they MUST NOT run in any
#: automated gate or CI — a third-party endpoint being slow or down would hang or
#: fail the build for reasons unrelated to our code. They stay available for
#: manual runs: ``GMEOW_RUN_NETWORK=1 uv run pytest -m network`` (or ``make
#: test-network``). This is the deliberate inverse of the docker policy above:
#: docker infra is REQUIRED (hard-fail if absent); network is FORBIDDEN in
#: automation (skip unless explicitly opted in).
_RUN_NETWORK_ENV = "GMEOW_RUN_NETWORK"


def pytest_collection_modifyitems(
    config: pytest.Config, items: list[pytest.Item]
) -> None:
    """Skip network tests unless opted in; swap docker skipifs for a hard-fail."""
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
        reasons: list[str] = []
        for marker in list(item.iter_markers("skipif")):
            reason = marker.kwargs.get("reason", "")
            if "ROBOT" in reason or "Jena" in reason:
                reasons.append(reason)
                if marker in item.own_markers:
                    item.own_markers.remove(marker)
        if reasons:
            item.stash[_DOCKER_REASONS] = reasons
            item.add_marker(pytest.mark.usefixtures("_docker_images"))
