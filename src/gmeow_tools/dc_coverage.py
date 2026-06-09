"""Dublin Core mapping coverage reporting.

Measures how much of the Dublin Core vocabulary (DCMI Terms, 15-element set,
DCMI Type) is aligned from GMEOW, grouped by namespace and domain.
Offline by default — no network access required.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path

from gmeow_tools.config import MAPPINGS_DIR, PREFIXES
from gmeow_tools.mappings import (
    Mapping,
    expand_curie,
    group_mappings_by_source,
    load_mappings,
)

_DCTERMS_NS = PREFIXES["dcterms"]
_DC_NS = PREFIXES["dc"]
_DCMITYPE_NS = PREFIXES["dcmitype"]

#: Unqualified Dublin Core 15-element set expected via dumb-down.
_EXPECTED_DC: frozenset[str] = frozenset(
    {
        f"{_DC_NS}title",
        f"{_DC_NS}creator",
        f"{_DC_NS}contributor",
        f"{_DC_NS}subject",
        f"{_DC_NS}description",
        f"{_DC_NS}publisher",
        f"{_DC_NS}date",
        f"{_DC_NS}type",
        f"{_DC_NS}format",
        f"{_DC_NS}identifier",
        f"{_DC_NS}source",
        f"{_DC_NS}language",
        f"{_DC_NS}relation",
        f"{_DC_NS}coverage",
        f"{_DC_NS}rights",
    }
)

#: Core DCMI Terms that are expected to be aligned (the 15 elements + key refinements).
_EXPECTED_DCTERMS: frozenset[str] = frozenset(
    {
        f"{_DCTERMS_NS}title",
        f"{_DCTERMS_NS}creator",
        f"{_DCTERMS_NS}contributor",
        f"{_DCTERMS_NS}subject",
        f"{_DCTERMS_NS}description",
        f"{_DCTERMS_NS}publisher",
        f"{_DCTERMS_NS}date",
        f"{_DCTERMS_NS}type",
        f"{_DCTERMS_NS}format",
        f"{_DCTERMS_NS}identifier",
        f"{_DCTERMS_NS}source",
        f"{_DCTERMS_NS}language",
        f"{_DCTERMS_NS}relation",
        f"{_DCTERMS_NS}coverage",
        f"{_DCTERMS_NS}rights",
        # refinements
        f"{_DCTERMS_NS}created",
        f"{_DCTERMS_NS}modified",
        f"{_DCTERMS_NS}issued",
        f"{_DCTERMS_NS}valid",
        f"{_DCTERMS_NS}available",
        f"{_DCTERMS_NS}dateAccepted",
        f"{_DCTERMS_NS}dateCopyrighted",
        f"{_DCTERMS_NS}dateSubmitted",
        f"{_DCTERMS_NS}abstract",
        f"{_DCTERMS_NS}tableOfContents",
        f"{_DCTERMS_NS}references",
        f"{_DCTERMS_NS}isReferencedBy",
        f"{_DCTERMS_NS}requires",
        f"{_DCTERMS_NS}isRequiredBy",
        f"{_DCTERMS_NS}replaces",
        f"{_DCTERMS_NS}isReplacedBy",
        f"{_DCTERMS_NS}hasPart",
        f"{_DCTERMS_NS}isPartOf",
        f"{_DCTERMS_NS}hasVersion",
        f"{_DCTERMS_NS}isVersionOf",
        f"{_DCTERMS_NS}conformsTo",
        f"{_DCTERMS_NS}license",
        f"{_DCTERMS_NS}rightsHolder",
        f"{_DCTERMS_NS}accessRights",
        f"{_DCTERMS_NS}spatial",
        f"{_DCTERMS_NS}temporal",
        f"{_DCTERMS_NS}bibliographicCitation",
        f"{_DCTERMS_NS}extent",
        f"{_DCTERMS_NS}medium",
        f"{_DCTERMS_NS}audience",
        f"{_DCTERMS_NS}provenance",
    }
)

#: DCMI Type classes expected to be aligned.
_EXPECTED_DCMITYPE: frozenset[str] = frozenset(
    {
        f"{_DCMITYPE_NS}Collection",
        f"{_DCMITYPE_NS}Dataset",
        f"{_DCMITYPE_NS}Event",
        f"{_DCMITYPE_NS}Image",
        f"{_DCMITYPE_NS}MovingImage",
        f"{_DCMITYPE_NS}PhysicalObject",
        f"{_DCMITYPE_NS}Service",
        f"{_DCMITYPE_NS}Software",
        f"{_DCMITYPE_NS}Sound",
        f"{_DCMITYPE_NS}StillImage",
        f"{_DCMITYPE_NS}Text",
        f"{_DCMITYPE_NS}InteractiveResource",
    }
)


@dataclass(slots=True)
class CoverageReport:
    """Result of a Dublin Core coverage analysis."""

    total_dcterms: int = len(_EXPECTED_DCTERMS)
    total_dc: int = len(_EXPECTED_DC)
    total_dcmitype: int = len(_EXPECTED_DCMITYPE)
    mapped_dcterms: set[str] = field(default_factory=set)
    mapped_dc: set[str] = field(default_factory=set)
    mapped_dcmitype: set[str] = field(default_factory=set)
    domain_counts: dict[str, dict[str, int]] = field(default_factory=dict)
    predicate_counts: dict[str, int] = field(default_factory=dict)
    low_confidence: list[tuple[str, str, str, float]] = field(default_factory=list)
    fallback_confidences: int = 0
    threshold: float = 0.5

    @property
    def dcterms_coverage(self) -> float:
        """Ratio of mapped dcterms to expected dcterms."""
        if not self.total_dcterms:
            return 0.0
        return len(self.mapped_dcterms) / self.total_dcterms

    @property
    def dcmitype_coverage(self) -> float:
        """Ratio of mapped dcmitype to expected dcmitype."""
        if not self.total_dcmitype:
            return 0.0
        return len(self.mapped_dcmitype) / self.total_dcmitype

    def gap_dcterms(self) -> frozenset[str]:
        """Return the set of unmapped dcterms."""
        return _EXPECTED_DCTERMS - self.mapped_dcterms

    def gap_dcmitype(self) -> frozenset[str]:
        """Return the set of unmapped dcmitype classes."""
        return _EXPECTED_DCMITYPE - self.mapped_dcmitype


def _is_dc_mapping(mapping: Mapping) -> bool:
    """Return whether a mapping targets a Dublin Core term."""
    try:
        obj = str(expand_curie(mapping.object_id))
    except Exception:
        return False
    return (
        obj.startswith(_DCTERMS_NS)
        or obj.startswith(_DC_NS)
        or obj.startswith(_DCMITYPE_NS)
    )


def _dc_namespace(obj_iri: str) -> str:
    """Return the DC namespace category for an IRI."""
    if obj_iri.startswith(_DCTERMS_NS):
        return "dcterms"
    if obj_iri.startswith(_DC_NS):
        return "dc"
    if obj_iri.startswith(_DCMITYPE_NS):
        return "dcmitype"
    return "other"


_INVALID_CONF: frozenset[str] = frozenset(
    {"", "nan", "inf", "-inf", "infinity", "-infinity"}
)


def _confidence_value(confidence: str | None) -> float:
    """Parse the confidence string into a float.

    Empty, missing, or non-numeric confidence is treated as a conservative 0.0
    so that low-quality input is flagged rather than silently promoted to 1.0.
    """
    if confidence is None:
        return 0.0
    stripped = str(confidence).strip().lower()
    if stripped in _INVALID_CONF:
        return 0.0
    try:
        return float(stripped)
    except (ValueError, TypeError):
        return 0.0


def run_coverage(
    mappings_dir: Path = MAPPINGS_DIR,
    threshold: float = 0.5,
) -> CoverageReport:
    """Analyze Dublin Core mapping coverage.

    Args:
        mappings_dir: Directory containing ``*.sssom.tsv`` files.
        threshold: Confidence values below this are flagged as low-confidence.

    Returns:
        A :class:`CoverageReport` with statistics and gap lists.
    """
    mappings = load_mappings(mappings_dir)
    dc_mappings = [m for m in mappings if _is_dc_mapping(m)]
    groups = group_mappings_by_source(dc_mappings)

    report = CoverageReport(threshold=threshold)

    for mapping in dc_mappings:
        obj_iri = str(expand_curie(mapping.object_id))
        ns = _dc_namespace(obj_iri)
        if ns == "dcterms":
            report.mapped_dcterms.add(obj_iri)
        elif ns == "dc":
            report.mapped_dc.add(obj_iri)
        elif ns == "dcmitype":
            report.mapped_dcmitype.add(obj_iri)

        raw_conf = mapping.confidence if mapping.confidence is not None else ""
        stripped_conf = str(raw_conf).strip()
        is_fallback = stripped_conf.lower() in _INVALID_CONF or (
            stripped_conf != "" and _confidence_value(stripped_conf) == 0.0
        )
        if is_fallback:
            report.fallback_confidences += 1
        conf = _confidence_value(stripped_conf)
        if conf < threshold:
            report.low_confidence.append(
                (mapping.subject_id, mapping.object_id, mapping.predicate_id, conf)
            )

        report.predicate_counts[mapping.predicate_id] = (
            report.predicate_counts.get(mapping.predicate_id, 0) + 1
        )

    for domain, domain_mappings in groups.items():
        report.domain_counts[domain] = {
            "total": len(domain_mappings),
            "exactMatch": sum(
                1 for m in domain_mappings if m.predicate_id == "skos:exactMatch"
            ),
            "closeMatch": sum(
                1 for m in domain_mappings if m.predicate_id == "skos:closeMatch"
            ),
            "relatedMatch": sum(
                1 for m in domain_mappings if m.predicate_id == "skos:relatedMatch"
            ),
        }

    return report


def render_report(report: CoverageReport, json_mode: bool = False) -> str:
    """Render a coverage report as human-readable text or JSON."""
    if json_mode:
        return json.dumps(
            {
                "totals": {
                    "dcterms": report.total_dcterms,
                    "dc": report.total_dc,
                    "dcmitype": report.total_dcmitype,
                },
                "mapped": {
                    "dcterms": len(report.mapped_dcterms),
                    "dc": len(report.mapped_dc),
                    "dcmitype": len(report.mapped_dcmitype),
                },
                "fallback_confidences": report.fallback_confidences,
                "coverage": {
                    "dcterms": round(report.dcterms_coverage, 4),
                    "dcmitype": round(report.dcmitype_coverage, 4),
                },
                "domains": report.domain_counts,
                "predicates": report.predicate_counts,
                "low_confidence": [
                    {"subject": s, "object": o, "predicate": p, "confidence": c}
                    for s, o, p, c in report.low_confidence
                ],
                "gaps": {
                    "dcterms": sorted(report.gap_dcterms()),
                    "dcmitype": sorted(report.gap_dcmitype()),
                },
            },
            indent=2,
        )

    lines: list[str] = []
    lines.append("Dublin Core Mapping Coverage")
    lines.append("=" * 40)
    lines.append("")
    lines.append(
        f"dcterms      {len(report.mapped_dcterms):>4} / {report.total_dcterms:<4} "
        f"({report.dcterms_coverage:.0%})"
    )
    lines.append(
        f"dc           {len(report.mapped_dc):>4} / {report.total_dc:<4} "
        f"(derived dumb-down)"
    )
    lines.append(
        f"dcmitype     {len(report.mapped_dcmitype):>4} / {report.total_dcmitype:<4} "
        f"({report.dcmitype_coverage:.0%})"
    )
    lines.append("")
    lines.append("By domain")
    lines.append("-" * 20)
    for domain, counts in sorted(report.domain_counts.items()):
        lines.append(
            f"  {domain:<40} total={counts['total']:>3}  "
            f"exact={counts['exactMatch']:>3}  close={counts['closeMatch']:>3}  "
            f"related={counts['relatedMatch']:>3}"
        )
    lines.append("")
    lines.append("By predicate")
    lines.append("-" * 20)
    for pred, count in sorted(report.predicate_counts.items(), key=lambda kv: -kv[1]):
        lines.append(f"  {pred:<40} {count}")
    if report.fallback_confidences:
        lines.append("")
        lines.append(
            f"Fallback confidences (treated as 0.0): {report.fallback_confidences}"
        )
    if report.low_confidence:
        lines.append("")
        lines.append(
            f"Low confidence (< {report.threshold}) "
            f"— {len(report.low_confidence)} mappings"
        )
        lines.append("-" * 20)
        for s, o, p, c in report.low_confidence:
            lines.append(f"  {s} → {o} ({p}, {c})")
    gaps_dcterms = report.gap_dcterms()
    if gaps_dcterms:
        lines.append("")
        lines.append(f"Gaps — dcterms ({len(gaps_dcterms)} unmapped)")
        lines.append("-" * 20)
        for term in sorted(gaps_dcterms):
            lines.append(f"  {term}")
    gaps_dcmitype = report.gap_dcmitype()
    if gaps_dcmitype:
        lines.append("")
        lines.append(f"Gaps — dcmitype ({len(gaps_dcmitype)} unmapped)")
        lines.append("-" * 20)
        for term in sorted(gaps_dcmitype):
            lines.append(f"  {term}")
    return "\n".join(lines)
