"""Repository-maintenance command-line entry point for GMEOW developers.

The CLI is a thin orchestration layer: every subcommand delegates to a focused
module (``validate``, ``reason``, ``mappings`` …) so the command surface stays
declarative and the logic stays unit-testable. The Makefile shells into these
subcommands rather than reimplementing any behaviour.
"""

from __future__ import annotations

import os
import tomllib
from collections.abc import Callable
from pathlib import Path
from typing import TYPE_CHECKING, Any, cast

import gmeow_slice
import gmeow_validate
import gts
import httpx
import typer
from rich.console import Console
from rich.markup import escape

from gmeow_tools import __version__
from gmeow_tools.config import MAPPINGS_DIR, PROJECT_ROOT
from gmeow_tools.projections import PROFILES as _PROFILES

if TYPE_CHECKING:
    from gmeow_rdf.compat.rdflib import Graph
    from gmeow_slice import ProjectionDiagnostic

    from gmeow_tools.diagnostics import DiagnosticsReport
    from gmeow_tools.language_tags import LangSelector


def _alignment_checks() -> frozenset[str]:
    return frozenset(gmeow_slice.alignment_policy()["alignment_checks"])


def _alignment_findings(*, network: bool = False) -> list[ProjectionDiagnostic]:
    """Run the Rust alignment lint and return only alignment-family findings."""
    checks = _alignment_checks()
    return [
        finding
        for finding in gmeow_slice.lint_projection(
            str(PROJECT_ROOT), allow_network=network
        )
        if finding["check"] in checks
    ]


app = typer.Typer(
    name="gmeow-dev",
    help="Build, validate, reason over, and publish the GMEOW ontology checkout.",
    no_args_is_help=True,
    add_completion=False,
)
console = Console()
err_console = Console(stderr=True)


def _fail(message: str, code: int = 1) -> typer.Exit:
    """Print an error and return an Exit to raise."""
    err_console.print(f"[red]{message}[/red]")
    return typer.Exit(code=code)


def _run_pipeline(jobs: int | None = None, check: bool = False) -> dict[str, Any]:
    """Run the Rust single-pass build executor and return its summary report.

    Calls ``gmeow_native.pipeline.run_pipeline(root, jobs, check)`` — the #861
    DAG executor that reads the dogfooded build graph (``slices/core/pipeline/``)
    and reproduces every committed artifact single-pass. This is THE build
    authority since #861 P7 retired the Python generator orchestrator.

    Raises a clear ``typer.Exit`` if the native extension is not importable (the
    ``gmeow_native`` cdylib must be rebuilt with the pipeline submodule).
    """
    from gmeow_tools.config import PROJECT_ROOT

    try:
        import gmeow_native.pipeline as _pipeline
    except ImportError as exc:
        raise _fail(
            "✗ the native pipeline is unavailable: "
            f"`import gmeow_native.pipeline` failed ({exc}). Rebuild the unified "
            "extension (e.g. `maturin develop --manifest-path "
            "crates/native/Cargo.toml`) to pick up the pipeline submodule."
        ) from exc

    cpu = jobs if jobs is not None else (os.cpu_count() or 1)
    report = _pipeline.run_pipeline(str(PROJECT_ROOT), int(cpu), check)
    return cast("dict[str, Any]", report)


def _compile_statements_native() -> dict[str, str]:
    """Compile statement artifacts through the native Rust statements stage."""
    try:
        import gmeow_native.pipeline as _pipeline
    except ImportError as exc:
        raise _fail(
            "✗ the native pipeline is unavailable: "
            f"`import gmeow_native.pipeline` failed ({exc}). Rebuild the unified "
            "extension (e.g. `maturin develop --manifest-path "
            "crates/native/Cargo.toml`) to pick up the pipeline submodule."
        ) from exc
    artifacts = _pipeline.compile_statements(str(PROJECT_ROOT))
    return cast("dict[str, str]", artifacts)


def _statement_compile_report() -> Any:
    """Native statement compiler diagnostics folded into feedback (#935).

    Python owns only the developer feedback surface here. The compiler itself is
    the Rust `stage-statements` implementation exposed through
    `gmeow_native.pipeline.compile_statements_report`.
    """
    from gmeow_tools.graph import load_merged_graph

    try:
        import gmeow_native.pipeline as _pipeline
    except ImportError as exc:
        raise _fail(
            "✗ the native pipeline is unavailable: "
            f"`import gmeow_native.pipeline` failed ({exc}). Rebuild the unified "
            "extension (e.g. `maturin develop --manifest-path "
            "crates/native/Cargo.toml`) to pick up the pipeline submodule."
        ) from exc
    onto = load_merged_graph(include_imports=False)
    return _pipeline.compile_statements_report(
        str(PROJECT_ROOT),
        onto.serialize(format="nt"),
    )


def _mapping_compile_report() -> Any:
    """Native mapping compiler diagnostics folded into feedback (#934)."""
    try:
        import gmeow_native.pipeline as _pipeline
    except ImportError as exc:
        raise _fail(
            "✗ the native pipeline is unavailable: "
            f"`import gmeow_native.pipeline` failed ({exc}). Rebuild the unified "
            "extension (e.g. `maturin develop --manifest-path "
            "crates/native/Cargo.toml`) to pick up the pipeline submodule."
        ) from exc
    return _pipeline.compile_mappings_report(str(PROJECT_ROOT))


def _regenerate_native(jobs: int | None = None, check: bool = False) -> None:
    """Build (or drift-check) every committed artifact via the Rust pipeline."""
    report = _run_pipeline(jobs=jobs, check=check)

    for finding in report.get("findings", []):
        err_console.print(
            f"[yellow]{finding['severity']}[/yellow] {finding['code']}: "
            f"{finding['message']}"
        )

    if check:
        drifted = report.get("drifted", [])
        if drifted:
            for path in drifted:
                err_console.print(f"[red]drift[/red] {path}")
            raise _fail(f"✗ {len(drifted)} artifact(s) drifted")
        console.print("[green]✓ pipeline check: zero drift[/green]")
    else:
        console.print(
            f"[green]✓ pipeline regenerate: produced {report['produced']}, "
            f"reproduced {report['reproduced']}[/green]"
        )


def _lang_option() -> Any:
    """Shared --lang / -l option for language-emitting commands."""
    return typer.Option(
        None,
        "--lang",
        "-l",
        help=(
            "Language(s) for emitted labels and definitions: a BCP-47 tag "
            "(en, zh, fr) or an internal tag (x-gmeow-english). Comma-separated "
            "for multiple languages. Overrides GMEOW_LANG. An empty value "
            "(--lang '') selects the default English carrier."
        ),
    )


def _gts_tag_map(path: Path | None = None) -> dict[str, str]:
    """Return the tag map for a .gts, falling back to the repo graph if missing."""
    from gmeow_tools.gts_views import load_fold

    try:
        return load_fold(path).tag_map()
    except FileNotFoundError:
        return _repo_tag_map()


def _repo_tag_map() -> dict[str, str]:
    """Return the tag map from the active repository ontology graph."""
    from gmeow_tools.graph import load_merged_graph
    from gmeow_tools.language_tags import load_tag_map

    return load_tag_map(load_merged_graph(include_imports=False))


def _resolve_lang(lang: str | None, tag_map: dict[str, str]) -> LangSelector:
    """Resolve CLI/env input against the supplied tag map."""
    from gmeow_tools.language_tags import UnknownLanguageError, resolve_lang_input

    try:
        return resolve_lang_input(
            lang if lang is not None else os.environ.get("GMEOW_LANG"),
            tag_map,
        )
    except UnknownLanguageError as exc:
        raise _fail(str(exc)) from exc


def _read_turtle(source: Path) -> tuple[Graph, str]:
    """Parse Turtle from a file, or from stdin when ``source`` is ``-``.

    Returns ``(graph, stem)`` — the stem is the basename for the file case and
    ``"stdin"`` for the pipe, so the tools compose: ``… | gmeow transpile -``.
    """
    import sys

    from gmeow_rdf.compat.rdflib import Graph

    graph = Graph()
    stdin = str(source) == "-"
    try:
        if stdin:
            graph.parse(data=sys.stdin.read(), format="turtle")
        else:
            graph.parse(source, format="turtle")
    except (OSError, ValueError, SyntaxError) as exc:
        where = "stdin" if stdin else source
        raise _fail(f"cannot read or parse {where}: {exc}") from exc
    return (graph, "stdin") if stdin else (graph, source.stem)


_REASONED_INPUT_OPTION = typer.Option(
    None,
    "--reasoned-input",
    help="Pre-computed reasoned ontology to query (skips a second reasoning pass).",
)


def _read_gts_or_fail(path: Path) -> gts.Graph:
    """Read a GTS file, converting I/O and parse errors into a CLI failure."""
    try:
        return gts.read(path.read_bytes())
    except OSError as exc:
        raise _fail(f"cannot read {path}: {exc}") from exc
    except Exception as exc:
        raise _fail(f"cannot parse GTS file {path}: {exc}") from exc


@app.callback()
def main() -> None:
    """GMEOW repository toolchain (see subcommands)."""


@app.command()
def version() -> None:
    """Print the gmeow_tools package version."""
    console.print(__version__)


@app.command()
def info() -> None:
    """Show a summary of the bundled GMEOW ontology snapshot."""
    from gmeow_tools.config import GTS_SNAPSHOT_FILE

    path = GTS_SNAPSHOT_FILE
    graph = _read_gts_or_fail(path)
    console.print(
        f"[bold]{path.name}[/bold]: {len(graph.terms)} terms, "
        f"{len(graph.quads)} quads, {len(graph.reifiers)} reifiers, "
        f"{len(graph.annotations)} annotations, {len(graph.blobs)} docs blobs, "
        f"{len(graph.opaque)} opaque"
    )
    for diag in graph.diagnostics:
        err_console.print(f"[yellow]{diag.code}[/yellow]: {diag.detail}")


@app.command()
def regenerate(
    jobs: int | None = typer.Option(
        None,
        "-j",
        "--jobs",
        help="Per-level parallelism budget (default: capped CPU count).",
    ),
    check: bool = typer.Option(
        False,
        "--check",
        help="Drift-check committed artifacts without writing (non-zero on drift).",
    ),
) -> None:
    """Rebuild all checked-in generated artifacts from canonical sources.

    Runs the dogfooded build DAG (``slices/core/pipeline/``) single-pass through
    the Rust ``gmeow-pipeline`` executor — THE build authority since #861 P7
    retired the Python generator orchestrator. With ``--check`` it compares every
    produced artifact against the committed bytes and exits non-zero on drift.
    """
    _regenerate_native(jobs=jobs, check=check)


def _parse_evidence_spec(spec: str) -> tuple[bytes, str, str, str, str]:
    """Parse one ``path:media_type:attestation_type:rep:label`` evidence spec.

    Reads the artifact file (HARD-fails if missing/unreadable — no silent skip,
    per §18 no-optionality). The label may itself contain ``:`` (only the first
    four separators are split). Returns the row the native fold consumes:
    ``(data, media_type, attestation_type_iri, rep, subject_label)``.
    """
    parts = spec.split(":", 4)
    if len(parts) != 5:
        raise _fail(
            f"✗ malformed --evidence spec {spec!r}; expected "
            "path:media_type:attestation_type:rep:label"
        )
    path_str, media_type, attestation_type, rep, label = parts
    path = Path(path_str)
    try:
        data = path.read_bytes()
    except OSError as exc:
        raise _fail(f"✗ evidence artifact {path_str!r} is unreadable: {exc}") from exc
    return (data, media_type, attestation_type, rep, label)


@app.command(name="release-bundle")
def release_bundle(
    out: Path = typer.Option(  # noqa: B008
        Path("dist/gmeow.gts"),
        "--out",
        help=(
            "Output path for the SIGNED release bundle (NEVER the committed snapshot)."
        ),
    ),
    sign_key: Path = typer.Option(  # noqa: B008
        ...,
        "--sign-key",
        help="ASCII-armored unencrypted Ed25519 OpenPGP SECRET key (SIGN_KEY).",
    ),
    public_key: Path = typer.Option(  # noqa: B008
        ...,
        "--public-key",
        help="ASCII-armored Ed25519 OpenPGP PUBLIC certificate for the transport key.",
    ),
    source: Path = typer.Option(  # noqa: B008
        Path("generated/dist/gmeow.gts"),
        "--source",
        help="The committed unsigned gmeow.gts to fold evidence into (read-only).",
    ),
    issued_at: str = typer.Option(
        ...,
        "--issued-at",
        help="INJECTED ISO-8601 release timestamp (REQUIRED for determinism, §18).",
    ),
    attester: str = typer.Option(
        "https://blackcatinformatics.ca/gmeow/agent/release-lane",
        "--attester",
        help="IRI of the release-lane software agent that vouches for the bundle.",
    ),
    release_subject: str = typer.Option(
        "https://blackcatinformatics.ca/gmeow/release/gmeow.gts",
        "--release-subject",
        help="IRI naming the signed release bundle (the attested subject).",
    ),
    evidence: list[str] = typer.Option(  # noqa: B008
        [],
        "--evidence",
        help=("Repeatable evidence spec path:media_type:attestation_type:rep:label."),
    ),
) -> None:
    """Fold check/conformance/SARIF/perf evidence into a SIGNED gmeow.gts (#673).

    Reads the committed unsigned snapshot and each evidence artifact (HARD-fails
    on any missing file), then calls the native Rust
    ``gmeow_native.pipeline.fold_release_bundle_native`` which augments the
    snapshot with a ``graph/attestations`` named graph and the evidence blobs,
    signs it Ed25519, and returns the bytes — written to ``--out``. This Python
    layer does NO fold/sign/attestation logic; it only marshals paths + bytes.
    """
    try:
        import gmeow_native.pipeline as _pipeline
    except ImportError as exc:
        raise _fail(
            "✗ the native pipeline is unavailable: "
            f"`import gmeow_native.pipeline` failed ({exc}). Rebuild the unified "
            "extension (e.g. `maturin develop --manifest-path "
            "crates/native/Cargo.toml`) to pick up the pipeline submodule."
        ) from exc

    try:
        snapshot_bytes = source.read_bytes()
    except OSError as exc:
        raise _fail(f"✗ source snapshot {source} is unreadable: {exc}") from exc
    try:
        secret_armor = sign_key.read_text(encoding="utf-8")
    except OSError as exc:
        raise _fail(f"✗ signing key {sign_key} is unreadable: {exc}") from exc
    try:
        public_armor = public_key.read_text(encoding="utf-8")
    except OSError as exc:
        raise _fail(f"✗ public key {public_key} is unreadable: {exc}") from exc

    rows = [_parse_evidence_spec(spec) for spec in evidence]

    signed = _pipeline.fold_release_bundle_native(
        snapshot_bytes,
        rows,
        attester,
        issued_at,
        release_subject,
        secret_armor,
        public_armor,
    )

    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(signed)
    console.print(
        f"[green]✓ signed release bundle: {out} "
        f"({len(rows)} evidence artifact(s), {len(signed)} bytes)[/green]"
    )


@app.command()
def check_generated(
    jobs: int | None = typer.Option(
        None,
        "-j",
        "--jobs",
        help="Per-level parallelism budget (default: capped CPU count).",
    ),
) -> None:
    """Drift-check every committed artifact against its canonical source.

    Runs the dogfooded build DAG in CHECK mode through the Rust ``gmeow-pipeline``
    executor (the build authority since #861 P7) and exits non-zero if any
    committed artifact has drifted from what the pipeline reproduces.
    """
    report = _run_pipeline(jobs=jobs, check=True)
    for finding in report.get("findings", []):
        err_console.print(
            f"[yellow]{finding['severity']}[/yellow] {finding['code']}: "
            f"{finding['message']}"
        )
    drifted = report.get("drifted", [])
    if drifted:
        for rel in sorted(drifted):
            err_console.print(f"[red]drift[/red] {rel}")
        raise _fail(f"✗ {len(drifted)} artifact(s) drifted — run `gmeow regenerate`")
    console.print(
        f"[green]✓ all {report['reproduced']} committed artifact(s) match "
        "canonical sources (no drift)[/green]"
    )


@app.command()
def validate(
    timings: bool = typer.Option(False, "--timings", help="Report per-phase timings."),
    gts: Path | None = typer.Option(  # noqa: B008
        None,
        "--gts",
        help="Validate a .gts bundle directly instead of the repo Turtle sources.",
    ),
    trust_policy: Path | None = typer.Option(  # noqa: B008
        None,
        "--trust-policy",
        help="TOML file with trusted signer KIDs and policy settings.",
    ),
    require_signed: bool = typer.Option(
        False,
        "--require-signed",
        help="Fail if the GTS bundle has no valid signature.",
    ),
    trusted_key: Path | None = typer.Option(  # noqa: B008
        None,
        "--trusted-key",
        help="Out-of-band armored OpenPGP public key (optional).",
    ),
    deep: bool = typer.Option(
        False,
        "--deep",
        help=(
            "Run the native semantic pass after structural validation: reason over "
            "the bundle and fold the shared logic:ReasoningResult verdict "
            "(inconsistency, unsatisfiable classes, undecided constructs) into the "
            "report. Runs the full reasoner, so it is opt-in (#768)."
        ),
    ),
) -> None:
    """Validate Turtle syntax, term annotations, and SHACL conformance.

    In normal mode this checks the repository Turtle sources. When ``--gts`` is
    given, validate a folded GTS bundle directly instead. If any signature or
    trust flag is supplied with ``--gts``, a signature/trust verification
    pre-gate runs before ontology validation (#646).

    The pre-gate verifies embedded GTS signatures against the configured trust
    policy: ``--trust-policy`` loads a TOML file with trusted signer KIDs and
    optional out-of-band key material; ``--require-signed`` hard-fails bundles
    with no valid signature; ``--trusted-key`` supplies an armored OpenPGP
    public key directly and overrides any ``trusted_key`` path in the policy
    file.
    """
    from gmeow_tools.diagnostics import emit_legacy_cli, report_from_validation_result
    from gmeow_tools.validate import validate_all

    signature_flags = (
        trust_policy is not None or require_signed or trusted_key is not None
    )
    if signature_flags and gts is None:
        raise typer.BadParameter(
            "--trust-policy/--require-signed/--trusted-key require --gts"
        )

    signature_config: dict[str, object] | None = None
    if signature_flags:
        signature_config = {
            "trusted_signers": [],
            "require_signatures": require_signed,
            "require_trusted_signer": False,
            "trusted_key": None,
        }
        if trust_policy is not None:
            try:
                policy = tomllib.loads(trust_policy.read_text(encoding="utf-8"))
            except OSError as exc:
                raise _fail(
                    f"cannot read --trust-policy {trust_policy}: {exc}"
                ) from exc
            except tomllib.TOMLDecodeError as exc:
                raise _fail(
                    f"invalid TOML in --trust-policy {trust_policy}: {exc}"
                ) from exc
            signature_config["trusted_signers"] = list(
                policy.get("trusted_signers", [])
            )
            signature_config["require_trusted_signer"] = bool(
                policy.get("require_trusted_signer", False)
            )
            policy_key = policy.get("trusted_key")
            if policy_key is not None:
                key_path = Path(policy_key)
                if not key_path.is_absolute():
                    key_path = trust_policy.parent / key_path
                try:
                    signature_config["trusted_key"] = key_path.read_text(
                        encoding="utf-8"
                    )
                except OSError as exc:
                    raise _fail(f"cannot read trusted key {key_path}: {exc}") from exc
        if trusted_key is not None:
            # CLI --trusted-key takes precedence over any trusted_key path in the
            # policy file. It is read here so the Rust pre-gate receives the raw
            # armored key rather than a filesystem path.
            try:
                signature_config["trusted_key"] = trusted_key.read_text(
                    encoding="utf-8"
                )
            except OSError as exc:
                raise _fail(f"cannot read --trusted-key {trusted_key}: {exc}") from exc

    result = validate_all(
        timings=timings, gts_input=gts, signature_config=signature_config, deep=deep
    )
    report = report_from_validation_result(result, tool="validate")
    emit_legacy_cli(report, err_console)
    if timings and result.timings:
        err_console.print("[dim]timings:[/dim]")
        for record in result.timings:
            phase = record.get("phase", "?")
            elapsed = record.get("elapsed_ms", 0)
            meta = record.get("metadata") or ""
            line = f"  {phase}: {elapsed} ms"
            if meta:
                line += f" ({meta})"
            err_console.print(f"[dim]{line}[/dim]")
    if result.ok:
        console.print("[green]✓ validation passed[/green]")
    else:
        raise _fail(f"✗ {len(result.errors)} error(s)")


def _surface_reports() -> list[tuple[str, Callable[[], DiagnosticsReport]]]:
    """The ``(label, thunk)`` table of dev-gate surfaces folded into feedback.

    Each thunk re-runs one ``make check`` surface and returns its
    ``DiagnosticsReport``. The thunks mirror exactly what the corresponding
    ``make`` targets run (offline lanes only). This table is the single place a
    migrated surface is registered;
    ``test_surface_reports_covers_every_migrated_surface`` pins it against
    ``_EXPECTED_SURFACES`` so the table cannot drift from the documented surface
    set. (``validate`` + native ``reason``/``verify`` are folded separately in
    :func:`feedback`; ROBOT and external-tool lanes are a documented follow-up.)
    """

    def _alignment() -> DiagnosticsReport:
        from gmeow_tools import diagnostics

        items = [
            diagnostics.finding(
                severity=finding["severity"].lower(),
                code=f"alignment.{finding['check']}",
                message=finding["message"],
                tool="alignment",
                logical=finding["instance"],
            )
            for finding in _alignment_findings(network=False)
        ]
        return diagnostics.report_from_findings(tool="alignment", findings=items)

    def _coverage() -> DiagnosticsReport:
        from gmeow_tools import coverage

        return coverage.to_diagnostics_report(coverage.run_coverage())

    def _acceptance() -> DiagnosticsReport:
        import gmeow_native.pipeline as _pipeline

        return _pipeline.acceptance_diagnostics_report(str(PROJECT_ROOT))

    def _wikidata() -> DiagnosticsReport:
        return gmeow_validate.wikidata_diagnostics_report(str(MAPPINGS_DIR))

    def _constitution() -> DiagnosticsReport:
        return gmeow_validate.constitution_full_report(
            str(PROJECT_ROOT / "governance" / "constitution.ttl"),
            str(PROJECT_ROOT / "CONSTITUTION.md"),
            str(PROJECT_ROOT),
        )

    def _crate_layering() -> DiagnosticsReport:
        return gmeow_validate.crate_layering_diagnostics_report(
            str(PROJECT_ROOT / "crates")
        )

    def _repo_static() -> DiagnosticsReport:
        return gmeow_validate.repo_static_diagnostics_report(str(PROJECT_ROOT))

    def _box_roles() -> DiagnosticsReport:
        from gmeow_tools import box_roles

        return box_roles.to_diagnostics_report(box_roles.audit_box_roles())

    def _audit() -> DiagnosticsReport:
        import gmeow_native.pipeline as _pipeline

        from gmeow_tools.config import FIXTURES_DIR

        corpus = FIXTURES_DIR / "hallucination-kg.ttl"
        return _pipeline.claim_audit_diagnostics_report(
            str(PROJECT_ROOT), [str(corpus)]
        )

    def _generated() -> DiagnosticsReport:
        # Drift surface for the build: run the Rust pipeline in CHECK mode (the
        # build authority since #861 P7) and project its drift findings into the
        # canonical diagnostics report folded into the bundle.
        from gmeow_tools import diagnostics

        report = _run_pipeline(check=True)
        items = [
            diagnostics.finding(
                severity="error",
                code="generator.drift",
                message=rel,
                tool="pipeline",
                path=rel,
            )
            for rel in report.get("drifted", [])
        ]
        items += [
            diagnostics.finding(
                severity=finding["severity"],
                code=finding["code"],
                message=finding["message"],
                tool="pipeline",
            )
            for finding in report.get("findings", [])
            if finding["severity"] == "error"
        ]
        return diagnostics.report_from_findings(tool="generated", findings=items)

    def _classic_cross_check() -> DiagnosticsReport:
        # The native↔oracle (ELK/HermiT/ROBOT) divergence ledger is already a
        # Rust-backed DiagnosticsReport (gmeow_logic.build_divergence_ledger →
        # classic_cross_check.build_report). Folding it carries the classic-oracle
        # cross-check findings into the bundle. Guarded: it needs the Docker/Java
        # lane, so on a Docker-less host the fold loop records a visible skip.
        from gmeow_tools.oracles import classic_cross_check as crosscheck

        _passed, _ledger, report = crosscheck.run()
        return report

    def _engine_cross_check() -> DiagnosticsReport:
        from gmeow_tools.oracles import engine_crosscheck

        return engine_crosscheck.build_report(engine_crosscheck.crosscheck_all())

    def _logic_compile() -> DiagnosticsReport:
        from gmeow_tools import logic_compile

        return logic_compile.compile_diagnostics_report()

    def _statement_compile() -> DiagnosticsReport:
        return _statement_compile_report()

    def _mapping_compile() -> DiagnosticsReport:
        return _mapping_compile_report()

    def _slice_ownership() -> DiagnosticsReport:
        # The FULL native slice-ownership report (#809): ownership-defect errors
        # PLUS the dependency-observation warnings that `make validate` keeps out
        # of its focused gate. Folding it here carries those previously-dropped
        # warnings, structured, into SARIF/JSON/HTML + gmeow.gts.
        from gmeow_tools import validate

        return validate.native_ownership_report()

    return [
        ("alignment", _alignment),
        ("coverage", _coverage),
        ("acceptance", _acceptance),
        ("wikidata", _wikidata),
        ("constitution", _constitution),
        ("crate-layering", _crate_layering),
        ("repo-static", _repo_static),
        ("box-roles", _box_roles),
        ("audit", _audit),
        ("generated", _generated),
        ("classic-cross-check", _classic_cross_check),
        ("engine-cross-check", _engine_cross_check),
        ("logic-compile", _logic_compile),
        ("statement-compile", _statement_compile),
        ("mapping-compile", _mapping_compile),
        ("slice-ownership", _slice_ownership),
    ]


def _fold_surfaces(report: Any) -> None:
    """Fold every migrated dev-gate surface's findings into ``report`` (#654).

    Mutates ``report`` in place. Each surface thunk is guarded: a surface that
    fails to run leaves a visible ``feedback.<label>-skipped`` *warning* finding
    rather than aborting the whole bundle. This swallow is correct ONLY because
    ``feedback`` is an artifact-builder, not a gate — one surface erroring must
    not blind the bundle to the others, and the skip is surfaced (fix-or-
    document, hide none), NOT a degraded-fallback path. Per-surface hard gating
    stays in each surface's own ``make check`` command; ``feedback``'s process
    exit stays driven solely by the validation result.
    """
    from gmeow_tools import diagnostics

    for label, thunk in _surface_reports():
        try:
            report.extend(thunk())
        except Exception as exc:  # artifact-builder: isolate per surface, warn with exc
            report.add(
                diagnostics.finding(
                    severity="warning",
                    code=f"feedback.{label}-skipped",
                    message=f"{label} findings not folded: {exc}",
                    tool="feedback",
                )
            )


@app.command()
def feedback(
    diagnostics_console: str | None = typer.Option(
        None,
        "--diagnostics-console",
        help="Console projection: auto|pretty|text|jsonl|silent "
        "(env GMEOW_DIAGNOSTICS_CONSOLE).",
    ),
    diagnostics_artifacts: str | None = typer.Option(
        None,
        "--diagnostics-artifacts",
        help="Artifact files to write: none|all|comma list of json,sarif,html "
        "(env GMEOW_DIAGNOSTICS_ARTIFACTS).",
    ),
    diagnostics_dir: Path | None = typer.Option(  # noqa: B008
        None,
        "--diagnostics-dir",
        help="Output directory (env GMEOW_DIAGNOSTICS_DIR). Defaults under dist/; "
        "CI category runs land under dist/diagnostics/<category>/.",
    ),
    diagnostics_stem: str | None = typer.Option(
        None,
        "--diagnostics-stem",
        help="Output filename stem (env GMEOW_DIAGNOSTICS_STEM; "
        "default gmeow-feedback).",
    ),
    diagnostics_category: str | None = typer.Option(
        None,
        "--diagnostics-category",
        help="Stable category for SARIF metadata and CI code-scanning grouping "
        "(env GMEOW_DIAGNOSTICS_CATEGORY).",
    ),
    timings: bool = typer.Option(False, "--timings", help="Record validation timings."),
) -> None:
    """Write first-class diagnostics artifacts for the whole dev gate.

    Folds validation, native reason/verify, AND every other migrated ``make
    check`` surface (alignment, coverage, acceptance, wikidata, constitution,
    box-roles, audit, generator drift) into ONE report, then projects it to the
    console (per ``--diagnostics-console``) and writes the selected
    ``<stem>.{json,sarif,html}`` artifacts (per ``--diagnostics-artifacts``) plus
    the self-describing ``<stem>.gts`` feedback bundle (the findings as queryable
    RDF plus the SARIF and JSON projections as content-addressed blobs, #654). The
    canonical ``gmeow.gts`` is never touched.

    All five ``--diagnostics-*`` knobs mirror ``GMEOW_DIAGNOSTICS_*`` env vars
    (flag > env > default) so Make and CI set policy once (#662). A
    ``--diagnostics-category`` rides into the SARIF run as ``automationDetails.id``
    for per-category GitHub code-scanning grouping, and (off a TTY, with no
    explicit dir) lands artifacts under ``dist/diagnostics/<category>/``.

    The process **exit code stays driven solely by the validation result** — the
    bundle carries every surface's findings as an artifact, but per-surface hard
    gating lives in each surface's own ``make check`` command, not here. ``silent``
    / ``none`` change what is shown or written, never the exit code.
    """
    import json

    from gmeow_tools import diagnostics
    from gmeow_tools.diagnostics import (
        emit_console,
        report_from_validation_result,
        write_report_artifacts,
    )
    from gmeow_tools.diagnostics_config import DiagnosticsConfig
    from gmeow_tools.feedback_bundle import build_feedback_bundle
    from gmeow_tools.validate import validate_all

    config = DiagnosticsConfig.resolve(
        console=diagnostics_console,
        artifacts=diagnostics_artifacts,
        directory=diagnostics_dir,
        stem=diagnostics_stem,
        category=diagnostics_category,
    )

    result = validate_all(timings=timings)
    report = report_from_validation_result(result, tool="validate")

    # Fold the native (Java/Docker-free) reasoning + reasoned-graph verify lanes
    # into the same report so their findings ride the shared SARIF + self-attesting
    # .gts feedback bundle (#695). The bundle then carries validation + reasoning +
    # verify findings, all self-attested.
    try:
        from gmeow_tools import reason as reasoning

        report.extend(
            reasoning.reason_native(output_dir=config.directory, run_box_roles=False)
        )
        report.extend(reasoning.verify_native(output_dir=config.directory))
    except (ImportError, ValueError, RuntimeError, OSError, FileNotFoundError) as exc:
        report.add(
            diagnostics.finding(
                severity="warning",
                code="feedback.native-skipped",
                message=f"native reason/verify findings not folded: {exc}",
                tool="feedback",
            )
        )

    # Fold every other migrated dev-gate surface (alignment, coverage,
    # acceptance, wikidata, constitution, box-roles, audit, generator drift) so
    # the bundle is the complete picture of the gate, not just validation (#654).
    _fold_surfaces(report)

    # The stable category rides into the report metadata so the Rust SARIF
    # renderer can emit run-level automationDetails.id (per-category grouping).
    report.set_metadata_json("category", json.dumps(config.category))

    emit_console(report, config, err_console)
    paths = write_report_artifacts(
        report,
        output_dir=config.directory,
        stem=config.stem,
        artifacts=config.artifacts,
    )
    for kind in ("json", "sarif", "html"):
        if kind in paths:
            console.print(f"[green]wrote[/green] {paths[kind]}")

    # The self-describing feedback bundle is the canonical record (findings RDF +
    # SARIF/JSON blobs), not a selectable projection — always written.
    config.directory.mkdir(parents=True, exist_ok=True)
    bundle_path = config.directory / f"{config.stem}.gts"
    bundle_path.write_bytes(build_feedback_bundle(report))
    console.print(f"[green]wrote[/green] {bundle_path}")

    if result.ok:
        console.print("[green]✓ diagnostics feedback written[/green]")
    else:
        raise _fail(f"✗ {len(result.errors)} error(s)")


@app.command(name="external-tool")
def external_tool_cmd(
    command: list[str] = typer.Argument(  # noqa: B008
        ...,
        help="The external command to run, e.g. `mypy src`. Use `--` to separate "
        "it from this command's own options.",
    ),
    name: str = typer.Option(
        ...,
        "--name",
        help="Stable tool name for the external.<name> finding code (e.g. mypy).",
    ),
    diagnostics_console: str | None = typer.Option(
        None, "--diagnostics-console", help="auto|pretty|text|jsonl|silent."
    ),
    diagnostics_artifacts: str | None = typer.Option(
        None, "--diagnostics-artifacts", help="none|all|comma list of json,sarif,html."
    ),
    diagnostics_dir: Path | None = typer.Option(  # noqa: B008
        None, "--diagnostics-dir", help="Output directory (env GMEOW_DIAGNOSTICS_DIR)."
    ),
    diagnostics_stem: str | None = typer.Option(
        None, "--diagnostics-stem", help="Output filename stem."
    ),
    diagnostics_category: str | None = typer.Option(
        None, "--diagnostics-category", help="Stable code-scanning category."
    ),
) -> None:
    """Run an external gate tool and represent a failure as a canonical finding.

    Wraps a tool GMEOW does not own (pre-commit, mypy, pytest, cargo, clippy,
    maturin) so its raw log rides the same diagnostics rail — projected to the
    console and written as the selected ``<stem>.{json,sarif,html}`` artifacts
    under the resolved (optionally category-scoped) directory (#662). The five
    ``--diagnostics-*`` knobs and ``GMEOW_DIAGNOSTICS_*`` env vars resolve exactly
    as for ``feedback``.

    The process **exit code mirrors the wrapped tool**: zero when it succeeds,
    non-zero when it fails — so a CI gate still fails on the underlying tool while
    the failure is also captured as a finding. Output config governs projection,
    never the exit code.
    """
    import json

    from gmeow_tools import external_tool
    from gmeow_tools.diagnostics import emit_console, write_report_artifacts
    from gmeow_tools.diagnostics_config import DiagnosticsConfig

    config = DiagnosticsConfig.resolve(
        console=diagnostics_console,
        artifacts=diagnostics_artifacts,
        directory=diagnostics_dir,
        stem=diagnostics_stem,
        category=diagnostics_category,
    )

    exit_code, report = external_tool.run_external_tool(name, command)
    report.set_metadata_json("category", json.dumps(config.category))

    emit_console(report, config, err_console)
    paths = write_report_artifacts(
        report,
        output_dir=config.directory,
        stem=config.stem,
        artifacts=config.artifacts,
    )
    for kind in ("json", "sarif", "html"):
        if kind in paths:
            console.print(f"[green]wrote[/green] {paths[kind]}")

    if report.ok:
        console.print(f"[green]✓ {name} passed[/green]")
    else:
        # Mirror the wrapped tool's exact exit code, not a generic 1, so callers
        # chaining on $? see the real status. Guard the success codepath: a report
        # with findings but a 0 exit still fails (use 1).
        err_console.print(f"[red]✗ {name} failed ({report.error_count} error(s))[/red]")
        raise typer.Exit(code=exit_code if exit_code != 0 else 1)


@app.command(name="constitution-check")
def constitution_check() -> None:
    """Verify every constitutional principle has live enforcement (#280)."""
    report = gmeow_validate.constitution_full_report(
        str(PROJECT_ROOT / "governance" / "constitution.ttl"),
        str(PROJECT_ROOT / "CONSTITUTION.md"),
        str(PROJECT_ROOT),
    )
    for f in report.findings:
        if f["severity"] == "warning":
            err_console.print(f"[yellow]warning[/yellow] {f['message']}")
        elif f["severity"] == "error":
            err_console.print(f"[red]error[/red] {f['message']}")
    if report.ok:
        console.print("[green]✓ constitution check passed[/green]")
    else:
        raise _fail("✗ constitution check failed")


box_roles_app = typer.Typer(
    help="Audit explicit graph-box role coverage in authored sources.",
    no_args_is_help=True,
)
app.add_typer(box_roles_app, name="box-roles")


@box_roles_app.command(name="audit")
def box_roles_audit(
    json_out: bool = typer.Option(
        False,
        "--json",
        help="Emit machine-readable JSON instead of text.",
    ),
) -> None:
    """Audit explicit ABox/TBox/RBox/CBox/ConfigBox role coverage."""
    from gmeow_tools.box_roles import audit_box_roles, render_json, render_text

    report = audit_box_roles()
    console.print(render_json(report) if json_out else render_text(report))
    if not report.ok:
        raise _fail(
            f"✗ {len(report.missing)} missing, {len(report.invalid)} invalid role(s)"
        )


@app.command()
def audit(
    files: list[Path] = typer.Argument(  # noqa: B008
        ...,
        help="Turtle data files to audit against the claim gates (#55).",
    ),
    json_out: bool = typer.Option(
        False, "--json", help="Emit the documented flat-JSON claim shape."
    ),
    strict: bool = typer.Option(
        False,
        "--strict",
        help="Exit non-zero when any claim is flagged (default: report only).",
    ),
) -> None:
    """Audit claims: ungrounded / contradicted / stale, flagged never deleted."""
    import gmeow_native.pipeline as _pipeline

    try:
        report = cast(
            "dict[str, Any]",
            _pipeline.claim_audit(str(PROJECT_ROOT), [str(path) for path in files]),
        )
    except (ImportError, OSError, RuntimeError, ValueError) as exc:
        raise _fail(str(exc)) from exc
    if json_out:
        console.out(str(report["json"]))
    else:
        console.print(str(report["text"]), markup=False, highlight=False)
    shacl_errors = cast("list[str]", report.get("shacl_errors", []))
    flagged = int(report.get("flagged", 0))
    if shacl_errors:
        raise _fail(f"✗ {len(shacl_errors)} SHACL error(s)")
    if strict and flagged:
        raise _fail(f"✗ {flagged} flagged claim(s) (--strict)")


@app.command(name="compliance-report")
def compliance_report_cmd(
    from_passing_check: bool = typer.Option(
        False,
        "--from-passing-check",
        help=(
            "Render pass evidence from gates already run by make check/CI "
            "instead of rerunning the in-process gate set."
        ),
    ),
) -> None:
    """Emit the RDF compliance report, running gates unless told they passed."""
    from gmeow_tools.compliance import write_report

    path = write_report(assume_runners_passed=from_passing_check)
    console.print(f"[green]✓ compliance report written to {path}[/green]")


@app.command(name="crosscheck-queries")
def crosscheck_queries() -> None:
    """Prove rdflib and gmeow_rdf answer every committed query identically.

    The trust anchor that licenses the test suite to run on the fast gmeow_rdf
    engine: each query under ``queries/`` is executed on the same merged graph
    under both engines and the answers compared by value. Any divergence fails.
    The agreement matrix is also written as JSON/SARIF/HTML via the diagnostics
    rail (#667 — the surface no longer terminates at stdout only).
    """
    from gmeow_tools.oracles.engine_crosscheck import run

    _passed, results, _report = run()
    diverged = [r for r in results if not r.agree and not r.skipped]
    skipped = [r for r in results if r.skipped]
    checked = [r for r in results if not r.skipped]
    for r in skipped:
        err_console.print(f"[yellow]skip[/yellow] {r.name} ({r.detail})")
    for r in diverged:
        err_console.print(f"[red]diverge[/red] [{r.form}] {r.name}: {r.detail}")
    if diverged:
        raise _fail(
            f"✗ {len(diverged)} query/queries diverge between rdflib and gmeow_rdf"
        )
    console.print(
        f"[green]✓ {len(checked)} queries agree across rdflib + gmeow_rdf"
        f" ({len(skipped)} skipped)[/green]"
    )


@app.command(name="classic-cross-check")
def classic_cross_check() -> None:
    """Enforced native↔oracle divergence cross-check (#666 — Docker/Java lane).

    The FINAL, ENFORCING step of ``make maint-classic-cross-check`` (the sole
    Docker/Java surface, Principle 18). It reasons the bundle natively
    (authority), runs the classic ELK + HermiT oracles (timing each), calls the
    authoritative Rust
    comparator, writes the agreement matrix + per-tool timing as SARIF/JSON, and
    fails NON-ZERO on any real divergence (``NativeOnly`` / ``OracleOnly``) or
    native coverage defect (``DlGap``). NEVER part of ``make check`` or the
    required ``quality`` gate.
    """
    from gmeow_tools.oracles import classic_cross_check as crosscheck
    from gmeow_tools.runner import ToolExecutionError, ToolUnavailableError

    try:
        passed, ledger, _report = crosscheck.run()
    except ToolUnavailableError as exc:
        raise _fail(f"tool unavailable: {exc}", code=2) from exc
    except ToolExecutionError as exc:
        raise _fail(f"classic cross-check oracle failed:\n{exc.output}") from exc

    console.print(
        "[bold]classic cross-check[/bold] — agreement matrix: "
        f"agree={ledger['agree']} native_only={ledger['native_only']} "
        f"oracle_only={ledger['oracle_only']} dl_gap={ledger['dl_gap']}"
    )
    if passed:
        console.print(
            "[green]✓ native ≡ oracle (ELK/HermiT) with zero native DL gaps — "
            "enforced cross-check passed[/green]"
        )
        return
    for row in ledger["rows"]:
        if row["kind"] in ("NativeOnly", "OracleOnly", "DlGap"):
            err_console.print(f"[red]{row['kind']}[/red] {row['detail']}")
    raise _fail(
        f"✗ native↔oracle divergence: {ledger['native_only']} native-only + "
        f"{ledger['oracle_only']} oracle-only + {ledger['dl_gap']} dl-gap row(s)"
    )


@app.command(name="classic-cross-check-rl")
def classic_cross_check_rl() -> None:
    """Enforced native-RL ≡ owlrl-RL agreement axis (#666 Task 5 — lane only).

    The native OWL 2 RL engine is the primary Docker-free entailment authority (the
    8 converted conformance suites run on it); ``owlrl`` lives ONLY here, in the
    lane, as the agreement ORACLE. This reasons the told facts under BOTH RL
    closures, compares the canonicalized named-vocabulary closures, writes the
    agreement matrix + per-engine timing as SARIF/JSON, and fails NON-ZERO on any
    real RL divergence. NEVER part of ``make check`` or the required gate.
    """
    from gmeow_tools.oracles import rl_agreement

    passed, result, _report = rl_agreement.run()

    native_only = result["native_only"]
    oracle_only = result["oracle_only"]
    assert isinstance(native_only, list)
    assert isinstance(oracle_only, list)
    console.print(
        "[bold]RL cross-check[/bold] — agreement: "
        f"agree={result['agree']} native_only={len(native_only)} "
        f"oracle_only={len(oracle_only)}"
    )
    if passed:
        console.print(
            "[green]✓ native RL ≡ owlrl RL (named-vocabulary closure) — "
            "enforced RL agreement passed[/green]"
        )
        return
    for row in native_only:
        err_console.print(f"[red]NativeOnly[/red] {row}")
    for row in oracle_only:
        err_console.print(f"[red]OracleOnly[/red] {row}")
    raise _fail(
        f"✗ native↔owlrl RL divergence: {len(native_only)} native-only + "
        f"{len(oracle_only)} oracle-only row(s)"
    )


@app.command()
def reason(
    mode: str = typer.Option(
        "native",
        "--mode",
        help=(
            "Reasoning backend: native (Rust, Java/Docker-free authority) or "
            "docker (classic ELK/HermiT oracle lane for the divergence ledger)."
        ),
    ),
    merge: bool = typer.Option(
        False,
        "--merge",
        help="Native mode: emit the union of the asserted + derived closure.",
    ),
    reasoner: str = typer.Option("ELK", help="Reasoner: ELK (fast) or hermit (DL)."),
    profile: str = typer.Option("DL", help="OWL 2 profile to validate against."),
    full: bool = typer.Option(
        False, "--full", help="Build the reasoned closure (gmeow-full.ttl)."
    ),
    exclude_tautologies: str | None = typer.Option(
        None,
        "--exclude-tautologies",
        help="Exclude tautologies from the reasoned output (e.g. 'structural').",
    ),
) -> None:
    """Reason over the ontology — native (authority) or docker (oracle) lane.

    The native lane runs the Rust EL/DL engine (Java/Docker-free), is the
    authority, emits the inferred-closure RDF 1.2 graph plus SARIF diagnostics,
    and fails on inconsistency. The docker lane keeps the classic ELK/HermiT
    pipeline reachable for the divergence ledger (``--reasoner``/``--profile``/
    ``--full``/``--exclude-tautologies`` apply to it).
    """
    from gmeow_tools import reason as reasoning
    from gmeow_tools.runner import ToolExecutionError, ToolUnavailableError

    if mode == "native":
        try:
            # emit_legacy_cli pulls in the gmeow_diagnostics extension; import it
            # only in the native lane (the Docker oracle lane — and the CI jobs
            # that run it without that extension — never need it), and inside the
            # guard so a missing/failed extension renders cleanly too.
            from gmeow_tools.diagnostics import emit_legacy_cli

            report = reasoning.reason_native(merge=merge)
            emit_legacy_cli(report, err_console)
        except ToolUnavailableError as exc:
            raise _fail(f"tool unavailable: {exc}", code=2) from exc
        except ToolExecutionError as exc:
            raise _fail(f"native reasoning failed:\n{exc.output}") from exc
        except (ImportError, ValueError, RuntimeError, OSError) as exc:
            # ImportError: native diagnostics extension unavailable; ValueError:
            # unreadable GTS bundle; RuntimeError: native chase failure; OSError:
            # artifact write failure. Render as a formatted diagnostic instead of
            # leaking a raw traceback.
            raise _fail(f"native reasoning failed: {exc}") from exc
        if report.ok:
            console.print("[green]✓ native EL/DL reasoning (Docker-free)[/green]")
            return
        raise _fail(f"✗ inconsistent / {report.error_count} error(s)")

    if mode != "docker":
        raise _fail(f"unknown reasoning mode: {mode!r} (expected native or docker)")

    try:
        reasoning.merge_release()
        console.print("[green]✓ merged import closure[/green]")
        reasoning.validate_profile(profile)
        console.print(f"[green]✓ OWL 2 {profile} profile[/green]")
        reasoning.reason(reasoner, exclude_tautologies=exclude_tautologies)
        console.print(f"[green]✓ {reasoner} consistency (no incoherence)[/green]")
        if full:
            out = reasoning.build_full()
            console.print(f"[green]✓ reasoned closure → {out.name}[/green]")
    except ToolUnavailableError as exc:
        raise _fail(f"tool unavailable: {exc}", code=2) from exc
    except ToolExecutionError as exc:
        raise _fail(f"reasoning failed:\n{exc.output}") from exc


@app.command()
def explain() -> None:
    """Explain unsatisfiable classes / inconsistency (HermiT)."""
    from gmeow_tools import reason as reasoning
    from gmeow_tools.runner import ToolUnavailableError

    try:
        report = reasoning.explain_unsatisfiable()
    except ToolUnavailableError as exc:
        raise _fail(f"tool unavailable: {exc}", code=2) from exc
    console.print(report or "[green]✓ no unsatisfiable classes[/green]")


@app.command()
def verify(
    mode: str = typer.Option(
        "native",
        "--mode",
        help=(
            "Verify backend: native (Rust reasoned closure, Java/Docker-free "
            "authority) or docker (classic ROBOT verify, classic-cross-check oracle)."
        ),
    ),
    reasoner: str = typer.Option("ELK", help="Reasoner: ELK (fast) or hermit (DL)."),
    reasoned_input: Path | None = _REASONED_INPUT_OPTION,
) -> None:
    """Run reasoned-graph negative tests — native (authority) or docker (oracle).

    The closed-world QC lane of the hybrid OWL+SHACL architecture: reason, then
    run each SPARQL "bad-example" query over the materialized graph. Any returned
    row is a violation (the OBO QC pattern), failing the gate. The native lane
    runs the Rust EL/DL closure (Java/Docker-free) and emits SARIF diagnostics;
    the docker lane keeps the classic ROBOT verify reachable for the
    classic-cross-check oracle (never on a required gate).
    """
    from gmeow_tools import reason as reasoning
    from gmeow_tools.runner import ToolExecutionError, ToolUnavailableError

    if mode == "native":
        try:
            from gmeow_tools.diagnostics import emit_legacy_cli

            report = reasoning.verify_native()
            emit_legacy_cli(report, err_console)
        except (
            ImportError,
            ValueError,
            RuntimeError,
            OSError,
            FileNotFoundError,
        ) as exc:
            # ImportError: native extension unavailable; ValueError: unreadable
            # GTS bundle; RuntimeError: native verify failure; OSError: artifact
            # write failure; FileNotFoundError: no verify queries.
            raise _fail(f"native verify failed: {exc}") from exc
        if report.ok:
            console.print(
                "[green]✓ verify: no violations on the reasoned graph "
                "(native, Docker-free)[/green]"
            )
            return
        raise _fail(
            f"✗ verify: {report.error_count} violation(s) on the reasoned graph"
        )

    if mode != "docker":
        raise _fail(f"unknown verify mode: {mode!r} (expected native or docker)")

    try:
        reasoning.verify(reasoner=reasoner, reasoned=reasoned_input)
    except ToolUnavailableError as exc:
        raise _fail(f"tool unavailable: {exc}", code=2) from exc
    except ToolExecutionError as exc:
        raise _fail(f"verify found violations:\n{exc.output}") from exc
    console.print(
        "[green]✓ verify: no violations on the reasoned graph (ROBOT)[/green]"
    )


@app.command()
def temporal(
    query: str = typer.Argument(..., help="TQL query name (e.g. timeline)."),
    data: str | None = typer.Option(None, help="Instance-data file (Turtle)."),
    focus: str | None = typer.Option(None, help="Focus event IRI."),
    window_start: str | None = typer.Option(None, help="Window start dateTime."),
    window_end: str | None = typer.Option(None, help="Window end dateTime."),
    valid_at: str | None = typer.Option(None, help="Valid-time instant."),
    as_of: str | None = typer.Option(None, help="Observation cutoff."),
) -> None:
    """Run a TQL (Temporal Query Language) query over the events model.

    A query algebra in standard SPARQL 1.1: Allen-relation closures, the event
    timeline, interval overlap, and the bitemporal four-clocks query. Parameters
    are bound safely (rdflib initBindings), never interpolated.
    """
    from gmeow_rdf.compat.rdflib import Literal, URIRef
    from gmeow_rdf.compat.rdflib.namespace import XSD
    from gmeow_rdf.compat.rdflib.util import guess_format

    from gmeow_tools.graph import load_merged_graph
    from gmeow_tools.temporal_query import TEMPORAL_QUERIES, run_temporal_query

    if query not in TEMPORAL_QUERIES:
        listing = "\n".join(
            f"  {q.name:<20} {q.summary}" for q in TEMPORAL_QUERIES.values()
        )
        raise _fail(f"unknown TQL query {query!r}. Available:\n{listing}")

    source = load_merged_graph(include_imports=False)
    if data is not None:
        source.parse(data, format=guess_format(data) or "turtle")

    def _dt(value: str) -> Literal:
        return Literal(value, datatype=XSD.dateTime)

    bindings: dict[str, object] = {}
    if focus is not None:
        bindings["focus"] = URIRef(focus)
    if window_start is not None:
        bindings["windowStart"] = _dt(window_start)
    if window_end is not None:
        bindings["windowEnd"] = _dt(window_end)
    if valid_at is not None:
        bindings["validAt"] = _dt(valid_at)
    if as_of is not None:
        bindings["asOf"] = _dt(as_of)

    try:
        rows = run_temporal_query(query, source, bindings or None)  # type: ignore[arg-type]
    except ValueError as exc:
        raise _fail(str(exc)) from exc
    for row in rows:
        console.print(" ".join(str(v) for v in row))
    console.print(f"[green]✓ {query}: {len(rows)} row(s)[/green]")


@app.command()
def extract(
    target: str = typer.Option(..., help="Alignment target key (license-checked)."),
) -> None:
    """Report the import/extract policy for an alignment target.

    Refuses (exit 1) for reference-only targets — the license guard that
    prevents copying NC/ND/copyleft axioms into CC BY 4.0 GMEOW.
    """
    from gmeow_tools.config import ALIGNMENT_TARGETS
    from gmeow_tools.extract import LicensePolicyError, guard_importable

    try:
        guard_importable(target)
    except LicensePolicyError as exc:
        raise _fail(f"✗ {exc}") from exc
    info = ALIGNMENT_TARGETS[target]
    console.print(
        f"[green]✓ {info.name} ({info.license}) is import-ok — "
        f"extraction permitted[/green]"
    )


@app.command(name="lint-alignment")
def lint_alignment(
    network: bool = typer.Option(
        False,
        "--network",
        help="Fetch reference-only target axioms (schema.org) live.",
    ),
    strict: bool = typer.Option(
        False, "--strict", help="Treat warnings as failures too."
    ),
) -> None:
    """Lint SSSOM property mappings for inverse / domain-range-mismatched targets.

    Validates each ``owl:equivalentProperty`` / ``skos:closeMatch`` row against the
    target term's own axioms (domain/range, ``owl:inverseOf``, property character).
    Offline by default — target axioms missing a vendored snapshot or fixture are
    reported as non-fatal info. ``--network`` fetches them live (incl. schema.org).
    """

    def _render(finding: ProjectionDiagnostic) -> str:
        check = finding["check"]
        instance = finding.get("instance")
        message = finding["message"]
        if instance:
            return escape(f"[{check}] {instance}: {message}")
        return escape(f"[{check}] {message}")

    findings = _alignment_findings(network=network)
    errors = [f for f in findings if f["severity"] == "ERROR"]
    warnings = [f for f in findings if f["severity"] == "WARNING"]
    infos = [f for f in findings if f["severity"] == "INFO"]

    for finding in errors:
        err_console.print(f"[red]error[/red] {_render(finding)}")
    for finding in warnings:
        err_console.print(f"[yellow]warning[/yellow] {_render(finding)}")
    if infos:
        console.print(f"[dim]{len(infos)} row(s) skipped (no target axioms)[/dim]")

    if errors or (strict and warnings):
        raise _fail(
            f"✗ {len(errors)} error(s), {len(warnings)} warning(s) in alignments"
        )
    console.print(
        f"[green]✓ alignment directions OK[/green] "
        f"({len(warnings)} warning(s), {len(infos)} skipped)"
    )


@app.command(name="doc-lint")
def doc_lint() -> None:
    """Lint the rust-rendered ontology-docs site for integrity + coverage.

    Builds the native ``gmeow_docs`` documentation set from the slice catalog and
    runs its lint, emitting a shared ``gmeow:Finding`` report. ERRORS are integrity
    defects (dangling internal links, broken in-page anchors) — a dangling link is
    always a render bug and fails the gate. WARNINGS are coverage gaps on the
    vocabulary surface (terms missing a definition, label, usage advice, example,
    scope note, or external alignment) and do not fail. The summarized render keeps
    errors in full while collapsing the high-volume coverage warnings to a per-code
    count, so the gate output stays digestible.
    """
    import gmeow_docs as _docs  # legacy-name shim → gmeow_native.docs submodule

    docset = _docs.DocSet.from_root(str(PROJECT_ROOT))
    report = docset.lint()

    text = report.render_text_summarized()
    if text.strip():
        console.print(text)

    if report.error_count > 0:
        raise _fail(
            f"✗ doc-lint: {report.error_count} error(s), "
            f"{report.warning_count} warning(s)"
        )
    console.print(f"[green]✓ doc-lint OK[/green] ({report.warning_count} warning(s))")


@app.command(name="crate-check")
def crate_check() -> None:
    """Verify Rust crate layering and repository-static policy.

    ``gmeow-rdf-core`` is the oxigraph-free RDF 1.2 kernel,
    ``gmeow-rdf-events`` is the neutral protocol seam, and ``gmeow-rdf`` is the
    oxigraph/PyO3 adapter that depends on and re-exports the core. The
    first-party crate dependency graph must stay acyclic. The same Rust gate also
    owns static repository policy: narrow-waist, lane-purity, and first-party
    upstream-rdflib import seals.
    """
    report = gmeow_validate.crate_layering_check(str(PROJECT_ROOT / "crates"))
    static_report = gmeow_validate.repo_static_check(str(PROJECT_ROOT))
    errors = [*list(report["errors"]), *list(static_report["errors"])]
    warnings = [*list(report["warnings"]), *list(static_report["warnings"])]
    edges = report["edges"]
    for message in errors:
        err_console.print(f"[red]error[/red] {message}")
    for message in warnings:
        err_console.print(f"[yellow]warning[/yellow] {message}")
    if errors:
        raise _fail(f"✗ {len(errors)} crate/static violation(s)")
    console.print(
        f"[green]✓ crate/static guards OK[/green] "
        f"({len(edges)} crates, RDF core pure, DAG acyclic; repo static seals clean)"
    )


@app.command(name="refresh-target-axioms")
def refresh_target_axioms(
    target: str = typer.Option(
        "all", help="Target prefix to refresh, or 'all' for every IMPORT_OK target."
    ),
) -> None:
    """Re-vendor minimal target-axiom snapshots into imports/targets/.

    Fetches each IMPORT_OK target's canonical document, keeps only the structural
    axioms (domain/range/inverse + property types), and writes the snapshot. Refuses
    reference-only targets (e.g. CC-BY-SA schema.org) — those are fetched live at
    lint time and never committed into the CC BY 4.0 artifact.
    """
    import httpx

    from gmeow_tools.config import ALIGNMENT_TARGETS, PROJECT_ROOT, LinkPolicy
    from gmeow_tools.extract import LicensePolicyError
    from gmeow_tools.target_axioms import TARGET_SOURCES, refresh_snapshot

    prefixes = list(TARGET_SOURCES) if target == "all" else [target]
    written = 0
    for prefix in prefixes:
        meta = ALIGNMENT_TARGETS.get(prefix)
        if meta is not None and meta.policy is not LinkPolicy.IMPORT_OK:
            err_console.print(
                f"[yellow]skip[/yellow] {prefix} ({meta.license}): reference-only — "
                "fetched live at lint time, not vendored"
            )
            continue
        try:
            path = refresh_snapshot(prefix)
        except LicensePolicyError as exc:
            raise _fail(f"✗ {exc}") from exc
        except httpx.HTTPError as exc:
            raise _fail(f"✗ fetch failed for {prefix}: {exc}", code=2) from exc
        console.print(f"[green]✓[/green] {path.relative_to(PROJECT_ROOT)}")
        written += 1
    console.print(f"[green]✓ refreshed {written} target snapshot(s)[/green]")


@app.command()
def mappings() -> None:
    """Build alignment axioms + VoID linksets from SSSOM, validating QIDs."""
    from gmeow_tools.config import DIST_DIR
    from gmeow_tools.mappings import (
        build_alignment_graph,
        build_linksets,
        load_mappings,
    )

    loaded = load_mappings()
    if not loaded:
        err_console.print("[yellow]no mappings found[/yellow]")
        return

    syntax = gmeow_validate.wikidata_mapping_syntax(str(MAPPINGS_DIR))
    if syntax["invalid"] or syntax["misuses"]:
        raise _fail(
            "✗ invalid Wikidata ids in mappings: "
            f"{syntax['invalid']} ({len(syntax['misuses'])} misuse(s))"
        )

    DIST_DIR.mkdir(parents=True, exist_ok=True)
    alignments = build_alignment_graph(loaded)
    alignments.serialize(destination=DIST_DIR / "gmeow-alignments.ttl", format="turtle")
    linksets = build_linksets(loaded)
    linksets.serialize(destination=DIST_DIR / "gmeow-linksets.ttl", format="turtle")
    from gmeow_rdf.compat.rdflib import RDF
    from gmeow_rdf.compat.rdflib.namespace import VOID

    n_links = len(set(linksets.subjects(RDF.type, VOID.Linkset)))
    console.print(
        f"[green]✓ {len(loaded)} mappings → {len(alignments)} alignment axioms[/green]"
    )
    console.print(f"[green]✓ {n_links} VoID linkset descriptions[/green]")
    console.print(
        f"[green]✓ {len(syntax['valid'])} Wikidata id(s) passed syntax[/green]"
    )


@app.command()
def wikidata(
    existence: bool = typer.Option(
        False, "--existence", help="Also check ids resolve on Wikidata (network)."
    ),
    fixtures: bool = typer.Option(
        False, "--fixtures", help="Audit fixtures and modules for Wikidata misuse."
    ),
) -> None:
    """Validate Wikidata QIDs/PIDs used in the mappings (syntax; optional live)."""
    from gmeow_tools.wikidata_audit import audit_all, render_audit

    if fixtures:
        report = audit_all(fixtures_dir=Path("tests/fixtures"))
        text = render_audit(report)
        for line in text.splitlines():
            if line.startswith("[yellow]") or line.startswith("[red]"):
                err_console.print(line)
            else:
                console.print(line)
        if not report.ok:
            raise _fail(
                f"✗ {len(report.errors)} error(s), {len(report.warnings)} warning(s)"
            )
        console.print("[green]✓ fixture audit passed[/green]")
        return

    syntax = gmeow_validate.wikidata_mapping_syntax(str(MAPPINGS_DIR))
    console.print(f"[green]✓ {len(syntax['valid'])} id(s) valid syntax[/green]")
    if syntax["invalid"]:
        err_console.print(f"[red]✗ invalid ids: {syntax['invalid']}[/red]")
    if syntax["misuses"]:
        for _local, misuse, message in syntax["misuses"]:
            err_console.print(f"[yellow]{misuse}[/yellow] {message}")
    if syntax["invalid"] or syntax["misuses"]:
        raise _fail(
            f"✗ {len(syntax['invalid'])} invalid, {len(syntax['misuses'])} misuse(s)"
        )

    if existence:
        try:
            statuses = gmeow_validate.wikidata_check_existence(
                syntax["valid"], str(PROJECT_ROOT)
            )
        except RuntimeError as exc:  # network failure -> visible, non-fatal skip
            err_console.print(f"[yellow]existence check skipped: {exc}[/yellow]")
            return
        bad = {k: v for k, v in statuses.items() if v != "ok"}
        for ident, status in bad.items():
            err_console.print(f"[red]{ident}: {status}[/red]")
        if bad:
            raise _fail(f"✗ {len(bad)} id(s) failed existence check")
        console.print(f"[green]✓ {len(statuses)} id(s) resolve on Wikidata[/green]")


@app.command()
def wikidata_coverage(
    json_mode: bool = typer.Option(
        False, "--json", help="Emit machine-readable JSON instead of plain text."
    ),
    threshold: float = typer.Option(
        0.5, "--threshold", help="Flag mappings below this confidence level."
    ),
) -> None:
    """Report Wikidata mapping coverage by domain/module (offline)."""
    text = gmeow_validate.wikidata_coverage_report(
        str(PROJECT_ROOT), str(MAPPINGS_DIR), threshold, json_mode
    )
    if json_mode:
        console.out(text)
    else:
        console.print(text)


@app.command()
def dc_coverage(
    json_mode: bool = typer.Option(
        False, "--json", help="Emit machine-readable JSON instead of plain text."
    ),
    threshold: float = typer.Option(
        0.5, "--threshold", help="Flag mappings below this confidence level."
    ),
) -> None:
    """Report Dublin Core mapping coverage by namespace (offline)."""
    text = gmeow_validate.dc_coverage_report(str(MAPPINGS_DIR), threshold, json_mode)
    if json_mode:
        console.out(text)
    else:
        console.print(text)


@app.command(name="up-projection-audit")
def up_projection_audit(
    report_path: Path | None = typer.Option(  # noqa: B008
        None,
        "--report",
        help="Write the full Markdown audit to this path (the summary still prints).",
    ),
    show_gaps: bool = typer.Option(
        False, "--gaps", help="List the coverage-gap terms."
    ),
) -> None:
    """Audit consumer→GMEOW up-projection invertibility on the real snapshots (#449)."""
    from gmeow_tools.up_projection_audit import render_markdown, run_audit

    report = run_audit()
    if report_path is not None:
        report_path.write_text(render_markdown(report), encoding="utf-8")
        console.print(f"[green]wrote[/green] {report_path}")
    pct = (100 * report.liftable // report.total) if report.total else 0
    console.print(
        f"[green]liftable[/green] {report.liftable}/{report.total} ({pct}%) "
        f"· SSSOM terms {report.sssom_total} · structural terms {report.struct_total}"
    )
    for f in report.files:
        console.print(f"  {f.name}: {f.liftable}/{f.total}")
    console.print(f"[yellow]gaps[/yellow] {len(report.gaps)} distinct terms")
    if show_gaps:
        for term in report.gaps:
            err_console.print(f"[yellow]gap[/yellow] {term}")


@app.command()
def coverage(
    show_gaps: bool = typer.Option(
        False, "--gaps", help="List the uncovered (gap) classes and predicates."
    ),
    min_class: float | None = typer.Option(
        None,
        "--min-class",
        help=(
            "Hard floor for class coverage (0..1). Exit 1 if the measured "
            "fraction is below it. Omit for report-only."
        ),
    ),
    min_predicate: float | None = typer.Option(
        None,
        "--min-predicate",
        help=(
            "Hard floor for predicate coverage (0..1). Exit 1 if the measured "
            "fraction is below it. Omit for report-only."
        ),
    ),
) -> None:
    """Report how much of the vendored entity slice GMEOW covers.

    With ``--min-class`` / ``--min-predicate`` the command becomes a HARD gate
    (#579): a measured coverage fraction below either floor exits 1. The floors
    are the project's vendored-entity coverage contract — the Makefile passes the
    current measured values so any regression below them fails the build.
    """
    from gmeow_tools.coverage import run_coverage

    report = run_coverage()
    console.print(
        f"[green]classes[/green]   {len(report.covered_classes)} covered / "
        f"{len(report.gap_classes)} gap "
        f"({report.class_coverage:.0%})"
    )
    console.print(
        f"[green]predicates[/green] {len(report.covered_predicates)} covered / "
        f"{len(report.gap_predicates)} gap "
        f"({report.predicate_coverage:.0%})"
    )
    if show_gaps:
        for iri in sorted(report.gap_classes):
            err_console.print(f"[yellow]gap class[/yellow] {iri}")
        for iri in sorted(report.gap_predicates):
            err_console.print(f"[yellow]gap predicate[/yellow] {iri}")

    if min_class is not None and report.class_coverage < min_class:
        raise _fail(
            f"✗ class coverage {report.class_coverage:.4f} is below the "
            f"required floor {min_class:.4f}"
        )
    if min_predicate is not None and report.predicate_coverage < min_predicate:
        raise _fail(
            f"✗ predicate coverage {report.predicate_coverage:.4f} is below the "
            f"required floor {min_predicate:.4f}"
        )


@app.command()
def crossref() -> None:
    """Generate (and doi-lint) the CrossRef DOI deposit XML for manual submission.

    The deposit is a transient submission document written to ``dist/`` (NOT a
    committed artifact): doi-lint runs first so an inconsistent deposit is never
    produced, then the registrant hand-verifies and submits it to CrossRef.
    """
    from gmeow_tools.crossref import lint_deposit, write_deposit
    from gmeow_tools.self_desc import load_self_description

    try:
        meta = load_self_description()
    except (FileNotFoundError, ValueError) as exc:
        raise _fail(f"✗ self-description unavailable: {exc}") from exc

    problems = lint_deposit(meta)
    if problems:
        for problem in problems:
            err_console.print(f"[red]doi-lint[/red] {problem}")
        raise _fail(
            f"✗ {len(problems)} doi-lint problem(s) — fix metadata/gmeow-self.ttl"
        )

    path = write_deposit(meta=meta)
    note = "concept-only" if meta.version_doi is None else "concept + version"
    console.print(f"[green]✓ {path} (DOI {meta.doi}, {note})[/green]")
    console.print(
        "[yellow]Review the deposit, then submit it to CrossRef manually.[/yellow]"
    )


@app.command()
def normalize() -> None:
    """Canonicalize the authored ontology sources for stable diffs."""
    from gmeow_tools.normalize import normalize_modules

    changed = normalize_modules()
    if changed:
        for path in changed:
            console.print(f"[yellow]normalized[/yellow] {path.name}")
    else:
        console.print("[green]✓ sources already canonical[/green]")


@app.command()
def build() -> None:
    """Build serializations and OWL-native syntaxes into dist/.

    The JSON-LD ``@context`` is no longer built here: it is emitted from the Rust
    ``PREFIX_REGISTRY`` authority by the ``mappings`` stage into
    ``generated/context.jsonld`` (and folded into ``gmeow.gts``), retiring the
    orphaned Python ``jsonld_context`` builder (#1009 §2 / #933).
    """
    from gmeow_rdf.compat.rdflib import Graph

    from gmeow_tools import reason as reasoning
    from gmeow_tools.runner import ToolUnavailableError
    from gmeow_tools.serialize import serialize_graph

    try:
        merged = reasoning.merge_release()
        owl_native = reasoning.convert_owl_syntaxes(merged=merged)
    except ToolUnavailableError as exc:
        raise _fail(f"tool unavailable: {exc}", code=2) from exc

    graph = Graph().parse(merged, format="turtle")
    written = serialize_graph(graph, stem="gmeow")
    for path in (*written.values(), *owl_native):
        console.print(f"[green]✓[/green] {path.relative_to(path.parents[1])}")


@app.command()
def project(
    source: Path | None = typer.Argument(  # noqa: B008
        None,
        help="A transpiled .gts to view, or a GMEOW data file (.ttl) to project; "
        "default: the worked-example fixtures.",
    ),
    profile: str = typer.Option(
        "all",
        help="Target view/profile: all|maximal|gmeow|"
        + "|".join(sorted(_PROFILES))
        + ".",
    ),
    data: str = typer.Option(
        "", help="(deprecated alias for the positional source — a GMEOW data file)."
    ),
    lang: str | None = _lang_option(),
) -> None:
    """Project GMEOW to a pure schema.org / FOAF / vCard / … profile.

    Two input kinds:

    * A **transpiled .gts** (the maximal product): the profile is a *view filter*
      — `--profile foaf` emits the FOAF subset already in the .gts, `--profile
      gmeow` the pure-GMEOW base, `--profile all` the whole maximal (GMEOW + every
      vocab). A filter of the already-down-projected artifact, never a re-run.
    * A **GMEOW data file** (.ttl): runs the per-profile CONSTRUCT (the FnO/EDOAL
      executor, lossy by design). With no source, the worked-example fixtures.
    """
    from gmeow_tools.projections import (
        GTS_VIEW_ALL,
        GTS_VIEW_GMEOW,
        PROFILES,
        project_examples,
        project_file,
        project_gts_subset,
    )

    src = source or (Path(data) if data else None)
    if src is None or src.suffix.lower() == ".ttl":
        tag_map = _repo_tag_map()
    elif src.suffix.lower() == ".gts":
        tag_map = _gts_tag_map(src)
    else:
        tag_map = _gts_tag_map(None)
    selector = _resolve_lang(lang, tag_map)

    if src is None:
        for path in project_examples(selector=selector):
            console.print(f"[green]✓[/green] {path.relative_to(path.parents[1])}")
        return

    if src.suffix.lower() == ".gts":
        valid = set(PROFILES) | {GTS_VIEW_GMEOW, *GTS_VIEW_ALL}
        if profile not in valid:
            raise _fail(f"unknown view: {profile} (vocab | gmeow | all | maximal)")
        path = project_gts_subset(src, profile, selector=selector)
        console.print(f"[green]✓[/green] {path.relative_to(path.parents[1])}")
        return

    names = list(PROFILES) if profile == "all" else [profile]
    for name in names:
        if name not in PROFILES:
            raise _fail(f"unknown profile: {name}")
        path = project_file(src, name, selector=selector)
        console.print(f"[green]✓[/green] {path.relative_to(path.parents[1])}")


@app.command()
def transform(
    abox: Path = typer.Argument(  # noqa: B008
        ...,
        help="Canonical GMEOW A-Box Turtle file, or '-' to read it from stdin.",
    ),
    out: Path | None = typer.Option(  # noqa: B008
        None,
        "-o",
        "--out",
        help="Output directory (default dist/transform/<stem>/).",
    ),
    profiles: str = typer.Option(
        "all",
        "--profiles",
        help="Projection profiles for P(G): all|name,name,…",
    ),
    diff_target: Path | None = typer.Option(  # noqa: B008
        None,
        "--diff-target",
        help="A parity-target Turtle file for the vocabulary-coverage diff.",
    ),
    report: Path | None = typer.Option(  # noqa: B008
        None,
        "--report",
        help="Write the coverage diff (Markdown) here instead of stdout.",
    ),
    lang: str | None = _lang_option(),
) -> None:
    """Transpile an A-Box to MAXIMAL(G) = G + E(G) + P(G) (#34).

    One fat multi-vocabulary file family: <stem>.gts (canonical, full RDF 1.2
    provenance audit trail), index.nq (RDF 1.2), index.ttl / index.jsonld
    (asserted base triples — plain-RDF readable). Saturation materializes
    STRONG equivalences only, gated by the alignment-direction lint;
    suppression (displayable false) is honored fail-closed. Reads the A-Box from
    stdin when <abox> is '-', so ``gmeow up-project src | gmeow transform -``
    streams the two halves.
    """
    from gmeow_rdf.compat.rdflib import Graph

    from gmeow_tools.transform import (
        TransformAbortedError,
        transform_graph,
        vocab_coverage,
    )
    from gmeow_tools.transform import transform as run_transform

    selector = _resolve_lang(lang, _repo_tag_map())

    names = None if profiles == "all" else [p.strip() for p in profiles.split(",")]
    try:
        if str(abox) == "-":
            graph, stem = _read_turtle(abox)
            result = transform_graph(
                graph, stem, out_dir=out, profiles=names, selector=selector
            )
        else:
            result = run_transform(abox, out_dir=out, profiles=names, selector=selector)
    except (TransformAbortedError, ValueError) as exc:
        raise _fail(f"✗ {exc}") from exc
    for path in result.written:
        console.print(f"[green]✓[/green] {path}")
    console.print(
        f"asserted {result.asserted} · saturated {result.saturated} · "
        f"projected {result.projected} · suppressed {result.suppressed_dropped} · "
        f"lint-denied cells {result.denied_cells} · "
        f"{result.wall_clock_s:.1f}s"
    )
    if diff_target is not None:
        index_ttl = next((p for p in result.written if p.name == "index.ttl"), None)
        if index_ttl is None:
            raise _fail("✗ transform output missing index.ttl")
        maximal = Graph().parse(index_ttl, format="turtle")
        target_graph = Graph().parse(diff_target, format="turtle")
        table = vocab_coverage(maximal, target_graph)
        if report is not None:
            report.write_text(table, encoding="utf-8")
            console.print(f"[green]✓[/green] coverage report → {report}")
        else:
            console.print(table)


@app.command(name="up-project")
def up_project_cmd(
    source: Path = typer.Argument(  # noqa: B008
        ...,
        help="A non-GMEOW source RDF file (Turtle), or '-' to read it from stdin.",
    ),
    out: Path | None = typer.Option(  # noqa: B008
        None, "-o", "--out", help="Write the GMEOW lift here (default: stdout Turtle)."
    ),
    descend: bool = typer.Option(
        False,
        "--descend",
        help="Use the context-aware graph-descent resolver (resolves a term by "
        "the subject's type) over the per-term floor.",
    ),
) -> None:
    """Lift a consumer-vocabulary RDF file UP into pure GMEOW (#451).

    Rewrites each term with a mechanically-invertible alignment rule to its GMEOW
    counterpart as a fact; a ``skos:closeMatch`` term is lifted as a provenance-
    stamped ``gmeow:StatementMetadata`` claim (confidence + mappedFrom) rather
    than a bare fact. Terms with no rule, or whose reverse is ambiguous (a
    many-to-one down-image), are reported and left out — never guessed.

    With ``--descend``, an ambiguous or inferred term is resolved by the
    subject's type — ``schema:about`` on a ``MediaObject`` becomes ``gmeow:depicts``
    but on any other entity ``gmeow:isAbout`` — falling through to the per-term
    floor when the type adds no signal. Reads from stdin and writes Turtle to
    stdout, so ``cat src | gmeow up-project - | gmeow transform -`` streams.
    """
    from gmeow_tools.up_projection import up_project
    from gmeow_tools.up_projection_descend import up_project_descend

    src, _stem = _read_turtle(source)
    try:
        result = up_project_descend(src) if descend else up_project(src)
    except ValueError as exc:
        raise _fail(str(exc)) from exc
    if out is not None:
        try:
            result.graph.serialize(destination=out, format="turtle")
        except OSError as exc:
            raise _fail(f"cannot write {out}: {exc}") from exc
        err_console.print(f"[green]wrote[/green] {out}")
    else:
        # raw Turtle on stdout (typer.echo, no Rich-markup mangling) so the
        # output pipes cleanly; all diagnostics go to stderr.
        typer.echo(result.graph.serialize(format="turtle"))
    err_console.print(
        f"[green]lifted[/green] {result.lifted} facts · "
        f"[cyan]claimed[/cyan] {result.claimed} inferred · "
        + (
            f"[magenta]context[/magenta] {result.context_resolved} by-type · "
            if descend
            else ""
        )
        + (
            f"[blue]bridged[/blue] {result.tag_resolved} QID-tag · "
            if result.tag_resolved
            else ""
        )
        + f"[yellow]gap[/yellow] {len(result.gap_terms)} terms · "
        f"[yellow]ambiguous[/yellow] {len(result.ambiguous_terms)} terms",
    )
    for term, n in sorted(result.claim_terms.items()):
        err_console.print(f"[cyan]claimed[/cyan] {term} (x{n})")
    for term, n in sorted(result.gap_terms.items()):
        err_console.print(f"[yellow]gap[/yellow] {term} (x{n})")
    for term, n in sorted(result.ambiguous_terms.items()):
        err_console.print(f"[yellow]ambiguous[/yellow] {term} (x{n})")


@app.command()
def acceptance(
    source: Path | None = typer.Argument(  # noqa: B008
        None,
        help="A real-world source RDF file to score; default: the vendored "
        "external/ snapshots (the un-gameable parity corpus).",
    ),
    out: Path | None = typer.Option(  # noqa: B008
        None, "-o", "--out", help="Write the Markdown scoreboard here (else stdout)."
    ),
    floor: bool = typer.Option(
        False,
        "--floor",
        help="Use the per-term floor instead of the context-aware descent.",
    ),
    min_recall: float | None = typer.Option(
        None,
        "--min-recall",
        help="HARD aggregate floor (#579): if the corpus-aggregate round-trip "
        "recall %% falls below this, fail with exit 1. Omit for report-only.",
    ),
) -> None:
    """Score the full transpile against real data — the honest scoreboard (#450).

    Runs every acceptance gate over each source: pure-GMEOW intermediate (hard),
    round-trip ⊇ source per vocabulary (scoreboard, red until done), size
    invariant (hard), external-validator (no x-gmeow leak hard; term-attestation
    and SHACL-from-vendored-axioms report-only), and the honest coverage report.
    The corpus is the verbatim ``external/`` snapshots — numbers that cannot be
    moved by writing fixtures.

    The per-file round-trip gate stays a scoreboard (red until done). Passing
    ``--min-recall`` adds a SEPARATE *aggregate* floor (#579): the pooled
    Σ recovered / Σ addressable recall across the whole corpus must clear it, or
    the command hard-fails — making the transpile gate block without demanding
    100%% per-file recall (honest-scoreboard doctrine preserved).
    """
    import gmeow_native.pipeline as _pipeline

    try:
        native = cast(
            "dict[str, Any]",
            _pipeline.acceptance(
                str(PROJECT_ROOT),
                None if source is None else str(source),
                not floor,
            ),
        )
    except (ImportError, OSError, RuntimeError, ValueError) as exc:
        raise _fail(str(exc)) from exc

    report = str(native["markdown"])
    if out is not None:
        out.write_text(report, encoding="utf-8")
        err_console.print(f"[green]wrote[/green] {out}")
    else:
        console.print(report, markup=False, highlight=False)
    results = cast("list[dict[str, Any]]", native.get("results", []))
    for fa in results:
        verdict = "[green]PASS[/green]" if fa.get("passed") else "[red]FAIL[/red]"
        err_console.print(f"{verdict} {fa.get('source', 'source')}")

    if min_recall is not None:
        aggregate = float(native.get("aggregate_recall", 100.0))
        if aggregate < min_recall:
            raise _fail(
                f"✗ corpus-aggregate round-trip recall {aggregate:.2f}% is below "
                f"the floor {min_recall:.2f}% ({len(results)} source(s))"
            )
        err_console.print(
            f"[green]✓[/green] corpus-aggregate round-trip recall "
            f"{aggregate:.2f}% ≥ floor {min_recall:.2f}%"
        )


@app.command()
def quality(
    foops_url: str = typer.Option(
        "", "--foops-url", help="Published ontology URL to assess with FOOPS!."
    ),
    strict: bool = typer.Option(
        False, "--strict", help="Fail if OOPS! or FOOPS! cannot be reached."
    ),
) -> None:
    """Run OOPS! (pitfalls) and optionally FOOPS! (FAIR) — network, best-effort."""
    from gmeow_tools import reason as reasoning
    from gmeow_tools.quality import run_foops, run_oops
    from gmeow_tools.runner import ToolUnavailableError

    try:
        merged = reasoning.merge_release()
    except ToolUnavailableError as exc:
        raise _fail(f"tool unavailable: {exc}", code=2) from exc

    try:
        report = run_oops(merged.read_text(encoding="utf-8"))
        console.print(f"[green]✓ OOPS! returned {len(report)} bytes[/green]")
    except (
        httpx.HTTPError
    ) as exc:  # network/service failure → raise if --strict, else skip
        if strict:
            raise _fail(f"OOPS! failed: {exc}") from exc
        err_console.print(f"[yellow]OOPS! skipped: {exc}[/yellow]")

    if foops_url:
        try:
            result = run_foops(foops_url)
            console.print(
                f"[green]✓ FOOPS! score {result.score:.2f} "
                f"({result.checks_passed}/{result.checks_total})[/green]"
            )
        except httpx.HTTPError as exc:
            if strict:
                raise _fail(f"FOOPS! failed: {exc}") from exc
            err_console.print(f"[yellow]FOOPS! skipped: {exc}[/yellow]")


_GTS_COMPILE_OUT = typer.Option(
    None, "--out", "-o", help="Output .gts path (default: generated/dist/gmeow.gts)."
)
_GTS_SIGN_KEY = typer.Option(
    None, "--sign-key", help="Armored Ed25519 OpenPGP secret key file."
)
_GTS_PUBLIC_KEY = typer.Option(
    None, "--public-key", help="Armored OpenPGP public key file to embed."
)


def _signed_gts_copy(
    source: Path,
    *,
    sign_key: Path,
    public_key: Path,
) -> tuple[bytes, str]:
    """Re-emit a folded GTS snapshot with release signatures.

    The Rust pipeline remains the build authority for the unsigned snapshot.
    Release signing is a packaging step over that freshly regenerated fold: read
    the pipeline product, embed the OpenPGP transport key in metadata, and write
    every frame through the GTS writer with the supplied signer.
    """
    try:
        secret_armor = sign_key.read_text(encoding="utf-8")
    except OSError as exc:
        raise _fail(f"cannot read --sign-key {sign_key}: {exc}") from exc
    try:
        public_key_armor = public_key.read_text(encoding="utf-8")
    except OSError as exc:
        raise _fail(f"cannot read --public-key {public_key}: {exc}") from exc
    try:
        signer = gts.Signer.from_gpg_secret_key(secret_armor)
    except Exception as exc:
        raise _fail(f"cannot load signer from {sign_key}: {exc}") from exc

    graph = _read_gts_or_fail(source)
    profile = graph.segment_profiles[-1] if graph.segment_profiles else "dist"
    writer = gts.Writer(profile=profile, signer=signer)

    meta = dict(sorted(graph.meta.items()))
    meta["gts:transportKey"] = {"kid": signer.kid, "gpg": public_key_armor}
    writer.add_meta(meta)

    if graph.terms:
        writer.add_terms(list(graph.terms))
    if graph.quads:
        writer.add_quads(list(graph.quads))
    if graph.reifiers:
        writer.add_reifies(dict(sorted(graph.reifiers.items())))
    if graph.annotations:
        writer.add_annot(list(graph.annotations))

    for digest in sorted(graph.blobs):
        blob_meta = graph.blob_meta.get(digest, {})
        mt = blob_meta.get("mt")
        rep = blob_meta.get("rep")
        writer.add_blob(
            graph.blobs[digest],
            mt=mt if isinstance(mt, str) else None,
            rep=rep if isinstance(rep, str) else None,
        )

    for suppression in graph.suppressions:
        writer.add_suppress(
            suppression.targets,
            reason=suppression.reason,
            by=suppression.by,
        )
    writer.add_index()
    return writer.to_bytes(), signer.kid


@app.command(name="compile-gts")
def compile_gts(
    out: Path | None = _GTS_COMPILE_OUT,
    sign_key: Path | None = _GTS_SIGN_KEY,
    public_key: Path | None = _GTS_PUBLIC_KEY,
) -> None:
    """Compile the statement-complete GTS dist snapshot (generated/gmeow.gts).

    The CLI face of the registered ``gts`` generator — the committed,
    drift-gated snapshot every exporter consumes (the narrow waist). With
    ``--out``, writes an ad-hoc copy of the identical bytes instead. With
    ``--sign-key`` and ``--public-key``, re-emits the freshly generated snapshot
    as a signed release package with the armored transport key embedded.
    """
    from gmeow_tools import config

    if (sign_key is None) != (public_key is None):
        raise _fail("--sign-key and --public-key must be supplied together")

    rdf12 = config.STATEMENT_RDF12_FILE
    if not rdf12.exists():
        raise _fail(
            f"RDF 1.2 statement artifact not found: {rdf12}\n"
            "run 'gmeow regenerate' first (a statement-less dist would drop "
            "confidence/standpoint/provenance)."
        )
    # The Rust pipeline (the build authority since #861 P7) folds the snapshot
    # at its single gts_sink; running it reproduces generated/dist/gmeow.gts.
    _regenerate_native()
    target = out or config.GTS_SNAPSHOT_FILE
    if sign_key is not None and public_key is not None:
        data, kid = _signed_gts_copy(
            config.GTS_SNAPSHOT_FILE, sign_key=sign_key, public_key=public_key
        )
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(data)
        console.print(f"[green]✓[/green] {target} ({len(data)} bytes)")
        console.print(f"[green]✓[/green] signed with kid {kid}")
        return

    if out is not None:
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(config.GTS_SNAPSHOT_FILE.read_bytes())
    size = target.stat().st_size
    console.print(f"[green]✓[/green] {target} ({size} bytes)")


@app.command(name="mcp")
def mcp_start() -> None:
    """Start the GMEOW MCP server (stdio transport).

    Exposes validation, compilation, reasoning, and term-lookup tools plus
    ontology resources to AI agents via the Model Context Protocol.
    """
    from gmeow_native import pipeline

    from gmeow_tools.config import GTS_SNAPSHOT_FILE

    pipeline.run_dev_mcp(GTS_SNAPSHOT_FILE.read_bytes(), str(PROJECT_ROOT))


@app.command(name="import-foundation")
def import_foundation(
    jsonl: Path = typer.Argument(  # noqa: B008
        ..., help="Foundation corpus JSONL (private; never committed)."
    ),
    out_dir: Path = typer.Option(  # noqa: B008
        Path("build/foundation"), "--out", help="Output directory."
    ),
    nq: Path | None = typer.Option(  # noqa: B008
        None, "--nq", help="Optional .nq form for reconciliation."
    ),
) -> None:
    """Import the foundation corpus (#364).

    Emits the graph, the budget report, and the six lossy projections
    (+ optional .nq reconciliation). Corpus-derived artifacts are external
    products, never repo artifacts (privacy).
    """
    import gmeow_native.foundation as _foundation

    out_dir.mkdir(parents=True, exist_ok=True)
    budget_text = _foundation.import_foundation(
        str(jsonl), str(out_dir), str(nq) if nq is not None else None
    )
    console.print(budget_text)
    console.print(f"[green]✓[/green] artifacts → {out_dir}")


@app.command()
def describe(
    term: str = typer.Argument(
        ..., help="A GMEOW term: gmeow:X, local name, or prefix."
    ),
    gts: Path | None = typer.Option(  # noqa: B008
        None, "--gts", help="Describe offline from a .gts package instead of the repo."
    ),
    lang: str | None = _lang_option(),
) -> None:
    """Describe a GMEOW term as useful prose (#325).

    Composes definition, stereotype, slice + tier, alignments, scope notes,
    examples, and the flat-first/reify-on-demand pairing. Works offline
    against any .gts file. Defaults to the repo graph when run inside the
    checkout; otherwise falls back to the bundled gmeow.gts.
    """
    from gmeow_tools.describe import describe as _describe

    if gts is not None:
        tag_map = _gts_tag_map(gts)
    else:
        from gmeow_tools.config import ONTOLOGY_FILE

        tag_map = _repo_tag_map() if ONTOLOGY_FILE.exists() else _gts_tag_map(None)
    selector = _resolve_lang(lang, tag_map)

    gts_path = gts
    if gts_path is None:
        from gmeow_tools.config import GTS_SNAPSHOT_FILE, ONTOLOGY_FILE

        if not ONTOLOGY_FILE.exists():
            gts_path = GTS_SNAPSHOT_FILE
    text, code = _describe(term, gts_path, selector=selector)
    console.print(text)
    if code:
        raise typer.Exit(code=code)


@app.command(name="extract-docs")
def extract_docs(
    gts_file: Path | None = typer.Argument(  # noqa: B008
        None,
        help="GTS file to project (default: bundled gmeow.gts).",
    ),
    directory: Path = typer.Option(  # noqa: B008
        ...,
        "--directory",
        "-d",
        help="Output directory for the docs tree.",
    ),
    force: bool = typer.Option(
        False,
        "--force",
        help="Write into a non-empty output directory.",
    ),
    lang: str | None = _lang_option(),
) -> None:
    """Extract the browsable docs tree from a GTS snapshot (#439).

    The tree is the full ontology-docs site (per-term reference pages, slice
    guides, alignment + linkage indexes), unpacked verbatim from the
    ``ontology-docs`` blob baked into the bundle. The site is rendered natively
    at ``regenerate gts`` time (``gmeow_docs::render_site_lang``) and embedded,
    not re-projected here; run ``regenerate gts`` to refresh the stored tree.
    """
    from gmeow_tools.config import GTS_SNAPSHOT_FILE
    from gmeow_tools.gts_views import extract_docs_site, load_fold

    path = gts_file or GTS_SNAPSHOT_FILE
    view = load_fold(path)
    selector = _resolve_lang(lang, view.tag_map())
    try:
        extract_docs_site(view, directory, selector=selector, force=force)
    except FileExistsError as exc:
        raise _fail(str(exc)) from exc
    except (OSError, ValueError) as exc:
        raise _fail(f"cannot create docs tree: {exc}") from exc
    console.print(f"[green]✓[/green] docs tree → {directory}")


logic_app = typer.Typer(
    name="logic",
    help="Logic compiler: logic: source → IR → generated artifacts.",
    no_args_is_help=True,
)
app.add_typer(logic_app, name="logic")

_LOGIC_MODES = (
    "owl-dl",
    "owl-el",
    "datalog",
    "n3",
    "gufo",
    "canonical-rdf12",
    "report",
)


@logic_app.command("query")
def logic_query(
    world: Path = typer.Argument(  # noqa: B008
        ...,
        help="N-Quads file of the materialized world(s) — the read-only EDB.",
    ),
    query_file: Path = typer.Argument(  # noqa: B008
        ...,
        help="A .logic query: prefixes, Horn rules, optional cut, one `?- goal.`",
    ),
    profile: str = typer.Option(
        "PositiveHornProfile",
        "--profile",
        help="Semantic profile in force. Cut (`!`) is permitted ONLY under "
        "ProceduralPrologProfile.",
    ),
    world_iri: str | None = typer.Option(
        None,
        "--world-iri",
        help="Target world IRI. Default: the single named graph in the N-Quads.",
    ),
    max_answers: int | None = typer.Option(
        None,
        "--max-answers",
        min=0,
        help="Cap the answer set (status=partial when the cap is hit).",
    ),
    max_steps: int | None = typer.Option(
        None,
        "--max-steps",
        min=0,
        help="Inference-count ceiling (status=exhausted when exceeded).",
    ),
    as_json: bool = typer.Option(
        False,
        "--json",
        help="Emit the raw {bindings, status} JSON instead of a table.",
    ),
) -> None:
    """Resolve a backward goal (`.logic`) over a materialized world (issue #504, v4).

    Loads the N-Quads EDB, parses the `.logic` program, enforces the cut/profile
    gate, and routes the goal through the dispatcher — the oxigraph SPARQL fast
    path for non-recursive pattern goals, or embedded Scryer Prolog (with
    tabling) for recursive/unification-heavy goals. Answers are **virtual**:
    nothing is written back into the world (cut is operational-only, never a
    stored fact).
    """
    try:
        import gmeow_logic
    except ImportError as exc:  # pragma: no cover - environment guard
        raise _fail(
            "✗ gmeow_logic extension not built — run `make native-py` "
            f"(maturin develop). Underlying error: {exc}"
        ) from exc

    if not world.is_file():
        raise _fail(f"✗ world N-Quads file not found: {world}")
    if not query_file.is_file():
        raise _fail(f"✗ query file not found: {query_file}")

    nquads = world.read_text(encoding="utf-8")
    program = query_file.read_text(encoding="utf-8")

    try:
        result = gmeow_logic.query(
            nquads, program, profile, world_iri, max_answers, max_steps
        )
    except (ValueError, OverflowError) as exc:
        # Cut outside ProceduralPrologProfile, malformed input, ambiguous world,
        # a Scryer resolution error, or a budget value too large to convert —
        # all surface as a hard failure.
        raise _fail(f"✗ query failed: {exc}") from exc

    if as_json:
        import json

        console.print(json.dumps(result, sort_keys=True, ensure_ascii=False))
        return

    bindings = result["bindings"]
    status = result["status"]
    if not bindings:
        console.print("[yellow]no answers[/yellow]")
    else:
        for row in bindings:
            rendered = ", ".join(f"{k} = {v}" for k, v in sorted(row.items()))
            console.print(rendered if rendered else "(true)")
    console.print(f"[dim]{len(bindings)} answer(s); status={status}[/dim]")


@logic_app.command("compile")
def logic_compile(
    check: bool = typer.Option(
        False,
        "--check",
        help=(
            "Drift-check the committed artifacts without writing "
            "(exit non-zero on drift)."
        ),
    ),
    mode: str | None = typer.Option(
        None,
        "--mode",
        help=(
            "Emit / inspect only the named back-end: "
            + "|".join(_LOGIC_MODES)
            + " (default: all 7 outputs)."
        ),
    ),
) -> None:
    """Compile logic: vocabulary → IR → canonical artifact + projections.

    Without flags: renders all 7 artifacts to their committed paths under
    ``generated/``.  With ``--check``: proves committed artifacts are not
    drifted (same as ``gmeow check-generated logic``) without writing.
    With ``--mode``: restricts the render or check to a single back-end.

    The overclaim gate blocks any emit that claims ExactPreservation while
    dropping content (CONSTITUTION Principle 7 / LOGIC-CONFORMANCE.md).
    """
    import gmeow_logic

    from gmeow_tools.config import PROJECT_ROOT as _PROJECT_ROOT
    from gmeow_tools.logic_compile import (
        LOGIC_DATALOG_FILE,
        LOGIC_GUFO_FILE,
        LOGIC_N3_FILE,
        LOGIC_OWL_DL_FILE,
        LOGIC_OWL_EL_FILE,
        LOGIC_RDF12_FILE,
        LOGIC_REPORT_FILE,
        LOGIC_SOURCE_FILE,
    )

    if mode is not None and mode not in _LOGIC_MODES:
        raise _fail(f"✗ unknown --mode {mode!r} (valid: {', '.join(_LOGIC_MODES)})")

    # --check with no --mode: drift-gate via the Rust pipeline (the build
    # authority since #861 P7). The pipeline reproduces every committed
    # artifact, the logic ones included, and reports any drift.
    if check and mode is None:
        report = _run_pipeline(check=True)
        logic_drift = [d for d in report.get("drifted", []) if "/logic/" in d]
        if logic_drift:
            for rel in sorted(logic_drift):
                err_console.print(f"[red]drift[/red] {rel}")
            raise _fail(
                f"✗ {len(logic_drift)} logic artifact(s) out of date — "
                "run `gmeow logic compile`"
            )
        console.print(
            "[green]✓ logic: committed artifacts match source (no drift)[/green]"
        )
        return

    # --mode only (no --check or with --check): compile (in Rust) and emit /
    # inspect one back-end.  The whole frontend → IR → projections + report
    # pipeline runs in ``gmeow_logic.compile_logic`` (#664/#727); this command
    # selects one artifact from its result dict.
    if mode is not None:
        _mode_to_file = {
            "owl-dl": LOGIC_OWL_DL_FILE,
            "owl-el": LOGIC_OWL_EL_FILE,
            "datalog": LOGIC_DATALOG_FILE,
            "n3": LOGIC_N3_FILE,
            "gufo": LOGIC_GUFO_FILE,
            "canonical-rdf12": LOGIC_RDF12_FILE,
            "report": LOGIC_REPORT_FILE,
        }
        # Projection target short-name → compile_logic dict key.
        _mode_to_key = {
            "owl-dl": "owl_dl",
            "owl-el": "owl_el",
            "datalog": "datalog",
            "n3": "n3",
            "gufo": "gufo",
            "canonical-rdf12": "canonical_rdf12",
            "report": "report",
        }

        if not LOGIC_SOURCE_FILE.exists():
            raise _fail(
                f"✗ logic: source not found: {LOGIC_SOURCE_FILE}\n"
                "Is the repo checkout complete?"
            )

        source_ttl = LOGIC_SOURCE_FILE.read_text(encoding="utf-8")
        try:
            compiled = gmeow_logic.compile_logic(source_ttl)
        except (ValueError, RuntimeError) as exc:
            raise _fail(f"✗ logic: compile failed: {exc}") from exc

        # Parse diagnostics now arrive as a native ``gmeow_diagnostics`` Report
        # (#856); each finding dict carries the canonical ``logic-compile.<code>``.
        for diag in compiled["diagnostics_report"].findings:
            err_console.print(
                f"[yellow]{diag['severity']}[/yellow] "
                f"[{diag['code']}] {diag['message']}"
            )

        target_file = _mode_to_file[mode]
        # Dynamic-key read off the TypedDict result: cast to a plain str-keyed
        # mapping (the key comes from the validated _mode_to_key table).
        _artifacts = cast("dict[str, object]", compiled)
        content = str(_artifacts[_mode_to_key[mode]])
        _sfx = ".ttl" if mode not in ("datalog", "n3") else f".{mode}"

        if check:
            import tempfile

            with tempfile.NamedTemporaryFile(suffix=_sfx, delete=False) as tf:
                tmp_path = Path(tf.name)
            tmp_path.write_text(content, encoding="utf-8")
            from gmeow_tools.genlib import rdf_compare

            drifts = rdf_compare(tmp_path, target_file)
            tmp_path.unlink(missing_ok=True)
            if drifts:
                for d in drifts:
                    err_console.print(f"[red]drift[/red] {d}")
                raise _fail(f"✗ --mode {mode}: committed artifact drifted")
            console.print(f"[green]✓ --mode {mode}: no drift[/green]")
        else:
            target_file.parent.mkdir(parents=True, exist_ok=True)
            target_file.write_text(content, encoding="utf-8")
            _rel = target_file.relative_to(_PROJECT_ROOT)
            console.print(f"[green]✓[/green] {_rel}")
        return

    # Default: full render via the Rust pipeline (the build authority). It
    # reproduces every committed artifact single-pass, the 7 logic outputs
    # included.
    _regenerate_native()
    console.print("[green]✓ logic: artifacts compiled[/green]")


@app.command()
def certify(
    input_path: Path = typer.Argument(  # noqa: B008
        ...,
        help=(
            "Path to an input.logic.ttl to statically certify against its "
            "declared semantic profile."
        ),
    ),
    profile: str | None = typer.Option(
        None,
        "--profile",
        help=(
            "Override the declared semantic profile localname (e.g. "
            "PositiveHornProfile, StratifiedNAFProfile). When omitted, read "
            "from a sibling profile.json (reasoning_contract.preset), else default "
            "PositiveHornProfile."
        ),
    ),
) -> None:
    """Statically certify a logic program against its declared profile.

    This is the standalone build-error surface for the logic-profile / decidability
    certifier — the analogue of ``reasoning_lint`` for the IR.  It parses the
    program, runs the native ``gmeow_logic.certify`` certifier (Rust-authoritative
    since #497/#651), prints
    every self-documenting violation string to stderr, and exits non-zero when
    any violation is found (zero when certified clean).  Mirror of how
    ``reasoning_lint`` fails the build under ``make check``.

    The profile is resolved (highest precedence first):

    1. the ``--profile`` override, if given;
    2. ``reasoning_contract.preset`` in a sibling ``profile.json``, if present;
    3. ``PositiveHornProfile`` (the v1 default).
    """
    #: The six logic:ReasoningPreset local names (mirrors the Rust
    #: SemanticProfileId enum / the ontology's named individuals).
    _valid_profiles = {
        "PositiveHornProfile",
        "StratifiedNAFProfile",
        "WellFoundedProfile",
        "StableModelProfile",
        "ProceduralPrologProfile",
        "ProbabilisticProfile",
    }

    if not input_path.is_file():
        raise _fail(f"✗ certify: input not found: {input_path}")
    input_path = input_path.resolve()

    # Resolve the declared profile: --profile > sibling profile.json > default.
    if profile is not None:
        profile_str = profile
    else:
        sibling = input_path.parent / "profile.json"
        profile_str = "PositiveHornProfile"
        if sibling.is_file():
            import json

            try:
                sibling_data = json.loads(sibling.read_text(encoding="utf-8"))
            except (OSError, json.JSONDecodeError) as exc:
                raise _fail(f"✗ certify: cannot read {sibling}: {exc}") from exc
            if isinstance(sibling_data, dict) and "reasoning_contract" in sibling_data:
                # #767: the contract preset lives under reasoning_contract.preset
                # (the retired top-level semantic_profile key is gone).
                #
                # Mirror the Rust authority's hard-fail (#767, Gap 5): a PRESENT
                # reasoning_contract that is not an object with a string `preset` is
                # malformed and must hard-fail, never silently fall through to the
                # default preset.  Absence stays the PositiveHornProfile fallback.
                contract = sibling_data["reasoning_contract"]
                if (
                    not isinstance(contract, dict)
                    or "preset" not in contract
                    or not isinstance(contract["preset"], str)
                ):
                    raise _fail(
                        f"✗ certify: malformed reasoning_contract in {sibling}: "
                        "expected an object with a string 'preset'"
                    )
                profile_str = contract["preset"]

    if profile_str not in _valid_profiles:
        raise _fail(
            f"✗ certify: unknown profile {profile_str!r}; must be one of "
            f"{sorted(_valid_profiles)}"
        )

    # Rust-authoritative certification (#497/#664): the whole compile pipeline and
    # the certifier run in Rust.  ``compile_logic`` returns the ``nemo_rules`` rule
    # text the certifier consumes (the Python compiler was deleted in #727).
    try:
        import gmeow_logic
    except ImportError as exc:
        raise _fail(
            "✗ certify: gmeow_logic native extension is not installed "
            "(certification is Rust-authoritative since #497) — run 'make native-py'."
        ) from exc
    try:
        source_ttl = input_path.read_text(encoding="utf-8")
    except OSError as exc:
        raise _fail(f"✗ certify: cannot read {input_path}: {exc}") from exc
    try:
        compiled = gmeow_logic.compile_logic(source_ttl)
    except (ValueError, RuntimeError) as exc:
        raise _fail(f"✗ certify: cannot compile {input_path}: {exc}") from exc
    rules_only = str(compiled["nemo_rules"])
    try:
        verdict = gmeow_logic.certify(rules_only, profile_str)
    except (ValueError, RuntimeError) as exc:
        raise _fail(f"✗ certify: native certifier failed: {exc}") from exc
    violations = list(verdict["violations"])
    if violations:
        err_console.print(
            f"[red]✗ certify: {len(violations)} violation(s) for "
            f"{profile_str} in {input_path.name}[/red]"
        )
        for v in violations:
            err_console.print(f"[red]  {v}[/red]")
        raise typer.Exit(code=1)

    console.print(
        f"[green]✓ certify: {input_path.name} is certified for {profile_str}[/green]"
    )


i18n_app = typer.Typer(help="Internationalization commands.", no_args_is_help=True)
app.add_typer(i18n_app, name="i18n")


@i18n_app.command(name="extract")
def extract_catalog(
    root: Path = typer.Option(  # noqa: B008
        PROJECT_ROOT,
        "--root",
        help="Repository root containing the slices/ directory.",
    ),
    output_dir: Path = typer.Option(  # noqa: B008
        PROJECT_ROOT / "dist" / "i18n",
        "--output-dir",
        "-o",
        help="Directory to write the generated POT/PO files.",
    ),
    lang: str | None = typer.Option(
        None,
        "--lang",
        "-l",
        help="If given, write .po files for this language instead of .pot templates.",
    ),
    terms_only: bool = typer.Option(
        False,
        "--terms-only",
        help="Only extract ontology term strings, skip Markdown docs and templates.",
    ),
) -> None:
    """Extract translatable ontology strings into gettext catalogs.

    Walks the merged ontology graph, groups translatable strings by owning
    slice, and emits one POT (or PO when --lang is given) file per slice.
    When --terms-only is not given, also extracts slice guides, project docs,
    README.md, and ontology-docs template strings.

    Args:
        root: Repository root containing the slices/ directory.
        output_dir: Directory to write the generated POT/PO files.
        lang: If given, write .po files for this language instead of .pot templates.
        terms_only: Only extract ontology term strings, skip Markdown docs and
            templates.
    """
    import gmeow_docs

    report = gmeow_docs.i18n_extract(str(root), str(output_dir), lang, terms_only)

    console.print(
        f"[green]✓[/green] wrote {report['groups']} term catalog(s) "
        f"({report['total_keys']} keys) to {output_dir}"
    )


@i18n_app.command(name="sync-english")
def sync_english(
    root: Path = typer.Option(  # noqa: B008
        PROJECT_ROOT,
        "--root",
        help="Repository root to search for slices.",
    ),
    dry_run: bool = typer.Option(
        False,
        "--dry-run",
        help="Report only; do not write changes.",
    ),
) -> None:
    """Synchronize English translations from PO catalogs back to canonical sources.

    Discovers ``en.po`` and ``*.md.po`` files under ``<root>/slices/**/i18n/``,
    maps them to their canonical masters, and applies a 3-way merge.  ``en.po``
    catalogs update sibling ``module.ttl`` and ``manifest.ttl`` files;
    ``*.md.po`` catalogs update the matching ``*.md`` file in the same slice.

    Args:
        root: Repository root to search for slices.
        dry_run: Report only; do not write changes.
    """
    import gmeow_docs

    po_files = sorted(root.glob("slices/**/i18n/*.po"))
    changed_files: list[Path] = []
    conflicts: list[str] = []
    skipped: list[str] = []
    unchanged = 0
    processed = 0

    for po_path in po_files:
        slice_dir = po_path.parent.parent
        source_paths: list[Path] = []

        if po_path.name == "en.po":
            source_paths = [
                slice_dir / "module.ttl",
                slice_dir / "manifest.ttl",
            ]
        elif po_path.name.endswith(".md.po"):
            md_name = po_path.name[:-3]  # strip ".po" -> e.g. "docs.md"
            source_paths = [slice_dir / md_name]
        else:
            continue

        for source_path in source_paths:
            if not source_path.is_file():
                continue
            report = gmeow_docs.i18n_sync_english_file(
                str(po_path), str(source_path), dry_run
            )
            processed += 1
            changed_files.extend(Path(path) for path in report["changed_files"])
            conflicts.extend(report["conflicts"])
            skipped.extend(report["skipped"])
            unchanged += len(report["unchanged"])

    def _rel(path: Path) -> Path:
        return (
            path.relative_to(PROJECT_ROOT)
            if path.is_relative_to(PROJECT_ROOT)
            else path
        )

    for path in sorted(set(changed_files)):
        status = "would change" if dry_run else "changed"
        console.print(f"[green]{status}[/green] {_rel(path)}")
    for conflict in conflicts:
        err_console.print(f"[red]conflict[/red] {conflict}")
    for skip in skipped:
        err_console.print(f"[yellow]skip[/yellow] {skip}")

    if conflicts:
        raise _fail(
            f"✗ {len(conflicts)} conflict(s), {len(changed_files)} file(s) "
            f"changed, {unchanged} unchanged, {len(skipped)} skipped"
        )

    mode_note = " (dry run)" if dry_run else ""
    console.print(
        f"[green]✓{mode_note}[/green] {processed} source(s) synced: "
        f"{len(changed_files)} changed, {len(conflicts)} conflicts, "
        f"{len(skipped)} skipped, {unchanged} unchanged"
    )


@i18n_app.command(name="merge")
def merge(
    root: Path = typer.Option(  # noqa: B008
        PROJECT_ROOT,
        "--root",
        help="Repository root to search for slices.",
    ),
    output: Path | None = typer.Option(  # noqa: B008
        None,
        "--output",
        "-o",
        help="Output Turtle file. Defaults to stdout.",
    ),
    lang: str | None = typer.Option(
        None,
        "--lang",
        help="BCP-47 language tag to merge (e.g. 'fr'). Defaults to all languages.",
    ),
) -> None:
    """Merge committed PO translations into a multilingual RDF graph.

    Discovers ``*.po`` files under ``<root>/slices/*/*/i18n/`` and adds their
    translated triples to the merged English ontology graph. The result is a
    single Turtle graph carrying language-tagged labels, definitions, and
    comments without modifying canonical ``.ttl`` or ``.md`` sources.

    Args:
        root: Repository root to search for slices.
        output: Output Turtle file. Defaults to stdout.
        lang: BCP-47 language tag to merge (e.g. 'fr'). Defaults to all languages.
    """
    import gmeow_docs

    report = gmeow_docs.i18n_merge(
        str(root), str(output) if output is not None else None, lang
    )
    if output is None:
        console.print(report["turtle"], end="")

    err_console.print(
        f"[green]✓ merged {report['po_files']} PO file(s), "
        f"{report['added']} translated triple(s) added "
        f"→ {report['output_note']}[/green]"
    )


@i18n_app.command(name="export-csv")
def export_csv(
    root: Path = typer.Option(  # noqa: B008
        PROJECT_ROOT,
        "--root",
        help="Repository root to search for slices.",
    ),
    output: Path | None = typer.Option(  # noqa: B008
        None,
        "--output",
        "-o",
        help="Output CSV file (default: stdout).",
    ),
) -> None:
    """Export translated PO catalogs to a flat CSV file.

    Discovers ``slices/*/*/i18n/*.po`` files, parses each entry's fuzzy flag,
    and emits one row per translatable term/predicate with the slice name,
    language, source string, and translation.

    Args:
        root: Repository root to search for slices.
        output: Output CSV file (default: stdout).
    """
    import gmeow_docs

    text = gmeow_docs.i18n_export_csv(str(root), str(output) if output else None)
    if output is None:
        console.print(text, end="")


@i18n_app.command(name="export-xliff")
def export_xliff(
    root: Path = typer.Option(  # noqa: B008
        PROJECT_ROOT,
        "--root",
        help="Repository root to search for slices.",
    ),
    output: Path | None = typer.Option(  # noqa: B008
        None,
        "--output",
        "-o",
        help="Output XLIFF 1.2 file (default: stdout).",
    ),
) -> None:
    """Export translated PO catalogs to an XLIFF 1.2 file.

    Discovers ``slices/*/*/i18n/*.po`` files and emits one XLIFF ``<file>`` per
    slice/language, with ``<trans-unit>`` elements keyed by
    ``term_iri|predicate``.

    Args:
        root: Repository root to search for slices.
        output: Output XLIFF 1.2 file (default: stdout).
    """
    import gmeow_docs

    text = gmeow_docs.i18n_export_xliff(str(root), str(output) if output else None)
    if output is None:
        console.print(text, end="")


@app.command(name="slice-fix-deps")
def slice_fix_deps(
    apply: bool = typer.Option(
        False,
        "--apply",
        help="Apply the proposed changes in-place (default: print patch only).",
    ),
    slices_dir: Path = typer.Option(  # noqa: B008
        None,
        "--slices-dir",
        help="Path to the slices/ directory (default: PROJECT_ROOT/slices).",
    ),
) -> None:
    """Propose manifest dependency edits as a reviewable unified diff.

    Computes undeclared/stale gmeow:sliceDependsOn entries by running the
    native ownership analyzer, then emits a unified diff for each affected
    manifest.ttl.

    By default: prints the patch to stdout, writes nothing.
    With --apply: writes the patched files in-place.

    The analysis result (gmeow:graph/slice-analysis) is NEVER written into
    authored manifests — only gmeow:sliceDependsOn additions/removals.
    """
    from gmeow_tools.slice_fix_deps import compute_fix_deps

    root = slices_dir or (PROJECT_ROOT / "slices")
    if not root.is_dir():
        raise _fail(f"slices directory not found: {root}")

    try:
        diffs = compute_fix_deps(root, apply=apply)
    except RuntimeError as exc:
        raise _fail(str(exc)) from exc

    if not diffs:
        console.print("[green]✓[/green] No dependency changes needed.")
        return

    for diff in diffs:
        console.print(diff, highlight=False)

    if apply:
        console.print(f"[green]✓[/green] Applied {len(diffs)} manifest patch(es).")
    else:
        console.print(
            f"[yellow]→[/yellow] {len(diffs)} manifest(s) need changes. "
            "Run with --apply to apply."
        )


if __name__ == "__main__":  # pragma: no cover
    app()
