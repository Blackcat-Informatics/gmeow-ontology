"""Tests for Wikidata QID/PID syntax validation (the 'nonsense QID' guard)."""

from __future__ import annotations

from pathlib import Path
from unittest.mock import patch

import pytest

from gmeow_tools.wikidata import (
    NamespaceMisuse,
    SyntaxReport,
    _load_cached,
    _save_cached,
    check_syntax,
    check_syntax_iri,
    is_valid_id,
    is_valid_pid,
    is_valid_qid,
    local_name,
    local_name_wdt,
    to_diagnostics_report,
)


def test_to_diagnostics_report_maps_invalid_and_misuse() -> None:
    report = SyntaxReport(
        valid=["Q42"],
        invalid=["Q0"],
        misuses=[
            (
                "P31",
                NamespaceMisuse.WD_PROP_SHOULD_BE_WDT,
                "wd:P31 should be wdt:P31",
            )
        ],
    )

    diag = to_diagnostics_report(report)

    assert diag.error_count == 2  # both invalid id and misuse are gate-failing
    by_code = {item["code"]: item for item in diag.findings}
    assert set(by_code) == {"wikidata.qid-syntax", "wikidata.namespace-misuse"}
    assert by_code["wikidata.namespace-misuse"]["tags"] == ["wd-prop-should-be-wdt"]


def test_to_diagnostics_report_clean_report_is_ok() -> None:
    diag = to_diagnostics_report(SyntaxReport(valid=["Q42"], invalid=[], misuses=[]))
    assert diag.ok
    assert diag.finding_count == 0


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
    # Strict mode (predicate position) flags wd:P… as misuse
    misuses = check_syntax_iri(
        "http://www.wikidata.org/entity/P275", in_object_position=False
    )
    assert len(misuses) == 1
    assert misuses[0][1] == NamespaceMisuse.WD_PROP_SHOULD_BE_WDT


def test_check_syntax_iri_wd_prop_object_ok() -> None:
    # Object position accepts wd:P… (property-concept reference)
    misuses = check_syntax_iri(
        "http://www.wikidata.org/entity/P275", in_object_position=True
    )
    assert len(misuses) == 0


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


def test_load_cached_logs_on_json_decode_error(
    caplog: pytest.LogCaptureFixture, tmp_path: Path
) -> None:
    with patch("gmeow_tools.wikidata._cache_path") as mock_path:
        cache_file = tmp_path / "test-key.json"
        cache_file.write_text("not json", encoding="utf-8")
        mock_path.return_value = cache_file
        with caplog.at_level("DEBUG", logger="gmeow_tools.wikidata"):
            result = _load_cached("test-key")
        assert result is None
        assert "cache read failed" in caplog.text


def test_save_cached_logs_on_os_error(caplog: pytest.LogCaptureFixture) -> None:
    with patch("pathlib.Path.open", side_effect=OSError("disk full")):
        with caplog.at_level("DEBUG", logger="gmeow_tools.wikidata"):
            _save_cached("test-key", {"data": 1})
        assert "cache write failed" in caplog.text
