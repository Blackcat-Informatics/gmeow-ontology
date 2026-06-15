"""Validation: Turtle syntax, structural lint, and SHACL conformance.

These checks run in pure Python (no Docker required) so contributors can lint
locally and CI can gate cheaply before the heavier reasoning step.
"""

from __future__ import annotations

import hashlib
import json
import re
import tempfile
from collections.abc import Callable
from contextlib import suppress
from dataclasses import dataclass, field
from functools import lru_cache, partial
from importlib import metadata
from pathlib import Path

import pyoxigraph
from rdflib import RDF, RDFS, Graph, Literal, URIRef
from rdflib.namespace import OWL, SKOS
from rdflib.term import Node

from gmeow_tools.config import (
    _SAMEAS_ALLOWLIST,
    EXTERNAL_FIXTURES_DIR,
    FIXTURES_DIR,
    GENERATED_SHAPES_DIR,
    MAPPING_DSL_DIR,
    NAMESPACE,
    ONTOLOGY_IRI,
    PROJECT_ROOT,
    SHAPES_DIR,
    SHAPES_FILE,
    SLICES_DIR,
    STATEMENT_DSL_DIR,
)
from gmeow_tools.graph import iter_source_files, load_merged_graph
from gmeow_tools.language_tags import check_annotation_literal
from gmeow_tools.reasoning_lint import reasoning_invariants
from gmeow_tools.slices import (
    iter_slice_example_files,
    iter_slice_module_files,
    iter_slice_shape_files,
)

_DEFINITION = SKOS.definition
_USE_WHEN = URIRef(NAMESPACE + "useWhen")
_AVOID_WHEN = URIRef(NAMESPACE + "avoidWhen")
_HOW_TO_USE = URIRef(NAMESPACE + "howToUse")
_USE_FOR_CONSUMER = URIRef(NAMESPACE + "useForConsumer")
_AVOID_FOR_CONSUMER = URIRef(NAMESPACE + "avoidForConsumer")
_PROJECTION_CONTEXT = URIRef(NAMESPACE + "ProjectionContext")
_TURTLE = pyoxigraph.RdfFormat.TURTLE

type _OxParseResult = tuple[tuple[pyoxigraph.Quad, ...] | None, Exception | None]


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
    # Generated shapes (#283): frame-relativity derived from gmeow:requiresFrame.
    # FAIL CLOSED: the hand-written frame constraints were deleted in favor of
    # these, so their absence would silently stop enforcing P11.
    generated_shapes = sorted(GENERATED_SHAPES_DIR.glob("*.ttl"))
    if not generated_shapes:
        msg = (
            f"no generated shapes under {GENERATED_SHAPES_DIR} — "
            f"run `gmeow regenerate frame-shapes` (P11 enforcement lives there)"
        )
        raise FileNotFoundError(msg)
    for generated in generated_shapes:
        graph.parse(generated, format="turtle")
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


_VALIDATION_CACHE_DIR = PROJECT_ROOT / ".cache" / "validate"


def _cache_key(parts: list[str]) -> str:
    """Return a short stable hash for cache-key parts."""
    h = hashlib.sha256()
    for part in parts:
        h.update(part.encode("utf-8"))
        h.update(b"\0")
    return h.hexdigest()[:16]


def _files_cache_key(paths: list[Path]) -> str:
    """Return a content hash for validation cache inputs."""
    from gmeow_tools.generator import source_hash

    return source_hash(paths)


def _validation_toolchain_salt() -> str:
    """Return a cache salt for validator package versions."""
    parts: list[str] = []
    for package in ("pyshacl", "pyoxigraph", "rdflib"):
        try:
            version = metadata.version(package)
        except metadata.PackageNotFoundError:
            version = "missing"
        parts.append(f"{package}={version}")
    return _cache_key(parts)


def _validation_cache_path(kind: str, key: str) -> Path:
    """Return the validation cache path for a keyed result."""
    safe_kind = re.sub(r"[^A-Za-z0-9_.-]+", "-", kind)
    return _VALIDATION_CACHE_DIR / safe_kind / f"{key}.json"


def _read_cached_result(path: Path) -> ValidationResult | None:
    """Read a cached validation result if present and valid."""
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None
    if not isinstance(payload, dict):
        return None
    errors = payload.get("errors")
    warnings = payload.get("warnings")
    if not isinstance(errors, list) or not isinstance(warnings, list):
        return None
    if not all(isinstance(item, str) for item in errors + warnings):
        return None
    return ValidationResult(errors=list(errors), warnings=list(warnings))


def _write_cached_result(path: Path, result: ValidationResult) -> None:
    """Persist a validation result, best-effort."""
    tmp_path: Path | None = None
    try:
        path.parent.mkdir(parents=True, exist_ok=True)
        payload = json.dumps(
            {"errors": result.errors, "warnings": result.warnings},
            sort_keys=True,
        )
        with tempfile.NamedTemporaryFile(
            "w",
            dir=path.parent,
            encoding="utf-8",
            prefix=f".{path.name}.",
            suffix=".tmp",
            delete=False,
        ) as handle:
            tmp_path = Path(handle.name)
            handle.write(payload)
        tmp_path.replace(path)
    except OSError:
        if tmp_path is not None:
            with suppress(OSError):
                tmp_path.unlink(missing_ok=True)


def _cached_result(
    kind: str, key: str, compute: Callable[[], ValidationResult]
) -> ValidationResult:
    """Return a cached validation result, computing and storing on miss."""
    path = _validation_cache_path(kind, key)
    cached = _read_cached_result(path)
    if cached is not None:
        return cached
    result = compute()
    _write_cached_result(path, result)
    return result


@lru_cache(maxsize=512)
def _parse_file_ox(path: Path) -> _OxParseResult:
    """Parse a single Turtle file with pyoxigraph, returning (quads, error).

    Syntax validation and the sameAs ban scan overlapping source files. Keeping
    that pass on pyoxigraph avoids building short-lived rdflib graphs for gates
    that only need parse success plus a predicate/object scan.
    """
    try:
        return tuple(pyoxigraph.parse(path.read_bytes(), format=_TURTLE)), None
    except Exception as exc:
        return None, exc


def _ox_term_display(term: object) -> str:
    """Return a readable RDF term string for pyoxigraph-backed diagnostics."""
    if isinstance(term, pyoxigraph.NamedNode):
        return term.value
    if isinstance(term, pyoxigraph.BlankNode):
        return f"_:{term.value}"
    return str(term)


def check_syntax() -> ValidationResult:
    """Parse every source Turtle file individually to catch syntax errors."""
    result = ValidationResult()
    for source in iter_source_files():
        _quads, exc = _parse_file_ox(source)
        if exc is not None:
            result.errors.append(f"syntax error in {source}: {exc}")
    return result


def _authored_fixtures() -> list[Path]:
    """Coverage fixtures that GMEOW authors — everything except external snapshots.

    The ``external/`` subtree holds verbatim real-world site dumps (parity
    targets); Principle 5 is a policy on our own RDF, not a rule we can impose on
    the outside world, so those snapshots are excluded from the authoring gates.
    """
    return sorted(
        p for p in FIXTURES_DIR.rglob("*.ttl") if EXTERNAL_FIXTURES_DIR not in p.parents
    )


def check_sameas_ban(paths: list[Path] | None = None) -> ValidationResult:
    """Enforce Principle 5: no ``owl:sameAs`` merge with external entities.

    Scans the given Turtle files and errors on every ``owl:sameAs`` triple whose
    object is a URI outside the GMEOW namespace, unless the (subject, object)
    pair is explicitly listed in :data:`gmeow_tools.config._SAMEAS_ALLOWLIST`.

    Args:
        paths: Files to audit. Defaults to canonical ontology sources plus the
            GMEOW-authored fixtures (the ``external/`` snapshot subtree is
            exempt — see :func:`_authored_fixtures`).

    Returns:
        Validation result with one error per banned triple.
    """
    if paths is None:
        paths = [
            *iter_source_files(),
            *_authored_fixtures(),
        ]
    if not paths:
        raise ValueError("check_sameas_ban: paths to audit must not be empty")
    result = ValidationResult()
    for source in paths:
        quads, exc = _parse_file_ox(source)
        if exc is not None:
            result.errors.append(f"failed to parse {source}: {exc}")
            continue
        assert quads is not None
        for quad in quads:
            subject = quad.subject
            predicate = quad.predicate
            obj_term = quad.object
            if not isinstance(
                predicate, pyoxigraph.NamedNode
            ) or predicate.value != str(OWL.sameAs):
                continue
            if not isinstance(obj_term, pyoxigraph.NamedNode):
                continue
            obj = obj_term.value
            if obj.startswith(NAMESPACE):
                continue
            subject_text = _ox_term_display(subject)
            if (subject_text, obj) in _SAMEAS_ALLOWLIST:
                continue
            result.errors.append(
                f"{source}: banned owl:sameAs to external entity "
                f"{subject_text} owl:sameAs {obj} (Principle 5); "
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


#: CamelCase tokens that mark a selector privileging one co-equal claim (P9).
_SELECTOR_TOKENS = frozenset({"primary", "preferred", "default", "main"})

_CAMEL_SPLIT = re.compile(r"[A-Z]?[a-z0-9]+|[A-Z]+(?![a-z])")


def term_naming_lint(graph: Graph) -> ValidationResult:
    """Principle 9 by annotation (#281): no selector names on ontology terms.

    Extends :func:`gmeow_tools.statement_lint.no_preferred_rank` from
    statement-annotation properties to every GMEOW term local name: a
    camelCase token of ``primary``/``preferred``/``default``/``main`` marks a
    selector that would privilege one co-equal claim over another. Legitimate
    value-vocabulary names (``scriptRolePrimary``, ``sourceTierPrimary``)
    carry an explicit ``gmeow:namingNote`` justification — the lint enforces
    the judgment instead of relying on audit-time discretion.
    """
    naming_note = URIRef(NAMESPACE + "namingNote")
    result = ValidationResult()
    for term, kind in sorted(_collect_typed_terms(graph).items()):
        local = str(term).removeprefix(NAMESPACE)
        tokens = {t.lower() for t in _CAMEL_SPLIT.findall(local)}
        offending = tokens & _SELECTOR_TOKENS
        if not offending:
            continue
        if next(graph.objects(term, naming_note), None) is not None:
            continue
        result.errors.append(
            f"{kind} gmeow:{local} carries the selector token "
            f"{sorted(offending)[0]!r} (Principle 9: co-equal claims have no "
            f"primary/preferred/default/main); rename it, or justify a "
            f"value-vocabulary use with gmeow:namingNote"
        )
    return result


_SLICES_CACHE: dict[str, object] = {}


def _discover_slices_cached() -> dict[str, object]:
    """Slice registry for tier lookups (cached per process)."""
    if not _SLICES_CACHE:
        from gmeow_tools.slices import discover_slices

        _SLICES_CACHE.update(discover_slices())
    return _SLICES_CACHE


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

    # Tier-1 depth, graded (#325/#471): public-facing terms — classes and
    # object/datatype properties in CORE slices — should also carry advisory
    # useWhen/howToUse metadata; worked skos:example coverage is nudged only
    # after howToUse exists, so warnings stay additive instead of duplicative.
    # WARNINGS for now; severity is promoted once coverage is high (tracked by
    # the module-status matrix). Annotation properties, individuals, and
    # extension-tier terms are exempt at this grade.
    core_slice_iris = {
        URIRef(s.iri)  # type: ignore[attr-defined]
        for s in _discover_slices_cached().values()
        if s.tier == "core"  # type: ignore[attr-defined]
    }
    for term, kind in sorted(typed.items(), key=lambda x: str(x[0])):
        if kind not in ("class", "property"):
            continue
        defined_by = set(graph.objects(term, RDFS.isDefinedBy))
        if not (defined_by & core_slice_iris):
            continue
        if (term, _USE_WHEN, None) not in graph:
            result.warnings.append(
                f"{kind} {term} is missing gmeow:useWhen (Tier-1 depth, #471)"
            )
        has_how_to_use = (term, _HOW_TO_USE, None) in graph
        if not has_how_to_use:
            result.warnings.append(
                f"{kind} {term} is missing gmeow:howToUse (Tier-1 depth, #471)"
            )
        elif (term, SKOS.example, None) not in graph:
            result.warnings.append(
                f"{kind} {term} has gmeow:howToUse but no skos:example "
                f"(Tier-1 depth, #471)"
            )

    for predicate in (_USE_FOR_CONSUMER, _AVOID_FOR_CONSUMER):
        for subject, _, consumer in graph.triples((None, predicate, None)):
            if (
                not isinstance(consumer, URIRef)
                or (
                    consumer,
                    RDF.type,
                    _PROJECTION_CONTEXT,
                )
                not in graph
            ):
                result.errors.append(
                    f"{predicate} on {subject} points to non-ProjectionContext "
                    f"value {consumer}"
                )

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


def _shacl_cache_inputs(shapes_path: Path = SHAPES_FILE) -> list[Path]:
    """Return files that affect normal SHACL validation outcomes."""
    dsl_shapes = {
        "mapping-dsl-shapes.ttl",
        "statement-dsl-shapes.ttl",
        "slice-manifest-shapes.ttl",
        shapes_path.name,
    }
    return [
        Path(__file__),
        shapes_path,
        *sorted(
            extra for extra in SHAPES_DIR.glob("*.ttl") if extra.name not in dsl_shapes
        ),
        *sorted(GENERATED_SHAPES_DIR.glob("*.ttl")),
        *iter_slice_shape_files(),
    ]


def _merged_shacl_cache_key() -> str:
    """Return the content key for merged ontology SHACL validation."""
    return _cache_key(
        [
            _files_cache_key([*iter_source_files(), *_shacl_cache_inputs()]),
            _validation_toolchain_salt(),
        ]
    )


def _dsl_shacl_cache_key(dsl_dir: Path, label: str) -> str:
    """Return the content key for DSL SHACL validation."""
    dsl_validate_source = PROJECT_ROOT / "src" / "gmeow_tools" / "dsl_validate.py"
    return _cache_key(
        [
            _files_cache_key(
                [
                    Path(__file__),
                    dsl_validate_source,
                    *sorted(dsl_dir.rglob("*.ttl")),
                    *sorted(SHAPES_DIR.glob("*.ttl")),
                ]
            ),
            label,
            _validation_toolchain_salt(),
        ]
    )


def _run_example_shacl(merged: Graph, data: Graph) -> ValidationResult:
    """Validate one example graph against the merged ontology graph."""
    return run_shacl(merged + data)


def check_examples(
    merged: Graph, *, base_cache_key: str | None = None
) -> ValidationResult:
    """Validate every slice example against the ontology + SHACL (#332).

    Examples are canonical worked data, not test scaffolding: each file is
    parsed, merged with the ontology, and SHACL-validated in isolation. Results
    are cached by ontology/shapes/example content so repeated local and CI runs
    do not spend minutes revalidating unchanged examples.
    """
    result = ValidationResult()
    base_cache_key = base_cache_key or _merged_shacl_cache_key()
    for example in iter_slice_example_files():
        name = example.relative_to(SLICES_DIR).as_posix()
        data = Graph()
        try:
            data.parse(example, format="turtle")
        except Exception as exc:
            result.errors.append(f"example {name}: does not parse: {exc}")
            continue
        example_key = _cache_key([base_cache_key, _files_cache_key([example])])
        shacl = _cached_result(
            "example-shacl",
            example_key,
            partial(_run_example_shacl, merged, data),
        )
        for err in shacl.errors:
            result.errors.append(f"example {name}: {err}")
        for warn in shacl.warnings:
            result.warnings.append(f"example {name}: {warn}")
    return result


_ANCHOR_PATTERN = re.compile(r"^###\s+`?gmeow:([A-Za-z][A-Za-z0-9]*)`?", re.MULTILINE)
# Term headings at the wrong depth are malformed anchors, not invisible ones —
# the canonical Tier-2 anchor shape is exactly `### gmeow:Term`.
_MALFORMED_ANCHOR_PATTERN = re.compile(
    r"^(?:##|#{4,})\s+`?gmeow:([A-Za-z][A-Za-z0-9]*)`?", re.MULTILINE
)
_STUB_MARKER = "This is a STUB guide"


def guide_anchor_lint(graph: Graph, root: Path | None = None) -> ValidationResult:
    """Tier-2 structural binding (#325): guides are bound to the graph.

    Every slice must carry a non-stub ``docs.md`` whose ``### gmeow:X``
    heading anchors resolve to declared GMEOW terms — a renamed term breaks
    the build, and a slice without a guide fails. Anchors owned by another
    slice are legal cross-references; anchors matching no term are errors.
    """
    from gmeow_tools.config import SLICES_DIR

    result = ValidationResult()
    declared = set(_collect_typed_terms(graph))
    base = root if root is not None else SLICES_DIR
    for manifest in sorted(base.glob("*/*/manifest.ttl")):
        slice_dir = manifest.parent
        name = slice_dir.name
        guide = slice_dir / "docs.md"
        if not guide.exists():
            result.errors.append(f"slice {name}: missing docs.md guide (#325 Tier-2)")
            continue
        text = guide.read_text(encoding="utf-8")
        if _STUB_MARKER in text:
            result.errors.append(
                f"slice {name}: docs.md is still a stub — Tier-2 guides are "
                f"mandatory (#325)"
            )
        anchors = _ANCHOR_PATTERN.findall(text)
        for local in _MALFORMED_ANCHOR_PATTERN.findall(text):
            result.errors.append(
                f"slice {name}: docs.md anchors gmeow:{local} at the wrong "
                f"heading depth — the canonical Tier-2 anchor is "
                f"`### gmeow:Term` (#325)"
            )
        if not anchors and _STUB_MARKER not in text:
            result.warnings.append(
                f"slice {name}: docs.md has no `### gmeow:Term` anchors — "
                f"guides should be term-anchored (#325)"
            )
        for local in anchors:
            term = URIRef(NAMESPACE + local)
            if term not in declared:
                result.errors.append(
                    f"slice {name}: docs.md anchors gmeow:{local}, which is "
                    f"not a declared GMEOW term (renamed or removed? #325)"
                )
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
    result.extend(term_naming_lint(merged))
    result.extend(slice_ownership_lint())
    result.extend(guide_anchor_lint(merged))
    result.extend(reasoning_lint(merged))
    shacl_key = _merged_shacl_cache_key()
    result.extend(_cached_result("merged-shacl", shacl_key, lambda: run_shacl(merged)))
    result.extend(check_examples(merged, base_cache_key=shacl_key))
    result.extend(
        _cached_result(
            "dsl-shacl",
            _dsl_shacl_cache_key(MAPPING_DSL_DIR, "mapping"),
            lambda: _dsl_shacl(MAPPING_DSL_DIR, "mapping"),
        )
    )
    result.extend(
        _cached_result(
            "dsl-shacl",
            _dsl_shacl_cache_key(STATEMENT_DSL_DIR, "statement"),
            lambda: _dsl_shacl(STATEMENT_DSL_DIR, "statement"),
        )
    )
    return result
