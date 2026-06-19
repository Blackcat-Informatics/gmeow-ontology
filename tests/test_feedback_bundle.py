# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only
"""Tests for the self-describing diagnostics feedback bundle (#654)."""

from __future__ import annotations

import json

import gmeow_diagnostics

from gmeow_tools.feedback_bundle import (
    META_SNAPSHOT_ID,
    REP_FINDINGS,
    REP_SARIF,
    build_feedback_bundle,
    read_report_blobs,
    verify_feedback_bundle,
)


def _report() -> object:
    report = gmeow_diagnostics.Report("validate")
    report.add(
        gmeow_diagnostics.Finding(
            "error",
            "shacl.MinCount",
            "missing property",
            tool="shacl",
            logical="gts:quad",
        )
    )
    return report


def test_feedback_bundle_carries_sarif_and_findings_blobs() -> None:
    bundle = build_feedback_bundle(_report())
    blobs = read_report_blobs(bundle)

    assert REP_SARIF in blobs
    assert REP_FINDINGS in blobs
    sarif = json.loads(blobs[REP_SARIF].decode("utf-8"))
    assert sarif["version"] == "2.1.0"
    flat = json.loads(blobs[REP_FINDINGS].decode("utf-8"))
    assert flat["findings"][0]["code"] == "shacl.MinCount"


def test_feedback_bundle_self_attests() -> None:
    """The embedded report's stamped snapshot id matches the bundle (#654)."""
    bundle = build_feedback_bundle(_report())

    flat = json.loads(read_report_blobs(bundle)[REP_FINDINGS].decode("utf-8"))
    assert flat["metadata"][META_SNAPSHOT_ID].startswith("blake3:")

    assert verify_feedback_bundle(bundle) is True


def test_feedback_bundle_is_deterministic() -> None:
    assert build_feedback_bundle(_report()) == build_feedback_bundle(_report())


def test_empty_report_bundle_round_trips() -> None:
    bundle = build_feedback_bundle(gmeow_diagnostics.Report("validate"))
    assert verify_feedback_bundle(bundle) is True
