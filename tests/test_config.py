"""Tests for the license-aware link policy (the core safety mechanism)."""

from __future__ import annotations

from pathlib import Path

import pytest

from gmeow_tools.config import (
    ALIGNMENT_TARGETS,
    PROJECT_ROOT,
    LinkPolicy,
    gmeow_temp_dir,
    policy_for_license,
    sweep_stale_gmeow_temp_dirs,
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


def test_gmeow_temp_dir_uses_prefix() -> None:
    with gmeow_temp_dir() as tmp:
        path = Path(tmp)
        assert path.name.startswith(".gmeow-tmp-")
        assert path.is_relative_to(PROJECT_ROOT)


def test_sweep_stale_gmeow_temp_dirs_removes_old() -> None:
    import time

    with gmeow_temp_dir() as tmp:
        path = Path(tmp)
        # Artificially age the directory
        old_time = time.time() - 7200
        path.touch()
        # Force mtime update (touch updates both, so set mtime explicitly)
        import os

        os.utime(path, (old_time, old_time))
        removed = sweep_stale_gmeow_temp_dirs(max_age_seconds=3600.0)
        assert path in removed
        assert not path.exists()


def test_sweep_stale_gmeow_temp_dirs_leaves_young() -> None:
    with gmeow_temp_dir() as tmp:
        path = Path(tmp)
        removed = sweep_stale_gmeow_temp_dirs(max_age_seconds=3600.0)
        assert path not in removed
        assert path.exists()
