"""Tests for the license-policy guard on module extraction."""

from __future__ import annotations

import pytest

from gmeow_tools.extract import LicensePolicyError, guard_importable


def test_guard_allows_import_ok() -> None:
    # Import-ok targets do not raise.
    guard_importable("gufo")
    guard_importable("foaf")
    guard_importable("umbel")


@pytest.mark.parametrize("target", ["dolce", "schema"])
def test_guard_refuses_reference_only(target: str) -> None:
    with pytest.raises(LicensePolicyError):
        guard_importable(target)


def test_guard_refuses_unknown_target() -> None:
    with pytest.raises(LicensePolicyError):
        guard_importable("definitely-not-a-target")
