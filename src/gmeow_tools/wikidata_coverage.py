"""Wikidata mapping coverage reporting.

Measures how much of the GMEOW ontology is mapped to Wikidata, grouped by
domain/module.  Offline by default — no network access required.
"""

from __future__ import annotations

import json
from dataclasses import dataclass, field
from pathlib import Path

from gmeow_tools.config import MAPPINGS_DIR, MODULES_DIR, PREFIXES
from gmeow_tools.mappings import (
    Mapping,
    collect_ontology_terms,
    expand_curie,
    group_mappings_by_source,
    load_mappings,
)

_WD_NS = PREFIXES["wd"]
_WDT_NS = PREFIXES["wdt"]


@dataclass(slots=True)
class CoverageReport:
    """Result of a Wikidata coverage analysis."""

    total_classes: int = 0
    total_properties: int = 0
    total_individuals: int = 0
    mapped_classes: set[str] = field(default_factory=set)
    mapped_properties: set[str] = field(default_factory=set)
    mapped_individuals: set[str] = field(default_factory=set)
    domain_counts: dict[str, dict[str, int]] = field(default_factory=dict)
    predicate_counts: dict[str, int] = field(default_factory=dict)
    low_confidence: list[tuple[str, str, str, float]] = field(default_factory=list)
    missing_labels: list[tuple[str, str]] = field(default_factory=list)
    all_classes: set[str] = field(default_factory=set)
    all_properties: set[str] = field(default_factory=set)
    all_individuals: set[str] = field(default_factory=set)
    threshold: float = 0.5

    @property
    def class_coverage(self) -> float:
        """Ratio of mapped classes to total classes."""
        if not self.total_classes:
            return 0.0
        return len(self.mapped_classes) / self.total_classes

    @property
    def property_coverage(self) -> float:
        """Ratio of mapped properties to total properties."""
        if not self.total_properties:
            return 0.0
        return len(self.mapped_properties) / self.total_properties

    @property
    def individual_coverage(self) -> float:
        """Ratio of mapped individuals to total individuals."""
        return (
            len(self.mapped_individuals) / self.total_individuals
            if self.total_individuals
            else 0.0
        )

    def gap_classes(self) -> set[str]:
        """Return the set of unmapped classes."""
        all_classes = self.all_classes or collect_ontology_terms(MODULES_DIR)["classes"]
        return all_classes - self.mapped_classes

    def gap_properties(self) -> set[str]:
        """Return the set of unmapped properties."""
        all_properties = (
            self.all_properties or collect_ontology_terms(MODULES_DIR)["properties"]
        )
        return all_properties - self.mapped_properties

    def gap_individuals(self) -> set[str]:
        """Return the set of unmapped individuals."""
        all_individuals = (
            self.all_individuals or collect_ontology_terms(MODULES_DIR)["individuals"]
        )
        return all_individuals - self.mapped_individuals


def _is_wikidata_mapping(mapping: Mapping) -> bool:
    """Return whether a mapping targets a Wikidata item or property."""
    obj = str(expand_curie(mapping.object_id))
    return obj.startswith(_WD_NS) or obj.startswith(_WDT_NS)


def _term_type(iri: str, terms: dict[str, set[str]] | None = None) -> str | None:
    """Return the kind of ontology term (class, property, individual)."""
    if terms is None:
        terms = collect_ontology_terms(MODULES_DIR)
    if iri in terms["classes"]:
        return "class"
    if iri in terms["properties"]:
        return "property"
    if iri in terms["individuals"]:
        return "individual"
    return None


def _confidence_value(mapping: Mapping) -> float:
    """Parse the confidence string into a float."""
    try:
        return float(mapping.confidence)
    except (ValueError, TypeError):
        return 1.0


def run_coverage(
    mappings_dir: Path = MAPPINGS_DIR,
    threshold: float = 0.5,
) -> CoverageReport:
    """Analyze Wikidata mapping coverage.

    Args:
        mappings_dir: Directory containing ``*.sssom.tsv`` files.
        threshold: Confidence values below this are flagged as low-confidence.

    Returns:
        A :class:`CoverageReport` with statistics and gap lists.
    """
    mappings = load_mappings(mappings_dir)
    wd_mappings = [m for m in mappings if _is_wikidata_mapping(m)]
    groups = group_mappings_by_source(wd_mappings)

    all_terms = collect_ontology_terms(MODULES_DIR)
    report = CoverageReport(
        total_classes=len(all_terms["classes"]),
        total_properties=len(all_terms["properties"]),
        total_individuals=len(all_terms["individuals"]),
        all_classes=all_terms["classes"],
        all_properties=all_terms["properties"],
        all_individuals=all_terms["individuals"],
        threshold=threshold,
    )

    for mapping in wd_mappings:
        subject_iri = str(expand_curie(mapping.subject_id))
        ttype = _term_type(subject_iri, all_terms)
        if ttype == "class":
            report.mapped_classes.add(subject_iri)
        elif ttype == "property":
            report.mapped_properties.add(subject_iri)
        elif ttype == "individual":
            report.mapped_individuals.add(subject_iri)

        conf = _confidence_value(mapping)
        if conf < threshold:
            report.low_confidence.append(
                (mapping.subject_id, mapping.object_id, mapping.predicate_id, conf)
            )

        label = mapping.object_label or ""
        if not label.strip():
            report.missing_labels.append((mapping.subject_id, mapping.object_id))

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
                    "classes": report.total_classes,
                    "properties": report.total_properties,
                    "individuals": report.total_individuals,
                },
                "mapped": {
                    "classes": len(report.mapped_classes),
                    "properties": len(report.mapped_properties),
                    "individuals": len(report.mapped_individuals),
                },
                "coverage": {
                    "classes": round(report.class_coverage, 4),
                    "properties": round(report.property_coverage, 4),
                    "individuals": round(report.individual_coverage, 4),
                },
                "domains": report.domain_counts,
                "predicates": report.predicate_counts,
                "low_confidence": [
                    {"subject": s, "object": o, "predicate": p, "confidence": c}
                    for s, o, p, c in report.low_confidence
                ],
                "missing_labels": [
                    {"subject": s, "object": o} for s, o in report.missing_labels
                ],
                "gaps": {
                    "classes": sorted(report.gap_classes()),
                    "properties": sorted(report.gap_properties()),
                    "individuals": sorted(report.gap_individuals()),
                },
            },
            indent=2,
        )

    lines: list[str] = []
    lines.append("Wikidata Mapping Coverage")
    lines.append("=" * 40)
    lines.append("")
    lines.append(
        f"classes      {len(report.mapped_classes):>4} / {report.total_classes:<4} "
        f"({report.class_coverage:.0%})"
    )
    lines.append(
        f"properties   {len(report.mapped_properties):>4} / "
        f"{report.total_properties:<4} "
        f"({report.property_coverage:.0%})"
    )
    lines.append(
        f"individuals  {len(report.mapped_individuals):>4} / "
        f"{report.total_individuals:<4} "
        f"({report.individual_coverage:.0%})"
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
    if report.low_confidence:
        lines.append("")
        lines.append(
            f"Low confidence (< {report.threshold}) "
            f"— {len(report.low_confidence)} mappings"
        )
        lines.append("-" * 20)
        for s, o, p, c in report.low_confidence:
            lines.append(f"  {s} → {o} ({p}, {c})")
    if report.missing_labels:
        lines.append("")
        lines.append(f"Missing objectLabel — {len(report.missing_labels)} mappings")
        lines.append("-" * 20)
        for s, o in report.missing_labels:
            lines.append(f"  {s} → {o}")
    return "\n".join(lines)
