"""Fixture-level Wikidata auditing.

Scans authored Turtle files (fixtures, ontology modules) for Wikidata IRIs and
detects invalid QIDs/PIDs, namespace misuse, and `owl:sameAs` overuse.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

from rdflib import Graph, URIRef
from rdflib.namespace import OWL

from gmeow_tools.config import MODULES_DIR, PREFIXES
from gmeow_tools.wikidata import NamespaceMisuse, check_syntax_iri

_WD_NS = PREFIXES["wd"]
_WDT_NS = PREFIXES["wdt"]
_SCHEMA_SAMEAS = "https://schema.org/sameAs"


@dataclass(slots=True)
class AuditFinding:
    """One finding from the fixture auditor."""

    file: Path
    subject: str
    predicate: str
    object: str
    severity: str  # "warning" or "error"
    message: str


@dataclass(slots=True)
class AuditReport:
    """Result of auditing a set of Turtle files."""

    findings: list[AuditFinding] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        """Return whether the audit found no errors."""
        return not any(f.severity == "error" for f in self.findings)

    @property
    def warnings(self) -> list[AuditFinding]:
        """Return only warning-level findings."""
        return [f for f in self.findings if f.severity == "warning"]

    @property
    def errors(self) -> list[AuditFinding]:
        """Return only error-level findings."""
        return [f for f in self.findings if f.severity == "error"]


def _is_wikidata_iri(iri: str) -> bool:
    return (
        iri.startswith(_WD_NS)
        or iri.startswith(_WDT_NS)
        or iri.startswith(_SCHEMA_SAMEAS)
    )


def audit_file(path: Path) -> list[AuditFinding]:
    """Audit a single Turtle file for Wikidata misuse."""
    findings: list[AuditFinding] = []
    try:
        graph = Graph()
        graph.parse(path, format="turtle")
    except Exception:
        return findings  # skip unparsable files

    for s, p, o in graph:
        if not isinstance(o, URIRef):
            continue
        obj = str(o)
        pred = str(p)

        # Invalid or misused Wikidata IRIs
        if (
            obj.startswith(_WD_NS)
            or obj.startswith(_WDT_NS)
            or obj.startswith("https://www.wikidata.org/entity/")
        ):
            misuses = check_syntax_iri(obj)
            for _local, misuse, message in misuses:
                severity = (
                    "error" if misuse is NamespaceMisuse.BAD_SYNTAX else "warning"
                )
                findings.append(
                    AuditFinding(
                        file=path,
                        subject=str(s),
                        predicate=pred,
                        object=obj,
                        severity=severity,
                        message=message,
                    )
                )

        # owl:sameAs overuse involving Wikidata
        if pred == str(OWL.sameAs) and (
            obj.startswith(_WD_NS) or obj.startswith(_WDT_NS)
        ):
            findings.append(
                AuditFinding(
                    file=path,
                    subject=str(s),
                    predicate=pred,
                    object=obj,
                    severity="warning",
                    message=(
                        "owl:sameAs to Wikidata risks standpoint collapse; "
                        "prefer skos:exactMatch or gmeow:authorityLink"
                    ),
                )
            )

        # schema:sameAs with Wikidata entity (acceptable but worth noting)
        if pred == _SCHEMA_SAMEAS and obj.startswith(_WD_NS):
            findings.append(
                AuditFinding(
                    file=path,
                    subject=str(s),
                    predicate=pred,
                    object=obj,
                    severity="warning",
                    message=(
                        "schema:sameAs to Wikidata entity — "
                        "ensure this is a profile link, not ontology alignment"
                    ),
                )
            )

    return findings


def audit_files(paths: list[Path]) -> AuditReport:
    """Audit a list of Turtle files."""
    findings: list[AuditFinding] = []
    for path in paths:
        findings.extend(audit_file(path))
    return AuditReport(findings=findings)


def audit_all(
    fixtures_dir: Path | None = None,
    modules_dir: Path = MODULES_DIR,
) -> AuditReport:
    """Audit all fixtures and ontology modules for Wikidata misuse."""
    paths: list[Path] = []
    if fixtures_dir is not None:
        paths.extend(sorted(fixtures_dir.rglob("*.ttl")))
    paths.extend(sorted(modules_dir.glob("*.ttl")))
    return audit_files(paths)


def render_audit(report: AuditReport) -> str:
    """Render audit findings as human-readable text."""
    lines: list[str] = []
    lines.append("Wikidata Fixture Audit")
    lines.append("=" * 40)
    lines.append("")
    if not report.findings:
        lines.append("No issues found.")
        return "\n".join(lines)

    for finding in report.findings:
        emoji = (
            "[yellow]warning[/yellow]"
            if finding.severity == "warning"
            else "[red]error[/red]"
        )
        lines.append(
            f"{emoji} {finding.file.name} — "
            f"{finding.subject} {finding.predicate} {finding.object}"
        )
        lines.append(f"    {finding.message}")
    lines.append("")
    lines.append(
        f"Totals: {len(report.errors)} error(s), {len(report.warnings)} warning(s)"
    )
    return "\n".join(lines)
