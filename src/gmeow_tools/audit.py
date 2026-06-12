# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
# SPDX-License-Identifier: Apache-2.0
"""``gmeow audit`` (#55): the hallucination-resistant gates as one command.

Runs the named audit queries (``queries/audit/``) and the claim SHACL shapes
over ontology + data, reporting ungrounded / contradicted / stale claims —
the difference between "the model said X" and "claim X, asserted by model M,
grounded in this exact span of this source, contradicted by a
higher-confidence claim, source now stale". No LLM calls; flags, never
deletions (P10); verified by construction against the worked fixture (P7).

``--json`` emits the documented flat claim shape (the cookbook's "simple JSON
API" projection): one object per LLM-extracted claim with its evidence spans
and audit flags.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from typing import TYPE_CHECKING

import pyoxigraph
from rdflib import Graph

from gmeow_tools import sparql
from gmeow_tools.config import AUDIT_QUERY_DIR, NAMESPACE
from gmeow_tools.validate import run_shacl

if TYPE_CHECKING:
    from pathlib import Path

#: The three headline questions, in report order.
_HEADLINE = (
    "claims-without-evidence",
    "claims-contradicted-by-higher-confidence",
    "stale-source-claims",
)

_FLAT_QUERY = """
PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
PREFIX rdfs:  <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?claim ?text ?model ?confidence ?span ?chunk ?source ?start ?end ?polarity
WHERE {
    ?claim a gmeow:StandpointClaim ;
           gmeow:observationMethod gmeow:methodLlmExtraction .
    OPTIONAL { ?claim rdfs:label ?text . }
    OPTIONAL { ?claim gmeow:vantage ?model . }
    OPTIONAL { ?claim gmeow:confidence ?confidence . }
    OPTIONAL {
        ?claim gmeow:groundedIn ?span .
        OPTIONAL { ?span gmeow:spanOfChunk ?chunk .
                   OPTIONAL { ?chunk gmeow:chunkOf ?source . } }
        OPTIONAL { ?span gmeow:spanStart ?start . }
        OPTIONAL { ?span gmeow:spanEnd ?end . }
        OPTIONAL { ?span gmeow:supportPolarity ?polarity . }
    }
}
ORDER BY ?claim ?span
"""


@dataclass(slots=True)
class AuditReport:
    """The outcome of one audit pass."""

    #: query stem → result rows (stringified terms).
    findings: dict[str, list[tuple[str, ...]]] = field(default_factory=dict)
    shacl_errors: list[str] = field(default_factory=list)
    shacl_warnings: list[str] = field(default_factory=list)
    claims: list[dict[str, object]] = field(default_factory=list)

    @property
    def flagged(self) -> int:
        """Total headline findings (ungrounded + contradicted + stale)."""
        return sum(len(self.findings.get(name, [])) for name in _HEADLINE)


def _local(term: object) -> str:
    text = str(term)
    return text.removeprefix(NAMESPACE) if text.startswith(NAMESPACE) else text


def _flat_claims(
    store: pyoxigraph.Store, report: AuditReport
) -> list[dict[str, object]]:
    """Assemble the documented flat-JSON claim objects."""
    flagged = {
        name: {row[0] for row in report.findings.get(name, [])} for name in _HEADLINE
    }
    contradicts: dict[str, set[str]] = {}
    for finding in report.findings.get("contradictions", []):
        contradiction_iri, _kind, _detector, member = finding
        contradicts.setdefault(contradiction_iri, set()).add(member)

    claims: dict[str, dict[str, object]] = {}
    for result_row in sparql.select(store, _FLAT_QUERY):
        values: list[str | None] = [
            str(v) if v is not None else None for v in result_row
        ]
        claim = values[0]
        text, model, confidence, span = values[1], values[2], values[3], values[4]
        chunk, source, start, end, polarity = (
            values[5],
            values[6],
            values[7],
            values[8],
            values[9],
        )
        assert claim is not None
        entry = claims.setdefault(
            claim,
            {
                "claim": claim,
                "text": text,
                "model": model,
                "method": "llm-extraction",
                "confidence": float(confidence) if confidence else None,
                "evidence": [],
                "flags": {
                    "ungrounded": claim in flagged["claims-without-evidence"],
                    "contradicted": claim
                    in flagged["claims-contradicted-by-higher-confidence"],
                    "stale": claim in flagged["stale-source-claims"],
                },
                "contradicts": sorted(
                    {
                        other
                        for members in contradicts.values()
                        if claim in members
                        for other in members
                        if other != claim
                    }
                ),
            },
        )
        if span is not None:
            evidence = entry["evidence"]
            assert isinstance(evidence, list)
            evidence.append(
                {
                    "span": span,
                    "chunk": chunk,
                    "source": source,
                    "start": int(start) if start else None,
                    "end": int(end) if end else None,
                    "polarity": _local(polarity) if polarity else None,
                }
            )
    return list(claims.values())


def audit_graph(paths: list[Path], *, include_imports: bool = False) -> AuditReport:
    """Run every audit gate over the ontology plus the given data files."""
    store = sparql.store_with(*paths, include_imports=include_imports)
    report = AuditReport()
    for rq in sorted(AUDIT_QUERY_DIR.glob("*.rq")):
        rows = sparql.select(store, rq.read_text(encoding="utf-8"))
        report.findings[rq.stem] = [
            tuple(str(term) if term is not None else "" for term in row) for row in rows
        ]
    from gmeow_tools.graph import load_merged_graph

    union = Graph()
    for triple in load_merged_graph(include_imports=False):
        union.add(triple)
    for path in paths:
        union.parse(path, format="turtle")
    shacl = run_shacl(union)
    report.shacl_errors = shacl.errors
    report.shacl_warnings = shacl.warnings
    report.claims = _flat_claims(store, report)
    return report


def render_text(report: AuditReport) -> str:
    """Human-readable audit summary."""
    lines: list[str] = []
    for name in _HEADLINE:
        rows = report.findings.get(name, [])
        lines.append(f"{name}: {len(rows)}")
        for row in rows:
            lines.append(f"  {' | '.join(_local(v) for v in row)}")
    coverage = report.findings.get("evidence-coverage", [])
    lines.append(f"claims audited: {len(coverage)}")
    if report.shacl_errors:
        lines.append(f"SHACL errors: {len(report.shacl_errors)}")
    lines.append(f"SHACL warnings: {len(report.shacl_warnings)}")
    return "\n".join(lines)


def render_json(report: AuditReport) -> str:
    """The documented flat-JSON projection of the audited claims."""
    return json.dumps({"claims": report.claims}, indent=2, sort_keys=True)
