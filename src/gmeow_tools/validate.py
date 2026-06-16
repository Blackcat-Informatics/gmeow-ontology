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

import gmeow_validate
from rdflib import Graph, URIRef
from rdflib.namespace import SKOS
from rdflib.term import Node

from gmeow_tools import shacl_engine
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

#: CamelCase tokens that mark a selector privileging one co-equal claim (P9).
_SELECTOR_TOKENS = frozenset({"primary", "preferred", "default", "main"})

_CAMEL_SPLIT = re.compile(r"[A-Z]?[a-z0-9]+|[A-Z]+(?![a-z])")

_TERM_KIND_ORDER = (
    "ontology",
    "class",
    "property",
    "annotation property",
    "datatype",
    "individual",
)


def _lint_config() -> gmeow_validate.LintConfig:
    """Build the typed Rust lint config from the Python single-source constants.

    Carries ``config.NAMESPACE``/``config.ONTOLOGY_IRI``, the P9 selector
    tokens, the core-slice IRIs (the Tier-1 grading set), and the standard
    annotation predicates the Check-2 language-tag policy polices (#579). The
    Rust engine owns the lint logic; Python owns the registry and constants.
    """
    from gmeow_tools.language_tags import _ANNOTATION_PREDICATES

    core_slice_iris = [
        s.iri  # type: ignore[attr-defined]
        for s in _discover_slices_cached().values()
        if s.tier == "core"  # type: ignore[attr-defined]
    ]
    return gmeow_validate.LintConfig(
        str(NAMESPACE),
        str(ONTOLOGY_IRI),
        sorted(_SELECTOR_TOKENS),
        core_slice_iris,
        sorted(str(p) for p in _ANNOTATION_PREDICATES),
    )


def _graph_source_paths(graph: Graph) -> tuple[list[str], Callable[[], None]]:
    """Serialize *graph* to a temporary N-Triples file for the Rust lints.

    The Rust lints build their own oxigraph store from file paths, so an
    in-memory rdflib graph (the merged ontology, or a test's hand-built graph)
    is written to one N-Triples temp file. Returns the source-path list plus a
    cleanup callback the caller invokes when done.

    N-Triples is chosen so any graph round-trips losslessly through oxigraph's
    Turtle-family parser without prefix bookkeeping.
    """
    with tempfile.NamedTemporaryFile(
        "wb", suffix=".nt", prefix="gmeow-lint-", delete=False
    ) as handle:
        graph.serialize(destination=handle, format="nt", encoding="utf-8")
        path = Path(handle.name)

    def _cleanup() -> None:
        with suppress(OSError):
            path.unlink(missing_ok=True)

    return [str(path)], _cleanup


@lru_cache(maxsize=4)
def _shapes_turtle(shapes_path: Path) -> str:
    """Merge (and cache) the SHACL shapes into one Turtle document for the engine.

    ``run_shacl`` runs ~150 times across the test suite; re-reading the shapes
    each call wastes time, so one shared cached merge per path is kept. The
    shapes are concatenated as raw Turtle text (preserving every ``@prefix``
    header the SHACL-AF ``sh:select`` queries resolve against) rather than parsed
    into a graph — ``gmeow_shacl`` ingests Turtle directly (#578).

    Loads the requested base shapes file plus every other modular ``*.ttl``
    shape file in ``shapes/`` except the DSL-specific lints, so new domain
    shape files (e.g. ``expertise-shapes.ttl``) are picked up automatically.
    """
    dsl_shapes = {
        "mapping-dsl-shapes.ttl",
        "statement-dsl-shapes.ttl",
        "slice-manifest-shapes.ttl",  # targets manifests, not the data graph
        shapes_path.name,
    }
    files: list[Path] = [shapes_path]
    files += [
        extra
        for extra in sorted(SHAPES_DIR.glob("*.ttl"))
        if extra.name not in dsl_shapes
    ]
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
    files += generated_shapes
    files += iter_slice_shape_files()
    return shacl_engine.shapes_files_to_turtle(files)


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
    """Return a cache salt for the SHACL validation toolchain versions.

    The production validation path is ``gmeow_shacl`` (the Rust validator) over
    pyoxigraph-ingested data (#578). pyshacl/rdflib no longer gate the SHACL
    result, so they are not part of the salt — one cache-invalidation regime.
    """
    parts: list[str] = []
    for package in ("gmeow-shacl", "pyoxigraph"):
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


def check_syntax() -> ValidationResult:
    """Parse every source Turtle file individually to catch syntax errors.

    Runs through the Rust ``gmeow_validate`` extension (#579): the per-file
    oxigraph parse and the ``"syntax error in {path}: {exc}"`` framing live in
    Rust now, with no Python pyoxigraph fallback (the extension is a hard
    dependency of the validation path).
    """
    report = gmeow_validate.check_syntax([str(p) for p in iter_source_files()])
    return ValidationResult(
        errors=list(report["errors"]),
        warnings=list(report["warnings"]),
    )


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

    The scan runs through the Rust ``gmeow_validate`` extension (#579): the
    per-quad ``owl:sameAs`` check, the namespace/allowlist filtering, and the
    exact error framing live in Rust. There is no Python pyoxigraph fallback —
    the extension is a hard dependency of the validation path.
    """
    if paths is None:
        paths = [
            *iter_source_files(),
            *_authored_fixtures(),
        ]
    if not paths:
        raise ValueError("check_sameas_ban: paths to audit must not be empty")
    report = gmeow_validate.check_sameas_ban(
        [str(p) for p in paths],
        str(NAMESPACE),
        [(subject, obj) for subject, obj in _SAMEAS_ALLOWLIST],
    )
    return ValidationResult(
        errors=list(report["errors"]),
        warnings=list(report["warnings"]),
    )


def _is_gmeow_term(term: URIRef) -> bool:
    """Return whether an IRI is the GMEOW root or lives in its namespace."""
    s = str(term)
    return s.startswith(NAMESPACE) or s == ONTOLOGY_IRI


def _term_kind(graph: Graph, term: URIRef) -> str:
    """Return the primary structural kind of a GMEOW term based on its rdf:type.

    Routed through the Rust ``gmeow_validate`` engine (#579): the per-term
    kind resolution (``_TERM_KIND_ORDER`` priority) lives in Rust. This thin
    wrapper looks the term up in the typed-terms map; a term with no explicit
    ``rdf:type`` (never in the map) is an ``"individual"`` by the same
    convention the Rust collector uses.
    """
    return _collect_typed_terms(graph).get(term, "individual")


def _collect_typed_terms(graph: Graph) -> dict[URIRef, str]:
    """Map every GMEOW-namespaced term with an rdf:type to its primary kind.

    Routed through the Rust ``gmeow_validate.typed_terms`` engine (#579): the
    type-query collection and multi-typed resolution live in Rust over an
    oxigraph store. The graph is serialized to a temp N-Triples file and the
    returned ``[(iri, kind)]`` pairs are rehydrated into rdflib ``URIRef``
    keys for the Python callers that still expect them.
    """
    source_paths, cleanup = _graph_source_paths(graph)
    try:
        pairs = gmeow_validate.typed_terms(source_paths, _lint_config())
    finally:
        cleanup()
    return {URIRef(iri): kind for iri, kind in pairs}


def term_naming_lint(graph: Graph) -> ValidationResult:
    """Principle 9 by annotation (#281): no selector names on ontology terms.

    Extends :func:`gmeow_tools.statement_lint.no_preferred_rank` from
    statement-annotation properties to every GMEOW term local name: a
    camelCase token of ``primary``/``preferred``/``default``/``main`` marks a
    selector that would privilege one co-equal claim over another. Legitimate
    value-vocabulary names (``scriptRolePrimary``, ``sourceTierPrimary``)
    carry an explicit ``gmeow:namingNote`` justification — the lint enforces
    the judgment instead of relying on audit-time discretion.

    Routed through the Rust ``gmeow_validate.term_naming_lint`` engine (#579):
    the CamelCase split, selector-token match, and ``gmeow:namingNote`` escape
    hatch all live in Rust over an oxigraph store built from the serialized
    graph. Python supplies only the typed config.
    """
    source_paths, cleanup = _graph_source_paths(graph)
    try:
        report = gmeow_validate.term_naming_lint(source_paths, _lint_config())
    finally:
        cleanup()
    return ValidationResult(
        errors=list(report["errors"]),
        warnings=list(report["warnings"]),
    )


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

    Routed through the Rust ``gmeow_validate.structural_lint`` engine (#579):
    the per-term annotation checks, Tier-1 depth grading, consumer-context
    check, dangling-target check, comprehensiveness heuristic, and the two
    language-tag policy checks (Check 1 / Check 2, with the rdflib ``Literal``
    repr framing reproduced byte-exact) all live in Rust over an oxigraph
    store built from the serialized graph. Python supplies only the typed
    config (namespace, core-slice IRIs, annotation predicates).
    """
    source_paths, cleanup = _graph_source_paths(graph)
    try:
        report = gmeow_validate.structural_lint(source_paths, _lint_config())
    finally:
        cleanup()
    return ValidationResult(
        errors=list(report["errors"]),
        warnings=list(report["warnings"]),
    )


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
        ValueError: If ``gmeow_shacl`` cannot parse the data or shapes — a hard
            failure that must surface, never a silent ``conforms`` (P11/§11).
    """
    if not shapes_path.exists():
        raise FileNotFoundError(f"SHACL shapes not found: {shapes_path}")

    shapes_ttl = _shapes_turtle(shapes_path)
    report = shacl_engine.validate_graph(data_graph, shapes_ttl)
    result = ValidationResult()
    if report["conforms"]:
        return result

    violations, warnings = shacl_engine.partition_results(report["results"])
    if violations:
        result.errors.append("SHACL violations:\n" + "\n".join(violations))
    if warnings:
        result.warnings.append("SHACL warnings:\n" + "\n".join(warnings))
    # Defensive: a non-conforming report with no parseable results must still
    # surface (gmeow_shacl reports conforms == results-empty, so this is unreachable
    # in practice, but a silent pass on non-conformance is the worst outcome).
    if not violations and not warnings:
        result.errors.append("SHACL validation failed: non-conforming with no results")
    return result


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

    Routed through the Rust ``gmeow_validate.slice_ownership_lint`` engine
    (#579): each module is parsed ALONE in Rust and its GMEOW subjects'
    ``rdfs:isDefinedBy`` objects are equality-checked against the owning slice
    IRI. Python supplies the ``(module_path, expected_slice_iri)`` registry —
    the slice-IRI derivation (``NAMESPACE + slices/<dir>``) stays here so the
    file→slice convention is owned in one place.
    """
    modules = iter_slice_module_files(root) if root else iter_slice_module_files()
    module_specs = [
        (str(module), f"{NAMESPACE}slices/{module.parent.name}") for module in modules
    ]
    report = gmeow_validate.slice_ownership_lint(module_specs, _lint_config())
    return ValidationResult(
        errors=list(report["errors"]),
        warnings=list(report["warnings"]),
    )


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
        # The SHACL outcome now flows through the gmeow_shacl seam — editing it
        # (serialization, partitioning) must invalidate the cache (#578).
        Path(__file__).parent / "shacl_engine.py",
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
    shacl_engine_source = PROJECT_ROOT / "src" / "gmeow_tools" / "shacl_engine.py"
    return _cache_key(
        [
            _files_cache_key(
                [
                    Path(__file__),
                    dsl_validate_source,
                    shacl_engine_source,  # DSL path also routes through the seam (#578)
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

    The declared-term set is computed by the Rust
    ``gmeow_validate.declared_terms`` engine (#579) over the serialized graph;
    the markdown anchor parsing and resolution stay in Python.
    """
    from gmeow_tools.config import SLICES_DIR

    result = ValidationResult()
    source_paths, cleanup = _graph_source_paths(graph)
    try:
        declared = {
            URIRef(iri)
            for iri in gmeow_validate.declared_terms(source_paths, _lint_config())
        }
    finally:
        cleanup()
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
