"""pytest configuration for the GMEOW test suite.

Docker-marked tests that require pinned images (ROBOT, Jena) fail hard when
the image is absent rather than silently skipping.  Missing infrastructure is
a CI failure, not a skip.
"""

from __future__ import annotations

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


@pytest.fixture
def _docker_images(request: pytest.FixtureRequest) -> None:
    """Fail hard when a docker test runs but the required image is missing."""
    for marker in request.node.iter_markers("skipif"):
        reason = marker.kwargs.get("reason", "")
        if "ROBOT" in reason and not image_available(ROBOT_IMAGE):
            pytest.fail(f"Hard fail: {reason}", pytrace=False)
        if "Jena" in reason and not image_available(JENA_IMAGE):
            pytest.fail(f"Hard fail: {reason}", pytrace=False)


def pytest_collection_modifyitems(
    config: pytest.Config, items: list[pytest.Item]
) -> None:
    """Replace docker-related skipif markers with a hard-fail fixture."""
    for item in items:
        has_docker_skip = False
        for marker in list(item.iter_markers("skipif")):
            reason = marker.kwargs.get("reason", "")
            if "ROBOT" in reason or "Jena" in reason:
                has_docker_skip = True
                if marker in item.own_markers:
                    item.own_markers.remove(marker)
        if has_docker_skip:
            item.add_marker(pytest.mark.usefixtures("_docker_images"))
