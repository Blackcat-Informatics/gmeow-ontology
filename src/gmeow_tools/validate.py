"""Validation: Turtle syntax, structural lint, and SHACL conformance.

These checks run without Docker so contributors can lint locally and CI can gate
cheaply before the heavier reasoning step. The orchestration in
:func:`validate_all` is a thin Python wrapper around the Rust-native
``gmeow_validate.validate_all_native`` entrypoint (#634): the Rust engine builds
the ontology store once, parses the SHACL shapes once, and runs every phase
against the shared store. The legacy N-Triples SHACL seam survives only as a
convenience for tests and ``audit.py``.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field
from functools import lru_cache
from pathlib import Path

import gmeow_diagnostics
import gmeow_validate

from gmeow_tools import shacl_engine
from gmeow_tools.config import (
    _SAMEAS_ALLOWLIST,
    EXTERNAL_FIXTURES_DIR,
    FIXTURES_DIR,
    GENERATED_SHAPES_DIR,
    MAPPING_DSL_DIR,
    MAPPING_DSL_SHAPES_FILE,
    NAMESPACE,
    ONTOLOGY_IRI,
    PROJECT_ROOT,
    SHAPES_DIR,
    SHAPES_FILE,
    SLICES_DIR,
    STATEMENT_DSL_DIR,
    STATEMENT_DSL_SHAPES_FILE,
)
from gmeow_tools.graph import iter_source_files
from gmeow_tools.slices import (
    iter_slice_module_files,
    iter_slice_shape_files,
)

#: CamelCase tokens that mark a selector privileging one co-equal claim (P9).
_SELECTOR_TOKENS = frozenset({"primary", "preferred", "default", "main"})


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


@lru_cache(maxsize=2)
def _dsl_shapes_turtle(shapes_path: Path) -> str:
    """Read (and cache) a DSL-specific SHACL shapes file as Turtle text.

    The mapping and statement DSL shapes are authored as dedicated files in
    ``shapes/`` and parsed directly by the Rust SHACL engine during DSL
    validation (#634).
    """
    return shapes_path.read_text(encoding="utf-8")


@dataclass(slots=True)
class ValidationResult:
    """Outcome of a validation pass."""

    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)
    timings: list[dict[str, object]] = field(default_factory=list)
    #: The single canonical diagnostics report serialized to JSON (#654). The
    #: Rust orchestration always supplies it; ``errors``/``warnings`` are its
    #: legacy projection. ``None`` only for hand-built results.
    report_json: str | None = None

    @property
    def ok(self) -> bool:
        """Return whether validation passed (no errors)."""
        return not self.errors

    def extend(self, other: ValidationResult) -> None:
        """Merge another result into this one."""
        self.errors.extend(other.errors)
        self.warnings.extend(other.warnings)
        self.timings.extend(other.timings)


#: Content-addressed cache root used by the Rust validation orchestration.
_VALIDATION_CACHE_DIR = PROJECT_ROOT / ".cache" / "validate"


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
    import tempfile
    from contextlib import suppress

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


def check_syntax() -> ValidationResult:
    """Parse every source Turtle file individually to catch syntax errors.

    Runs through the Rust ``gmeow_validate`` extension (#579): the per-file
    oxigraph parse and the ``"syntax error in {path}: {exc}"`` framing live in
    Rust now, with no Python fallback (the extension is a hard
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
    exact error framing live in Rust. There is no Python fallback —
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


def term_naming_lint(source_paths: list[str]) -> ValidationResult:
    """Principle 9 by annotation (#281): no selector names on ontology terms.

    Extends :func:`gmeow_tools.statement_lint.no_preferred_rank` from
    statement-annotation properties to every GMEOW term local name: a
    camelCase token of ``primary``/``preferred``/``default``/``main`` marks a
    selector that would privilege one co-equal claim over another. Legitimate
    value-vocabulary names (``scriptRolePrimary``, ``sourceTierPrimary``)
    carry an explicit ``gmeow:namingNote`` justification — the lint enforces
    the judgment instead of relying on audit-time discretion.

    Args:
        source_paths: The Turtle source file paths to lint (the validation path
            passes ``iter_source_files()``; tests serialize a synthetic graph to
            a temp N-Triples file and pass its path — graph-free here, #579).

    Routed through the Rust ``gmeow_validate.term_naming_lint`` engine (#579):
    the CamelCase split, selector-token match, and ``gmeow:namingNote`` escape
    hatch all live in Rust over an oxigraph store built from the source paths.
    Python supplies only the typed config.
    """
    report = gmeow_validate.term_naming_lint(source_paths, _lint_config())
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


def structural_lint(source_paths: list[str]) -> ValidationResult:
    """Check that every GMEOW term is fully annotated.

    Every GMEOW-namespaced ontology header, class, property, annotation
    property, datatype, and individual must carry ``rdfs:label``,
    ``skos:definition``, and ``rdfs:isDefinedBy`` (all errors as of issue #221).
    Dangling ``rdfs:subClassOf`` / ``rdfs:subPropertyOf`` targets are reported as
    errors, and a comprehensiveness heuristic warns when a parent class has
    multiple undocumented direct subclasses.

    Args:
        source_paths: The Turtle source file paths to lint (the validation path
            passes ``iter_source_files()``; tests serialize a synthetic graph to
            a temp N-Triples file and pass its path — graph-free here, #579).

    Returns:
        The validation result.

    Routed through the Rust ``gmeow_validate.structural_lint`` engine (#579):
    the per-term annotation checks, Tier-1 depth grading, consumer-context
    check, dangling-target check, comprehensiveness heuristic, and the two
    language-tag policy checks (Check 1 / Check 2, with the legacy ``Literal``
    repr framing reproduced byte-exact) all live in Rust over an oxigraph
    store built from the source paths. Python supplies only the typed config
    (namespace, core-slice IRIs, annotation predicates).
    """
    report = gmeow_validate.structural_lint(source_paths, _lint_config())
    return ValidationResult(
        errors=list(report["errors"]),
        warnings=list(report["warnings"]),
    )


def reasoning_lint(source_paths: list[str]) -> ValidationResult:
    """Wrap the UFO anti-pattern checks as a :class:`ValidationResult`.

    Args:
        source_paths: The Turtle source file paths to check (the validation path
            passes ``iter_source_files()`` — graph-free here, #579).

    Routed through the Rust ``gmeow_validate.reasoning_invariants`` engine
    (#579): the six OntoUML anti-pattern checks run over an oxigraph store built
    from the source paths. Each violation (missing/conflicting gUFO stereotype,
    identity conflict, anti-rigidity breach, under-mediated relator, co-equal
    facet collapse, missing frame declaration) becomes an error so ``make
    validate`` fails if the meta-grounding is incomplete.
    """
    report = gmeow_validate.reasoning_invariants(source_paths, str(NAMESPACE))
    return ValidationResult(
        errors=list(report["errors"]),
        warnings=list(report["warnings"]),
    )


def run_shacl(data_nt: str, *, shapes_path: Path = SHAPES_FILE) -> ValidationResult:
    """Test/audit helper: validate an N-Triples data graph against the SHACL shapes.

    The production ``make validate`` path no longer serializes the merged
    ontology to N-Triples; it validates the shared oxigraph store directly in
    Rust (#634). This function remains as a convenience for the test suite and
    for ``audit.py``, which still build small rdflib graphs and serialize them
    to N-Triples.

    Args:
        data_nt: The data graph to validate, serialized as N-Triples.
        shapes_path: Path to the SHACL shapes Turtle file.

    Returns:
        The validation result, bucketed by SHACL severity: ``sh:Violation``
        results become errors, while ``sh:Warning`` / ``sh:Info`` results become
        warnings. A warning-only graph therefore still passes (``result.ok`` is
        ``True``).

    Raises:
        FileNotFoundError: If the shapes file is missing.
        ValueError: If ``gmeow_shacl`` cannot parse the data or shapes — a hard
            failure that must surface, never a silent ``conforms`` (P11/§11).
    """
    if not shapes_path.exists():
        raise FileNotFoundError(f"SHACL shapes not found: {shapes_path}")

    shapes_ttl = _shapes_turtle(shapes_path)
    report = shacl_engine.validate_nt(data_nt, shapes_ttl)
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


_ANCHOR_PATTERN = re.compile(r"^###\s+`?gmeow:([A-Za-z][A-Za-z0-9]*)`?", re.MULTILINE)
# Term headings at the wrong depth are malformed anchors, not invisible ones —
# the canonical Tier-2 anchor shape is exactly `### gmeow:Term`.
_MALFORMED_ANCHOR_PATTERN = re.compile(
    r"^(?:##|#{4,})\s+`?gmeow:([A-Za-z][A-Za-z0-9]*)`?", re.MULTILINE
)
_STUB_MARKER = "This is a STUB guide"


def guide_anchor_lint(
    source_paths: list[str],
    root: Path | None = None,
    *,
    declared_terms: list[str] | None = None,
) -> ValidationResult:
    """Tier-2 structural binding (#325): guides are bound to the graph.

    Every slice must carry a non-stub ``docs.md`` whose ``### gmeow:X``
    heading anchors resolve to declared GMEOW terms — a renamed term breaks
    the build, and a slice without a guide fails. Anchors owned by another
    slice are legal cross-references; anchors matching no term are errors.

    Args:
        source_paths: The Turtle source file paths whose declared GMEOW terms
            the anchors resolve against (the validation path passes
            ``iter_source_files()`` — graph-free here, #579).
        root: Override the slice-discovery root (tests).
        declared_terms: Optional pre-computed declared GMEOW-term IRIs. When
            provided, the expensive Rust declared-term scan is skipped (#634).

    The declared-term set is computed by the Rust
    ``gmeow_validate.declared_terms`` engine (#579) over the source paths; the
    markdown anchor parsing and resolution stay in Python (plain string IRIs).
    """
    from gmeow_tools.config import SLICES_DIR

    result = ValidationResult()
    if declared_terms is not None:
        declared = set(declared_terms)
    else:
        declared = set(gmeow_validate.declared_terms(source_paths, _lint_config()))
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
            term = NAMESPACE + local
            if term not in declared:
                result.errors.append(
                    f"slice {name}: docs.md anchors gmeow:{local}, which is "
                    f"not a declared GMEOW term (renamed or removed? #325)"
                )
    return result


def validate_all(
    timings: bool = False,
    gts_input: Path | None = None,
    signature_config: dict[str, object] | None = None,
) -> ValidationResult:
    """Run syntax, structural lint, SHACL, and sameAs-ban checks.

    This is now a thin Python compatibility wrapper around the Rust-native
    ``gmeow_validate.validate_all_native`` orchestration (#634). The Rust
    engine builds the ontology store once, parses the SHACL shapes once, and
    runs every phase against the shared store. Python keeps the guide-anchor
    lint (it needs the slice filesystem) and translates the returned dict into
    the existing :class:`ValidationResult` model.

    When ``gts_input`` is provided, the engine validates the folded GTS bundle
    directly. If ``signature_config`` is also provided, a signature/trust
    verification pre-gate runs before the ontology phases (#646): it checks
    embedded GTS signatures against the configured trusted signers, signature
    requirements, and optional out-of-band armored public key, and aborts with
    hard failures when the policy is not satisfied.

    Args:
        timings: When ``True``, ask the Rust engine to record per-phase wall
            timings and surface them in :attr:`ValidationResult.timings` for
            the CLI to report.
        gts_input: When provided, validate the folded GTS bundle at this path
            directly instead of the repo Turtle sources (#644). The Rust engine
            builds the shared store from the bundle and skips the per-file
            Turtle phases (syntax + ``owl:sameAs`` ban) that don't apply to an
            already-folded graph; every store-based phase runs unchanged. The
            Python-side per-file lints (guide-anchor, i18n PO) are likewise
            skipped to mirror the Rust GTS phase set — ``--gts`` differs only in
            input provenance, never in validation semantics.
        signature_config: Optional signature/trust policy configuration for the
            GTS verification pre-gate (#646). Keys: ``trusted_signers`` (list of
            strings), ``require_signatures`` (bool), ``require_trusted_signer``
            (bool), and ``trusted_key`` (optional armored public key content).
            When omitted, signature verification is disabled.
    """
    # Ensure the content-addressed cache root exists before Rust needs it.
    _VALIDATION_CACHE_DIR.mkdir(parents=True, exist_ok=True)

    if gts_input is not None:
        try:
            gts_bytes: bytes | None = gts_input.read_bytes()
        except OSError as exc:
            # A missing/unreadable --gts path is a usage error, not a crash:
            # surface it as a normal validation failure instead of a traceback.
            return ValidationResult(
                errors=[f"failed to read GTS bundle '{gts_input}': {exc}"]
            )
    else:
        gts_bytes = None
    # In GTS mode the store is built from the bundle, so there are no per-file
    # source paths to attribute Turtle syntax/sameAs errors to.
    source_paths = (
        [] if gts_input is not None else [str(p) for p in iter_source_files()]
    )
    shapes_ttl = _shapes_turtle(SHAPES_FILE)

    # GTS mode validates a folded graph, so only the store-based phases apply
    # (structural lint, term-naming, reasoning/gUFO invariants, merged SHACL).
    # The source-layout phases — slice-ownership, example coverage, per-example
    # SHACL, and the mapping/statement DSL SHACL — validate the repo source tree,
    # not a bundle, so we withhold their filesystem inputs; the Rust engine skips
    # any phase whose inputs are absent (#644).
    if gts_input is not None:
        module_specs: list[tuple[str, str]] = []
        slices_dir_opt: str | None = None
        mapping_shapes_ttl: str | None = None
        statement_shapes_ttl: str | None = None
        mapping_dsl_dir = ""
        statement_dsl_dir = ""
    else:
        mapping_shapes_ttl = _dsl_shapes_turtle(MAPPING_DSL_SHAPES_FILE)
        statement_shapes_ttl = _dsl_shapes_turtle(STATEMENT_DSL_SHAPES_FILE)
        module_specs = [
            (str(module), f"{NAMESPACE}slices/{module.parent.name}")
            for module in iter_slice_module_files()
        ]
        slices_dir_opt = str(SLICES_DIR)
        mapping_dsl_dir = str(MAPPING_DSL_DIR)
        statement_dsl_dir = str(STATEMENT_DSL_DIR)

    config = _lint_config()
    if signature_config is not None:
        raw_signers = signature_config.get("trusted_signers", [])
        trusted_signers = (
            [str(s) for s in raw_signers] if isinstance(raw_signers, list) else []
        )
        raw_key = signature_config.get("trusted_key")
        trusted_key = raw_key if isinstance(raw_key, str) else None
        signature_options = gmeow_validate.SignatureConfig(
            trusted_signers=trusted_signers,
            require_signatures=bool(signature_config.get("require_signatures", False)),
            require_trusted_signer=bool(
                signature_config.get("require_trusted_signer", False)
            ),
            trusted_key=trusted_key,
        )
    else:
        signature_options = None
    options = gmeow_validate.ValidateOptions(
        timings=timings,
        sameas_allowlist=[(subject, obj) for subject, obj in _SAMEAS_ALLOWLIST],
        module_specs=module_specs,
        slices_dir=slices_dir_opt,
        mapping_shapes_ttl=mapping_shapes_ttl,
        statement_shapes_ttl=statement_shapes_ttl,
        project_root=str(PROJECT_ROOT),
        gts_bytes=gts_bytes,
        signature_config=signature_options,
    )

    report = gmeow_validate.validate_all_native(
        source_paths,
        shapes_ttl,
        mapping_dsl_dir,
        statement_dsl_dir,
        config,
        options,
    )

    result = ValidationResult(
        errors=list(report["errors"]),
        warnings=list(report["warnings"]),
        timings=list(report.get("timings", [])),
        report_json=report.get("report_json"),
    )

    # The Python-side per-file lints below validate repo source files (slice
    # docs.md, authored PO files), not the folded GTS graph. In GTS mode there
    # are no per-file source paths, so we skip them to mirror the Rust engine's
    # GTS phase set (which skips its own per-file Turtle phases) — #644. Their
    # findings are tracked separately so they can be folded back into the single
    # canonical report alongside the Rust findings (#654).
    py_errors: list[str] = []
    py_warnings: list[str] = []
    if gts_input is None:
        # Guide-anchor lint stays Python-side: it resolves markdown anchors in
        # slice docs.md against the declared GMEOW terms returned by Rust. Skip
        # it when earlier phases already failed (mirrors the original
        # orchestration).
        if result.ok:
            declared_terms = list(report.get("declared_terms", []))
            anchor = guide_anchor_lint(source_paths, declared_terms=declared_terms)
            result.extend(anchor)
            py_errors.extend(anchor.errors)
            py_warnings.extend(anchor.warnings)

        # PO i18n lint: structural validity, orphaned/stale entries, and fuzzy
        # ratio gates (#572).  Kept Python-side because it reads authored PO
        # files and the merged rdflib English graph.
        try:
            from gmeow_tools.i18n_lint import lint_po_files

            i18n_report = lint_po_files(PROJECT_ROOT)
            result.warnings.extend(i18n_report.warnings)
            result.errors.extend(i18n_report.errors)
            py_errors.extend(i18n_report.errors)
            py_warnings.extend(i18n_report.warnings)
        except Exception as exc:  # pragma: no cover - guard against unknown failures
            message = f"i18n PO lint failed: {exc}"
            result.errors.append(message)
            py_errors.append(message)

    # Fold the Python-only lint findings into the single canonical report so
    # report_json stays the complete source of truth (#654).
    if result.report_json is not None and (py_errors or py_warnings):
        merged = gmeow_diagnostics.Report.from_json(result.report_json)
        merged.extend(gmeow_diagnostics.from_legacy("validate", py_errors, py_warnings))
        result.report_json = merged.to_json()

    return result
