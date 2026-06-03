"""Tests for Wikidata QID/PID syntax validation (the 'nonsense QID' guard)."""

from __future__ import annotations

import pytest

from gmeow_tools.wikidata import (
    check_syntax,
    is_valid_id,
    is_valid_pid,
    is_valid_qid,
    local_name,
)


@pytest.mark.parametrize(
    ("identifier", "valid"),
    [
        ("Q42", True),
        ("Q5", True),
        ("Q1", True),
        ("Q0", False),  # leading zero / zero not allowed
        ("Q01", False),
        ("Q12abc", False),
        ("Q", False),
        ("42", False),
        ("", False),
        ("P31", True),
        ("P0", False),
    ],
)
def test_is_valid_id(identifier: str, valid: bool) -> None:
    assert is_valid_id(identifier) is valid


def test_qid_vs_pid() -> None:
    assert is_valid_qid("Q42") and not is_valid_qid("P31")
    assert is_valid_pid("P31") and not is_valid_pid("Q42")


def test_local_name() -> None:
    assert local_name("http://www.wikidata.org/entity/Q42") == "Q42"
    assert local_name("http://example.org/Q42") is None


def test_check_syntax_partitions() -> None:
    report = check_syntax(["Q5", "Q0", "P31", "Q12abc"])
    assert set(report.valid) == {"Q5", "P31"}
    assert set(report.invalid) == {"Q0", "Q12abc"}
    assert not report.ok


def test_check_syntax_all_valid() -> None:
    report = check_syntax(["Q5", "Q43229"])
    assert report.ok
