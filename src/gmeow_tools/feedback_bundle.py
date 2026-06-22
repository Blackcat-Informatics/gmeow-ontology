# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: AGPL-3.0-only

"""Build, read, and verify the self-describing diagnostics feedback bundle (#654).

`gmeow-dev feedback` always emits ``dist/gmeow-feedback.gts``: a self-contained
GTS bundle whose snapshot graph IS the ``gmeow:`` RDF projection of the findings
(SPARQL-queryable), with the SARIF 2.1.0 and flat-JSON projections riding as
content-addressed blob frames. The snapshot content id is stamped into the
report metadata so the bundle is a verifiable self-attestation: re-deriving the
snapshot id from the folded graph must reproduce the stamped value.

The canonical ``gmeow.gts`` never carries a report — only this feedback bundle
does (artifact separation, no flag), per the no-optionality doctrine.
"""

from __future__ import annotations

import json
from typing import Any

import gts
from gmeow_rdf.compat.rdflib import Dataset

from gmeow_tools.gts_producer import _Builder

#: Blob representation labels for the embedded report projections.
REP_SARIF = "gmeow:report/sarif"
REP_FINDINGS = "gmeow:report/findings"  # flat JSON
#: Report-metadata key carrying the snapshot self-attestation content id.
META_SNAPSHOT_ID = "snapshotContentId"


def _findings_dataset(report: Any) -> Dataset:
    """The ``gmeow:`` RDF projection of *report* as an rdflib Dataset."""
    dataset = Dataset()
    nquads = report.to_gmeow_rdf()
    if nquads.strip():
        dataset.parse(data=nquads, format="nquads")
    return dataset


def build_feedback_bundle(report: Any) -> bytes:
    """Embed *report* into a self-describing feedback ``.gts`` bundle.

    The snapshot graph is the findings RDF; SARIF and flat JSON ride as blobs.
    The snapshot content id is stamped into the report metadata before the
    JSON/SARIF projections are rendered, so the embedded report attests to the
    bundle it lives in.
    """
    builder = _Builder()
    builder.add_graph(_findings_dataset(report))
    report.set_metadata_json(
        META_SNAPSHOT_ID, json.dumps(builder.snapshot_content_id())
    )
    sarif = report.to_sarif().encode("utf-8")
    flat = report.to_json().encode("utf-8")
    return builder.to_gts(
        report_blobs=[
            (sarif, "application/sarif+json", REP_SARIF),
            (flat, "application/json", REP_FINDINGS),
        ]
    )


def read_report_blobs(bundle: bytes) -> dict[str, bytes]:
    """Map each embedded report blob's ``rep`` to its decoded payload."""
    folded = gts.read(bundle)
    out: dict[str, bytes] = {}
    for digest, meta in folded.blob_meta.items():
        rep = meta.get("rep")
        payload = folded.blobs.get(digest)
        if rep is not None and payload is not None:
            out[str(rep)] = payload
    return out


def verify_feedback_bundle(bundle: bytes) -> bool:
    """True when the embedded report attests to this bundle's snapshot (#654).

    Re-derives the snapshot content id from the folded findings graph and checks
    it equals the ``snapshotContentId`` the embedded flat-JSON report recorded.

    The bundle is untrusted input: a verifier sits on a trust boundary, so a
    corrupt or tampered bundle (unreadable bytes, malformed JSON, a non-mapping
    payload, an unparsable graph) is simply *not a valid self-attestation* and
    returns ``False`` rather than raising — honoring the boolean contract. This
    guards external input and does not soften the no-optionality doctrine that
    governs our own pipeline's report_json.
    """
    try:
        blobs = read_report_blobs(bundle)
        flat = blobs.get(REP_FINDINGS)
        if flat is None:
            return False
        payload = json.loads(flat)
        if not isinstance(payload, dict):
            return False
        metadata = payload.get("metadata")
        if not isinstance(metadata, dict):
            return False
        stamped = metadata.get(META_SNAPSHOT_ID)
        if stamped is None:
            return False

        folded = gts.read(bundle)
        dataset = Dataset()
        nquads = gts.to_nquads(folded)
        if nquads.strip():
            dataset.parse(data=nquads, format="nquads")
        builder = _Builder()
        builder.add_graph(dataset)
        return bool(stamped == builder.snapshot_content_id())
    except Exception:
        # A bundle we cannot read, fold, or parse cannot attest to itself.
        return False
