# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Suite-local pytest configuration for the classic-cross-check lane.

This lane lives outside the repository's normal workflow (see the top-level
``validations/README.md``), so it cannot rely on the repo-root ``conftest.py``
being loaded — its own ``pytest.ini`` makes this directory pytest's rootdir.

The fixtures and the docker hard-fail hook here mirror the repo-root
``conftest.py`` so the relocated tests behave identically. They **import** the
shared helpers (``shared_merged_graph``, ``sparql.merged_store``,
``image_available``) from ``gmeow_tools`` rather than reimplementing them, so
they cannot drift from the definitions the tests were validated against — a
forward dependency on the built repository, which is intrinsic to a lane that
cross-checks native reasoning against classical oracles.
"""

from __future__ import annotations

import os

import purrdf
import pytest
from rdflib import Graph

from gmeow_tools import sparql
from gmeow_tools.config import JENA_IMAGE, ROBOT_IMAGE
from gmeow_tools.graph import shared_merged_graph
from gmeow_tools.runner import image_available


@pytest.fixture(scope="session")
def merged_graph() -> Graph:
    """The merged ontology as a shared, read-only rdflib graph (no per-test copy)."""
    return shared_merged_graph(include_imports=False)


@pytest.fixture(scope="session")
def merged_store() -> purrdf.Store:
    """The merged ontology as a shared, read-only purrdf store (query only)."""
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


_RUN_NETWORK_ENV = "GMEOW_RUN_NETWORK"


def pytest_collection_modifyitems(
    config: pytest.Config, items: list[pytest.Item]
) -> None:
    """Skip network tests unless opted in; swap docker skipifs for a hard-fail."""
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
