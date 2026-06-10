"""Validation: Turtle syntax, structural lint, and SHACL conformance.

These checks run in pure Python (no Docker required) so contributors can lint
locally and CI can gate cheaply before the heavier reasoning step.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from functools import lru_cache
from pathlib import Path

from rdflib import RDF, RDFS, Graph, Literal, URIRef
from rdflib.namespace import OWL, SKOS
from rdflib.term import Node

from gmeow_tools.config import (
    MAPPING_DSL_DIR,
    NAMESPACE,
    ORGANIZATION_SHAPES_FILE,
    SHAPES_FILE,
    SOFTWARE_SHAPES_FILE,
    STATEMENT_DSL_DIR,
)
from gmeow_tools.graph import iter_source_files, load_merged_graph
from gmeow_tools.reasoning_lint import reasoning_invariants

_DEFINITION = SKOS.definition


@lru_cache(maxsize=4)
def _shapes_graph(shapes_path: Path) -> Graph:
    """Parse (and cache) the SHACL shapes graph.

    ``run_shacl`` runs ~150 times across the test suite; re-parsing the shapes
    Turtle each call wastes ~30 ms apiece. pyshacl does not mutate the shapes
    graph it is handed, so one shared cached parse per path is safe.
    """
    graph = Graph().parse(shapes_path, format="turtle")
    if SOFTWARE_SHAPES_FILE.exists():
        graph.parse(SOFTWARE_SHAPES_FILE, format="turtle")
    if ORGANIZATION_SHAPES_FILE.exists():
        graph.parse(ORGANIZATION_SHAPES_FILE, format="turtle")
    return graph


@dataclass(slots=True)
class ValidationResult:
    """Outcome of a validation pass."""

    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)

    @property
    def ok(self) -> bool:
        """Return whether validation passed (no errors)."""
        return not self.errors

    def extend(self, other: ValidationResult) -> None:
        """Merge another result into this one."""
        self.errors.extend(other.errors)
        self.warnings.extend(other.warnings)


def check_syntax() -> ValidationResult:
    """Parse every source Turtle file individually to catch syntax errors."""
    result = ValidationResult()
    for source in iter_source_files():
        try:
            Graph().parse(source, format="turtle")
        except Exception as exc:  # report any parse failure as an error
            result.errors.append(f"syntax error in {source}: {exc}")
    return result


def _is_gmeow_term(term: URIRef) -> bool:
    """Return whether an IRI belongs to the GMEOW namespace."""
    return str(term).startswith(NAMESPACE)


def structural_lint(graph: Graph) -> ValidationResult:
    """Check that every GMEOW term is fully annotated.

    Each GMEOW-namespaced class and property must carry an ``rdfs:label`` and a
    ``skos:definition`` (errors), and should declare ``rdfs:isDefinedBy``
    (warning). Dangling ``rdfs:subClassOf`` targets in the GMEOW namespace that
    are never declared are reported as errors.

    Args:
        graph: The merged ontology graph.

    Returns:
        The validation result.
    """
    result = ValidationResult()
    typed: list[tuple[URIRef, str]] = []
    for cls in graph.subjects(RDF.type, OWL.Class):
        if isinstance(cls, URIRef) and _is_gmeow_term(cls):
            typed.append((cls, "class"))
    for prop_type in (OWL.ObjectProperty, OWL.DatatypeProperty):
        for prop in graph.subjects(RDF.type, prop_type):
            if isinstance(prop, URIRef) and _is_gmeow_term(prop):
                typed.append((prop, "property"))

    declared = {term for term, _ in typed}
    for term, kind in typed:
        if (term, RDFS.label, None) not in graph:
            result.errors.append(f"{kind} {term} is missing rdfs:label")
        if (term, _DEFINITION, None) not in graph:
            result.errors.append(f"{kind} {term} is missing skos:definition")
        if (term, RDFS.isDefinedBy, None) not in graph:
            result.warnings.append(f"{kind} {term} is missing rdfs:isDefinedBy")

    # Dangling GMEOW subclass/subproperty targets.
    for predicate in (RDFS.subClassOf, RDFS.subPropertyOf):
        for _, _, target in graph.triples((None, predicate, None)):
            if (
                isinstance(target, URIRef)
                and _is_gmeow_term(target)
                and target not in declared
            ):
                result.errors.append(
                    f"dangling {predicate} target (undeclared GMEOW term): {target}"
                )

    # Ensure all language-tagged string literals on GMEOW properties
    # use the GMEOW-internal 'x-gmeow-' prefix.
    import re

    x_gmeow_pattern = re.compile(r"^x-gmeow-[a-z0-9\-]+$", re.IGNORECASE)
    for s, p, o in graph:
        if (
            str(p).startswith(NAMESPACE)
            and isinstance(o, Literal)
            and o.language
            and not x_gmeow_pattern.match(o.language)
        ):
            msg = (
                f"literal {o!r} (on subject {s}, predicate {p}) carries "
                f"external or invalid language tag '{o.language}'; "
                f"GMEOW internal data must use the private-use 'x-gmeow-' prefix."
            )
            result.errors.append(msg)

    return result


def reasoning_lint(graph: Graph) -> ValidationResult:
    """Wrap the UFO anti-pattern checks as a :class:`ValidationResult`.

    Each :mod:`gmeow_tools.reasoning_lint` violation (missing/conflicting gUFO
    stereotype, identity conflict, anti-rigidity breach, under-mediated relator)
    becomes an error so ``make validate`` fails if the meta-grounding is incomplete.
    """
    result = ValidationResult()
    result.errors.extend(reasoning_invariants(graph))
    return result


def run_shacl(
    data_graph: Graph, *, shapes_path: Path = SHAPES_FILE
) -> ValidationResult:
    """Validate a data graph against the GMEOW SHACL shapes.

    Args:
        data_graph: The merged ontology graph to validate.
        shapes_path: Path to the SHACL shapes Turtle file.

    Returns:
        The validation result, bucketed by SHACL severity: ``sh:Violation``
        results become errors, while ``sh:Warning`` / ``sh:Info`` results become
        warnings. A warning-only graph therefore still passes (``result.ok`` is
        ``True``) — which is the point of the Warning severity on the suppression
        contract (a source may legitimately lag setting ``gmeow:displayable``).

    Raises:
        FileNotFoundError: If the shapes file is missing.
    """
    if not shapes_path.exists():
        raise FileNotFoundError(f"SHACL shapes not found: {shapes_path}")
    # Imported lazily so the lighter syntax/lint checks do not pay the cost.
    from pyshacl import validate as shacl_validate

    shapes_graph = _shapes_graph(shapes_path)
    conforms, report_graph, report_text = shacl_validate(
        data_graph,
        shacl_graph=shapes_graph,
        advanced=True,  # SPARQL-based targets are SHACL-AF
        inference="none",
        abort_on_first=False,
        meta_shacl=False,
    )
    result = ValidationResult()
    if conforms:
        return result

    violations, warnings = _partition_shacl_results(report_graph)
    if violations:
        result.errors.append("SHACL violations:\n" + "\n".join(violations))
    if warnings:
        result.warnings.append("SHACL warnings:\n" + "\n".join(warnings))
    # Defensive: a non-conforming report we could not parse must still surface.
    if not violations and not warnings:
        result.errors.append(f"SHACL validation failed:\n{report_text.strip()}")
    return result


def _partition_shacl_results(report_graph: Graph) -> tuple[list[str], list[str]]:
    """Split a SHACL report into (violations, warnings) by ``sh:resultSeverity``.

    ``sh:Violation`` results are returned as error lines; ``sh:Warning`` and
    ``sh:Info`` as warning lines. Each line is ``<focusNode>: <message>`` for a
    readable, severity-aware report.
    """
    from rdflib.namespace import SH

    violations: list[str] = []
    warnings: list[str] = []
    for node in report_graph.subjects(RDF.type, SH.ValidationResult):
        severity = report_graph.value(node, SH.resultSeverity)
        message = report_graph.value(node, SH.resultMessage)
        focus = report_graph.value(node, SH.focusNode)
        line = f"{focus}: {message}" if message is not None else str(focus)
        if severity in (SH.Warning, SH.Info):
            warnings.append(line)
        else:
            violations.append(line)
    return violations, warnings


def _dsl_shacl(dsl_dir: Path, label: str) -> ValidationResult:
    """Validate every ``.ttl`` file under *dsl_dir* against its SHACL shapes.

    Returns a :class:`ValidationResult` whose errors carry per-file provenance.
    """
    from gmeow_tools.dsl_validate import (
        validate_mapping_dsl,
        validate_statement_dsl,
    )

    result = ValidationResult()
    graph = Graph()
    node_to_file: dict[Node, Path] = {}
    for path in sorted(dsl_dir.rglob("*.ttl")):
        graph.parse(path, format="turtle")
        file_graph = Graph().parse(path, format="turtle")
        for subject in file_graph.subjects():
            if isinstance(subject, URIRef) and subject not in node_to_file:
                node_to_file[subject] = path
    if label == "mapping":
        violations = validate_mapping_dsl(graph, node_to_file)
    else:
        violations = validate_statement_dsl(graph, node_to_file)
    if violations:
        result.errors.append(
            f"{label} DSL SHACL violations:\n  " + "\n  ".join(violations)
        )
    return result


def validate_all() -> ValidationResult:
    """Run syntax, structural lint, and SHACL checks over the merged graph."""
    result = check_syntax()
    if not result.ok:
        # No point loading a merged graph if a file will not parse.
        return result
    merged = load_merged_graph()
    result.extend(structural_lint(merged))
    result.extend(reasoning_lint(merged))
    result.extend(run_shacl(merged))
    result.extend(_dsl_shacl(MAPPING_DSL_DIR, "mapping"))
    result.extend(_dsl_shacl(STATEMENT_DSL_DIR, "statement"))
    return result
