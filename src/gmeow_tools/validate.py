"""Validation: Turtle syntax, structural lint, and SHACL conformance.

These checks run in pure Python (no Docker required) so contributors can lint
locally and CI can gate cheaply before the heavier reasoning step.
"""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

from rdflib import RDF, RDFS, Graph, URIRef
from rdflib.namespace import OWL, SKOS

from gmeow_tools.config import NAMESPACE, SHAPES_FILE
from gmeow_tools.graph import iter_source_files, load_merged_graph

_DEFINITION = SKOS.definition


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
    return result


def run_shacl(
    data_graph: Graph, *, shapes_path: Path = SHAPES_FILE
) -> ValidationResult:
    """Validate a data graph against the GMEOW SHACL shapes.

    Args:
        data_graph: The merged ontology graph to validate.
        shapes_path: Path to the SHACL shapes Turtle file.

    Returns:
        The validation result; SHACL violations become errors.

    Raises:
        FileNotFoundError: If the shapes file is missing.
    """
    if not shapes_path.exists():
        raise FileNotFoundError(f"SHACL shapes not found: {shapes_path}")
    # Imported lazily so the lighter syntax/lint checks do not pay the cost.
    from pyshacl import validate as shacl_validate

    shapes_graph = Graph().parse(shapes_path, format="turtle")
    conforms, _report_graph, report_text = shacl_validate(
        data_graph,
        shacl_graph=shapes_graph,
        advanced=True,  # SPARQL-based targets are SHACL-AF
        inference="none",
        abort_on_first=False,
        meta_shacl=False,
    )
    result = ValidationResult()
    if not conforms:
        result.errors.append(f"SHACL validation failed:\n{report_text.strip()}")
    return result


def validate_all() -> ValidationResult:
    """Run syntax, structural lint, and SHACL checks over the merged graph."""
    result = check_syntax()
    if not result.ok:
        # No point loading a merged graph if a file will not parse.
        return result
    merged = load_merged_graph()
    result.extend(structural_lint(merged))
    result.extend(run_shacl(merged))
    return result
