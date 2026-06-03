"""Tests for the license-aware link policy (the core safety mechanism)."""

from __future__ import annotations

import pytest

from gmeow_tools.config import (
    ALIGNMENT_TARGETS,
    LinkPolicy,
    policy_for_license,
)


@pytest.mark.parametrize(
    ("license_id", "expected"),
    [
        ("CC-BY-4.0", LinkPolicy.IMPORT_OK),
        ("CC-BY-3.0", LinkPolicy.IMPORT_OK),
        ("CC0-1.0", LinkPolicy.IMPORT_OK),
        ("MIT", LinkPolicy.IMPORT_OK),
        ("Apache-2.0", LinkPolicy.IMPORT_OK),
        ("PDDL-1.0", LinkPolicy.IMPORT_OK),
        ("ODC-BY-1.0", LinkPolicy.IMPORT_OK),
        ("Public-Domain", LinkPolicy.IMPORT_OK),
        ("CC-BY-SA-3.0", LinkPolicy.REFERENCE_ONLY),
        ("CC-BY-NC-ND 4.0", LinkPolicy.REFERENCE_ONLY),
        ("CC-BY-NC-SA 4.0", LinkPolicy.REFERENCE_ONLY),
        ("GPL-2.0", LinkPolicy.REFERENCE_ONLY),
        ("LGPL", LinkPolicy.REFERENCE_ONLY),
        ("EUPL-1.2", LinkPolicy.REFERENCE_ONLY),
        ("Proprietary", LinkPolicy.REFERENCE_ONLY),
        ("SomethingUnknown", LinkPolicy.REFERENCE_ONLY),
    ],
)
def test_policy_for_license(license_id: str, expected: LinkPolicy) -> None:
    assert policy_for_license(license_id) is expected


def test_share_alike_never_import_ok() -> None:
    # CC-BY-SA contains the permissive substring CC-BY but must stay reference-only.
    assert policy_for_license("CC-BY-SA-4.0") is LinkPolicy.REFERENCE_ONLY


def test_public_domain_not_flagged_by_nd_marker() -> None:
    # "Public Domain" must not be mis-flagged by the "ND" marker.
    assert policy_for_license("Public Domain") is LinkPolicy.IMPORT_OK


def test_alignment_targets_policies() -> None:
    # Spot-check that curated targets resolve to the expected policy.
    assert ALIGNMENT_TARGETS["gufo"].policy is LinkPolicy.IMPORT_OK
    assert ALIGNMENT_TARGETS["umbel"].policy is LinkPolicy.IMPORT_OK
    assert ALIGNMENT_TARGETS["foaf"].policy is LinkPolicy.IMPORT_OK
    assert ALIGNMENT_TARGETS["dolce"].policy is LinkPolicy.REFERENCE_ONLY
    assert ALIGNMENT_TARGETS["schema"].policy is LinkPolicy.REFERENCE_ONLY
