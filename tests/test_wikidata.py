"""Tests for Wikidata QID/PID syntax validation (the 'nonsense QID' guard)."""

from __future__ import annotations

import pytest

from gmeow_tools.wikidata import (
    NamespaceMisuse,
    check_syntax,
    check_syntax_iri,
    is_valid_id,
    is_valid_pid,
    is_valid_qid,
    local_name,
    local_name_wdt,
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


def test_local_name_wdt() -> None:
    assert local_name_wdt("http://www.wikidata.org/prop/direct/P31") == "P31"
    assert local_name_wdt("http://www.wikidata.org/entity/Q42") is None


def test_check_syntax_partitions() -> None:
    report = check_syntax(["Q5", "Q0", "P31", "Q12abc"])
    assert set(report.valid) == {"Q5", "P31"}
    assert set(report.invalid) == {"Q0", "Q12abc"}
    assert not report.ok


def test_check_syntax_all_valid() -> None:
    report = check_syntax(["Q5", "Q43229"])
    assert report.ok


def test_check_syntax_iri_https_url() -> None:
    misuses = check_syntax_iri("https://www.wikidata.org/entity/Q42")
    assert len(misuses) == 1
    assert misuses[0][1] == NamespaceMisuse.HTTPS_URL_SHOULD_BE_CURIE


def test_check_syntax_iri_wd_prop() -> None:
    misuses = check_syntax_iri("http://www.wikidata.org/entity/P275")
    assert len(misuses) == 1
    assert misuses[0][1] == NamespaceMisuse.WD_PROP_SHOULD_BE_WDT


def test_check_syntax_iri_wdt_item() -> None:
    misuses = check_syntax_iri("http://www.wikidata.org/prop/direct/Q42")
    assert len(misuses) == 1
    assert misuses[0][1] == NamespaceMisuse.WDT_ITEM_SHOULD_BE_WD


def test_check_syntax_iri_valid_wd() -> None:
    misuses = check_syntax_iri("http://www.wikidata.org/entity/Q42")
    assert len(misuses) == 0


def test_check_syntax_iri_valid_wdt() -> None:
    misuses = check_syntax_iri("http://www.wikidata.org/prop/direct/P31")
    assert len(misuses) == 0


def test_check_syntax_iri_bad_syntax() -> None:
    misuses = check_syntax_iri("http://www.wikidata.org/entity/Q0")
    assert len(misuses) == 1
    assert misuses[0][1] == NamespaceMisuse.BAD_SYNTAX
