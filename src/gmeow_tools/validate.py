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
    _SAMEAS_ALLOWLIST,
    FIXTURES_DIR,
    MAPPING_DSL_DIR,
    NAMESPACE,
    ONTOLOGY_IRI,
    SHAPES_DIR,
    SHAPES_FILE,
    STATEMENT_DSL_DIR,
)
from gmeow_tools.graph import iter_source_files, load_merged_graph
from gmeow_tools.language_tags import check_annotation_literal
from gmeow_tools.reasoning_lint import reasoning_invariants
from gmeow_tools.slices import iter_slice_module_files, iter_slice_shape_files

_DEFINITION = SKOS.definition


@lru_cache(maxsize=4)
def _shapes_graph(shapes_path: Path) -> Graph:
    """Parse (and cache) the SHACL shapes graph.

    ``run_shacl`` runs ~150 times across the test suite; re-parsing the shapes
    Turtle each call wastes ~30 ms apiece. pyshacl does not mutate the shapes
    graph it is handed, so one shared cached parse per path is safe.

    Loads the requested base shapes file plus every other modular ``*.ttl``
    shape file in ``shapes/`` except the DSL-specific lints, so new domain
    shape files (e.g. ``expertise-shapes.ttl``) are picked up automatically.
    """
    graph = Graph().parse(shapes_path, format="turtle")
    dsl_shapes = {
        "mapping-dsl-shapes.ttl",
        "statement-dsl-shapes.ttl",
        "slice-manifest-shapes.ttl",  # targets manifests, not the data graph
        shapes_path.name,
    }
    for extra in sorted(SHAPES_DIR.glob("*.ttl")):
        if extra.name in dsl_shapes:
            continue
        graph.parse(extra, format="turtle")
    for slice_shapes in iter_slice_shape_files():
        graph.parse(slice_shapes, format="turtle")
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


@lru_cache(maxsize=256)
def _parse_file(path: Path) -> tuple[Graph | None, Exception | None]:
    """Parse a single Turtle file, returning (graph, error) for caching.

    Both syntax validation and the sameAs ban scan the same source files.
    Caching the parse avoids redundant I/O when both checks run in sequence.
    """
    try:
        return Graph().parse(path, format="turtle"), None
    except Exception as exc:
        return None, exc


def check_syntax() -> ValidationResult:
    """Parse every source Turtle file individually to catch syntax errors."""
    result = ValidationResult()
    for source in iter_source_files():
        _graph, exc = _parse_file(source)
        if exc is not None:
            result.errors.append(f"syntax error in {source}: {exc}")
    return result


def check_sameas_ban(paths: list[Path] | None = None) -> ValidationResult:
    """Enforce Principle 5: no ``owl:sameAs`` merge with external entities.

    Scans the given Turtle files and errors on every ``owl:sameAs`` triple whose
    object is a URI outside the GMEOW namespace, unless the (subject, object)
    pair is explicitly listed in :data:`gmeow_tools.config._SAMEAS_ALLOWLIST`.

    Args:
        paths: Files to audit. Defaults to canonical ontology sources plus the
            fixture corpus under ``tests/fixtures``.

    Returns:
        Validation result with one error per banned triple.
    """
    if paths is None:
        paths = [
            *iter_source_files(),
            *sorted(FIXTURES_DIR.rglob("*.ttl")),
        ]
    result = ValidationResult()
    for source in paths:
        graph, exc = _parse_file(source)
        if exc is not None:
            result.errors.append(f"failed to parse {source}: {exc}")
            continue
        assert graph is not None
        for s, p, o in graph:
            if p != OWL.sameAs:
                continue
            if not isinstance(o, URIRef):
                continue
            obj = str(o)
            if obj.startswith(NAMESPACE):
                continue
            if (str(s), obj) in _SAMEAS_ALLOWLIST:
                continue
            result.errors.append(
                f"{source}: banned owl:sameAs to external entity "
                f"{s} owl:sameAs {o} (Principle 5); "
                f"use skos:exactMatch or gmeow:authorityLink"
            )
    return result


def _is_gmeow_term(term: URIRef) -> bool:
    """Return whether an IRI is the GMEOW root or lives in its namespace."""
    s = str(term)
    return s.startswith(NAMESPACE) or s == ONTOLOGY_IRI


_TERM_KIND_ORDER = (
    "ontology",
    "class",
    "property",
    "annotation property",
    "datatype",
    "individual",
)


def _term_kind(graph: Graph, term: URIRef) -> str:
    """Return the primary structural kind of a GMEOW term based on its rdf:type."""
    types = set(graph.objects(term, RDF.type))
    if OWL.Ontology in types:
        return "ontology"
    if OWL.Class in types:
        return "class"
    if OWL.AnnotationProperty in types:
        return "annotation property"
    if OWL.ObjectProperty in types or OWL.DatatypeProperty in types:
        return "property"
    if RDFS.Datatype in types:
        return "datatype"
    return "individual"


def _collect_typed_terms(graph: Graph) -> dict[URIRef, str]:
    """Map every GMEOW-namespaced term with an rdf:type to its primary kind.

    Queries by type rather than scanning every subject, then resolves
    multi-typed terms to the most specific kind using the canonical priority.
    """
    terms: dict[URIRef, str] = {}
    typed_queries = (
        OWL.Ontology,
        OWL.Class,
        OWL.ObjectProperty,
        OWL.DatatypeProperty,
        OWL.AnnotationProperty,
        RDFS.Datatype,
    )
    for rdf_type in typed_queries:
        for term in graph.subjects(RDF.type, rdf_type):
            if not isinstance(term, URIRef) or not _is_gmeow_term(term):
                continue
            current = terms.get(term)
            kind = _term_kind(graph, term)
            if current is None or _TERM_KIND_ORDER.index(kind) < _TERM_KIND_ORDER.index(
                current
            ):
                terms[term] = kind
    # Any remaining GMEOW subjects with an explicit rdf:type are treated as individuals.
    for term in set(graph.subjects(RDF.type, None)):
        if isinstance(term, URIRef) and _is_gmeow_term(term) and term not in terms:
            terms[term] = "individual"
    return terms


def structural_lint(graph: Graph) -> ValidationResult:
    """Check that every GMEOW term is fully annotated.

    Every GMEOW-namespaced ontology header, class, property, annotation
    property, datatype, and individual must carry ``rdfs:label``,
    ``skos:definition``, and ``rdfs:isDefinedBy`` (all errors as of issue #221).
    Dangling ``rdfs:subClassOf`` / ``rdfs:subPropertyOf`` targets are reported as
    errors, and a comprehensiveness heuristic warns when a parent class has
    multiple undocumented direct subclasses.

    Args:
        graph: The merged ontology graph.

    Returns:
        The validation result.
    """
    result = ValidationResult()
    typed = _collect_typed_terms(graph)

    for term, kind in sorted(typed.items(), key=lambda x: str(x[0])):
        if (term, RDFS.label, None) not in graph:
            result.errors.append(f"{kind} {term} is missing rdfs:label")
        if (term, _DEFINITION, None) not in graph:
            result.errors.append(f"{kind} {term} is missing skos:definition")
        if (term, RDFS.isDefinedBy, None) not in graph:
            result.errors.append(f"{kind} {term} is missing rdfs:isDefinedBy")

    declared = set(typed)

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

    # Comprehensiveness heuristic: if a GMEOW class has ≥3 direct subclasses
    # in the GMEOW namespace and ≥3 of them lack skos:definition, warn about
    # the parent being under-documented. This surfaces systematic gaps even
    # when the basic per-term check catches each individually. Blank-node
    # restrictions and non-GMEOW children are filtered out.
    parent_to_children: dict[URIRef, list[URIRef]] = {}
    for child, _, parent in graph.triples((None, RDFS.subClassOf, None)):
        if (
            isinstance(child, URIRef)
            and _is_gmeow_term(child)
            and isinstance(parent, URIRef)
            and _is_gmeow_term(parent)
        ):
            parent_to_children.setdefault(parent, []).append(child)
    for parent, children in parent_to_children.items():
        if len(children) < 3:
            continue
        missing = [c for c in children if (c, _DEFINITION, None) not in graph]
        if len(missing) >= 3:
            result.warnings.append(
                f"class {parent} has {len(missing)} of {len(children)} "
                f"direct subclasses missing skos:definition "
                f"(systematic documentation gap)"
            )

    # Ensure all language-tagged string literals on GMEOW properties
    # use the GMEOW-internal 'x-gmeow-' prefix.
    import re

    x_gmeow_pattern = re.compile(r"^x-gmeow-[a-z0-9\-]+$", re.IGNORECASE)
    for s, p, o in graph:
        # Check 1: literal on a GMEOW-namespace predicate.
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

        # Check 2: literal on a standard annotation predicate when the subject
        # is a GMEOW-authored term.
        if (
            isinstance(s, URIRef)
            and isinstance(p, URIRef)
            and isinstance(o, Literal)
            and _is_gmeow_term(s)
        ):
            anno_msg = check_annotation_literal(s, p, o)
            if anno_msg:
                result.errors.append(anno_msg)

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


def slice_ownership_lint(root: Path | None = None) -> ValidationResult:
    """Enforce the per-term ownership convention (#329, Principles 15-16).

    Every ``rdfs:isDefinedBy`` asserted on a GMEOW-namespaced subject inside a
    slice module must point at the *containing slice's IRI* — equality, not
    mere presence. This makes the one-defining-slice rule machine-checked on
    every term and keeps ownership honest through merges and GTS composition
    (a term claiming a foreign owner, or the retired root-pointing form, is an
    error).
    """
    result = ValidationResult()
    modules = iter_slice_module_files(root) if root else iter_slice_module_files()
    for module in modules:
        slice_iri = URIRef(f"{NAMESPACE}slices/{module.parent.name}")
        graph = Graph()
        graph.parse(module, format="turtle")
        for subject, obj in graph.subject_objects(RDFS.isDefinedBy):
            if (
                isinstance(subject, URIRef)
                and _is_gmeow_term(subject)
                and obj != slice_iri
            ):
                result.errors.append(
                    f"{module}: {subject} rdfs:isDefinedBy {obj} — must equal "
                    f"the owning slice IRI {slice_iri} (#329)"
                )
    return result


def check_examples(merged: Graph) -> ValidationResult:
    """Validate every slice example against the ontology + SHACL (#332).

    Examples are canonical worked data, not test scaffolding: each file is
    parsed, merged with the ontology, and SHACL-validated in isolation (so
    one example's IRIs can never mask another's violations). The merged
    graph itself is gated separately, so any NEW violation here belongs to
    the example.
    """
    from gmeow_tools.slices import iter_slice_example_files

    result = ValidationResult()
    for example in iter_slice_example_files():
        data = Graph()
        try:
            data.parse(example, format="turtle")
        except Exception as exc:
            result.errors.append(f"{example}: does not parse: {exc}")
            continue
        union = Graph()
        for triple in merged:
            union.add(triple)
        for triple in data:
            union.add(triple)
        shacl = run_shacl(union)
        for err in shacl.errors:
            result.errors.append(f"example {example.name}: {err}")
        for warn in shacl.warnings:
            result.warnings.append(f"example {example.name}: {warn}")
    return result


def validate_all() -> ValidationResult:
    """Run syntax, structural lint, SHACL, and sameAs-ban checks."""
    result = check_syntax()
    result.extend(check_sameas_ban())
    if not result.ok:
        # No point loading a merged graph if a file will not parse or the
        # sameAs ban is violated.
        return result
    merged = load_merged_graph()
    result.extend(structural_lint(merged))
    result.extend(slice_ownership_lint())
    result.extend(reasoning_lint(merged))
    result.extend(run_shacl(merged))
    result.extend(check_examples(merged))
    result.extend(_dsl_shacl(MAPPING_DSL_DIR, "mapping"))
    result.extend(_dsl_shacl(STATEMENT_DSL_DIR, "statement"))
    return result
