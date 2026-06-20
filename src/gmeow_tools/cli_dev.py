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
from typing import TYPE_CHECKING, Any

import gts
import httpx
import typer
from rich.console import Console

from gmeow_tools import __version__
from gmeow_tools.config import PROJECT_ROOT
from gmeow_tools.projections import PROFILES as _PROFILES
from gmeow_tools.slices import Slice

if TYPE_CHECKING:
    from rdflib import Graph

    from gmeow_tools.language_tags import LangSelector

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

    from rdflib import Graph

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
    names: list[str] | None = typer.Argument(  # noqa: B008
        None,
        help="Generator names to run (default: all in dependency order).",
    ),
    jobs: int | None = typer.Option(
        None,
        "-j",
        "--jobs",
        help="Number of parallel workers (default: capped CPU count).",
    ),
    skip_unchanged: bool | None = typer.Option(
        None,
        "--skip-unchanged/--no-skip-unchanged",
        help=(
            "Skip generators whose inputs and implementation have not changed."
            " Defaults to True when running all generators, False when a"
            " specific generator is named."
        ),
    ),
) -> None:
    """Rebuild all checked-in generated artifacts from canonical sources.

    Runs every registered generator in topologically sorted order, or the
    named generators in the order given.
    """
    # Import all generator modules to trigger @register side effects.
    from gmeow_tools.config import PROJECT_ROOT
    from gmeow_tools.generator import regenerate as _regenerate
    from gmeow_tools.load_generators import load_all

    load_all()

    effective_skip = skip_unchanged if skip_unchanged is not None else (names is None)
    results = _regenerate(names or None, jobs=jobs, skip_unchanged=effective_skip)
    for _name, report in results.items():
        if report.skipped:
            console.print(f"[blue]⏵[/blue] {_name} skipped (unchanged)")
        for path in report.written:
            console.print(f"[green]✓[/green] {path.relative_to(PROJECT_ROOT)}")
        if report.orphans:
            for orphan in report.orphans:
                err_console.print(f"[yellow]orphan[/yellow] {orphan}")
    console.print(f"[green]✓ regenerated {len(results)} generator(s)[/green]")


@app.command()
def check_generated(
    names: list[str] | None = typer.Argument(  # noqa: B008
        None,
        help="Generator names to check (default: all).",
    ),
    skip: list[str] | None = typer.Option(  # noqa: B008
        None,
        "--skip",
        help=(
            "Generator names to exclude (e.g. --skip statements in a CI job "
            "without the Docker/Jena toolchain). A NEW generator is always "
            "included by default — exclusion is explicit, never wiring lag."
        ),
    ),
    jobs: int | None = typer.Option(
        None,
        "-j",
        "--jobs",
        help="Number of parallel workers (default: capped CPU count).",
    ),
    skip_unchanged: bool | None = typer.Option(
        None,
        "--skip-unchanged/--no-skip-unchanged",
        help=(
            "Skip generators whose inputs and implementation have not changed."
            " Defaults to True when checking all generators, False when a"
            " specific generator is named."
        ),
    ),
) -> None:
    """Drift + orphan check for every registered generator.

    Runs ``--check`` mode for all registered generators (or the named ones)
    and exits non-zero if any drift or orphans are found.
    """
    # Import all generator modules to trigger @register side effects.
    from gmeow_tools.generator import check_all, registry
    from gmeow_tools.load_generators import load_all

    load_all()

    selected = names or None
    if skip:
        unknown = sorted(set(skip) - set(registry()))
        if unknown:
            raise _fail(f"✗ --skip names not in the registry: {', '.join(unknown)}")
        selected = sorted(set(selected or registry()) - set(skip))
    effective_skip = skip_unchanged if skip_unchanged is not None else (names is None)
    results = check_all(selected, jobs=jobs, skip_unchanged=effective_skip)
    total_drift = 0
    total_orphans = 0
    for name, report in results.items():
        if report.skipped:
            console.print(f"[blue]⏵[/blue] {name} skipped (unchanged)")
        if report.drifted:
            total_drift += len(report.drifted)
            for rel in sorted(report.drifted):
                err_console.print(f"[red]drift[/red] {name}: {rel}")
        if report.orphans:
            total_orphans += len(report.orphans)
            for rel in sorted(report.orphans):
                err_console.print(f"[yellow]orphan[/yellow] {name}: {rel}")

    if total_drift or total_orphans:
        raise _fail(
            f"✗ {total_drift} drifted, {total_orphans} orphaned — "
            "run `gmeow regenerate`"
        )
    console.print(
        f"[green]✓ all {len(results)} generator(s) match committed sources "
        "(no drift, no orphans)[/green]"
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
        timings=timings, gts_input=gts, signature_config=signature_config
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


def _surface_reports() -> list[tuple[str, Callable[[], Any]]]:
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

    def _alignment() -> Any:
        from gmeow_tools import alignment_lint

        findings = alignment_lint.lint_alignment_directions(allow_network=False)
        findings += alignment_lint.lint_dc_refinement()
        return alignment_lint.to_diagnostics_report(findings)

    def _coverage() -> Any:
        from gmeow_tools import coverage

        return coverage.to_diagnostics_report(coverage.run_coverage())

    def _acceptance() -> Any:
        from gmeow_tools import acceptance

        results = [acceptance.run_acceptance(p) for p in acceptance.default_corpus()]
        return acceptance.to_diagnostics_report(results)

    def _wikidata() -> Any:
        from gmeow_tools import wikidata
        from gmeow_tools.mappings import collect_wikidata_ids, load_mappings

        report = wikidata.check_syntax(collect_wikidata_ids(load_mappings()))
        return wikidata.to_diagnostics_report(report)

    def _constitution() -> Any:
        from gmeow_tools import constitution

        return constitution.to_diagnostics_report(constitution.check_constitution())

    def _box_roles() -> Any:
        from gmeow_tools import box_roles

        return box_roles.to_diagnostics_report(box_roles.audit_box_roles())

    def _audit() -> Any:
        from gmeow_tools import audit
        from gmeow_tools.config import FIXTURES_DIR

        corpus = FIXTURES_DIR / "hallucination-kg.ttl"
        return audit.to_diagnostics_report(audit.audit_graph([corpus]))

    def _generated() -> Any:
        from gmeow_tools import generator

        return generator.to_diagnostics_report(
            generator.check_all(skip_unchanged=False)
        )

    def _classic_cross_check() -> Any:
        # The native↔oracle (ELK/HermiT/ROBOT) divergence ledger is already a
        # Rust-backed DiagnosticsReport (gmeow_logic.build_divergence_ledger →
        # classic_cross_check.build_report). Folding it carries the classic-oracle
        # cross-check findings into the bundle. Guarded: it needs the Docker/Java
        # lane, so on a Docker-less host the fold loop records a visible skip.
        from gmeow_tools import classic_cross_check as crosscheck

        _passed, _ledger, report = crosscheck.run()
        return report

    return [
        ("alignment", _alignment),
        ("coverage", _coverage),
        ("acceptance", _acceptance),
        ("wikidata", _wikidata),
        ("constitution", _constitution),
        ("box-roles", _box_roles),
        ("audit", _audit),
        ("generated", _generated),
        ("classic-cross-check", _classic_cross_check),
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
    from gmeow_tools.constitution import check_constitution

    result = check_constitution()
    for warning in result.warnings:
        err_console.print(f"[yellow]warning[/yellow] {warning}")
    for error in result.errors:
        err_console.print(f"[red]error[/red] {error}")
    if result.ok:
        console.print("[green]✓ constitution check passed[/green]")
    else:
        raise _fail(f"✗ {len(result.errors)} error(s)")


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
    from gmeow_tools.audit import audit_graph, render_json, render_text

    report = audit_graph(list(files))
    if json_out:
        console.print(render_json(report))
    else:
        console.print(render_text(report))
    if report.shacl_errors:
        raise _fail(f"✗ {len(report.shacl_errors)} SHACL error(s)")
    if strict and report.flagged:
        raise _fail(f"✗ {report.flagged} flagged claim(s) (--strict)")


evals_app = typer.Typer(
    help="Claim-extraction eval suite (#298).", no_args_is_help=True
)
app.add_typer(evals_app, name="evals")


@evals_app.command(name="score")
def evals_score() -> None:
    """Score every committed emission against the published contract (offline)."""
    from gmeow_tools.evals import all_scorecards

    for card in all_scorecards():
        console.print(
            f"[bold]{card.model}[/bold] overall {card.overall:.2f} "
            f"({card.valid}/{card.emitted} valid)"
        )
        for name, value in sorted(card.scores.items()):
            console.print(f"  {name}: {value:.2f}")


@evals_app.command(name="run")
def evals_run(
    model: str = typer.Option(..., "--model", help="Model identifier to send."),
    endpoint: str = typer.Option(..., "--endpoint", help="API endpoint URL."),
    api: str = typer.Option("openai", "--api", help="openai | anthropic."),
) -> None:
    """Call a model API over the corpus (network; keys from env)."""
    from gmeow_tools.evals import run_model

    if api not in ("openai", "anthropic"):
        raise _fail(f"✗ unsupported --api {api!r} (openai | anthropic)")
    try:
        out = run_model(model=model, endpoint=endpoint, api=api)
    except httpx.HTTPError as exc:
        raise _fail(f"✗ model API call failed: {exc}") from exc
    console.print(
        f"[green]✓ emission written to {out} — run `gmeow regenerate evals`[/green]"
    )


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
    """
    from gmeow_tools.engine_crosscheck import crosscheck_all

    results = crosscheck_all()
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

    The FINAL, ENFORCING step of ``make classic-cross-check`` (the sole Docker/Java
    surface, Principle 18). It reasons the bundle natively (authority), runs the
    classic ELK + HermiT oracles (timing each), calls the authoritative Rust
    comparator, writes the agreement matrix + per-tool timing as SARIF/JSON, and
    fails NON-ZERO on any real divergence (``NativeOnly`` / ``OracleOnly``).
    ``DlGap`` is the only honest-expected, non-failing class. NEVER part of
    ``make check`` or the required ``quality`` gate.
    """
    from gmeow_tools import classic_cross_check as crosscheck
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
            f"[green]✓ native ≡ oracle (ELK/HermiT); {ledger['dl_gap']} honest "
            "DL gap(s) — enforced cross-check passed[/green]"
        )
        return
    for row in ledger["rows"]:
        if row["kind"] in ("NativeOnly", "OracleOnly"):
            err_console.print(f"[red]{row['kind']}[/red] {row['detail']}")
    raise _fail(
        f"✗ native↔oracle divergence: {ledger['native_only']} native-only + "
        f"{ledger['oracle_only']} oracle-only row(s)"
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
    from gmeow_tools import rl_agreement

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
    from rdflib import Literal, URIRef
    from rdflib.namespace import XSD
    from rdflib.util import guess_format

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
    from gmeow_tools.alignment_lint import Severity, lint_alignment_directions

    findings = lint_alignment_directions(allow_network=network)
    errors = [f for f in findings if f.severity is Severity.ERROR]
    warnings = [f for f in findings if f.severity is Severity.WARNING]
    infos = [f for f in findings if f.severity is Severity.INFO]

    for finding in errors:
        err_console.print(f"[red]error[/red] {finding.render()}")
    for finding in warnings:
        err_console.print(f"[yellow]warning[/yellow] {finding.render()}")
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
        collect_wikidata_ids,
        load_mappings,
    )
    from gmeow_tools.wikidata import check_syntax

    loaded = load_mappings()
    if not loaded:
        err_console.print("[yellow]no mappings found[/yellow]")
        return

    syntax = check_syntax(collect_wikidata_ids(loaded))
    if not syntax.ok:
        raise _fail(f"✗ invalid Wikidata ids in mappings: {syntax.invalid}")

    DIST_DIR.mkdir(parents=True, exist_ok=True)
    alignments = build_alignment_graph(loaded)
    alignments.serialize(destination=DIST_DIR / "gmeow-alignments.ttl", format="turtle")
    linksets = build_linksets(loaded)
    linksets.serialize(destination=DIST_DIR / "gmeow-linksets.ttl", format="turtle")
    from rdflib import RDF
    from rdflib.namespace import VOID

    n_links = len(set(linksets.subjects(RDF.type, VOID.Linkset)))
    console.print(
        f"[green]✓ {len(loaded)} mappings → {len(alignments)} alignment axioms[/green]"
    )
    console.print(f"[green]✓ {n_links} VoID linkset descriptions[/green]")
    console.print(f"[green]✓ {len(syntax.valid)} Wikidata id(s) passed syntax[/green]")


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
    from gmeow_tools.mappings import collect_wikidata_ids, expand_curie, load_mappings
    from gmeow_tools.wikidata import (
        ExistenceStatus,
        check_existence,
        check_syntax,
        check_syntax_iri,
    )
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

    ids = collect_wikidata_ids(load_mappings())
    syntax = check_syntax(ids)
    console.print(f"[green]✓ {len(syntax.valid)} id(s) valid syntax[/green]")
    if syntax.invalid:
        err_console.print(f"[red]✗ invalid ids: {syntax.invalid}[/red]")
    if syntax.misuses:
        for _local, misuse, message in syntax.misuses:
            err_console.print(f"[yellow]{misuse.value}[/yellow] {message}")
    if not syntax.ok:
        raise _fail(f"✗ {len(syntax.invalid)} invalid, {len(syntax.misuses)} misuse(s)")

    # Also check full object IRIs for namespace misuse
    loaded = load_mappings()
    iri_misuses = []
    for mapping in loaded:
        iri_misuses.extend(
            check_syntax_iri(
                str(expand_curie(mapping.object_id)), in_object_position=True
            )
        )
    if iri_misuses:
        for _local, misuse, message in iri_misuses:
            err_console.print(f"[yellow]{misuse.value}[/yellow] {message}")
        raise _fail(f"✗ {len(iri_misuses)} namespace misuse(s) in mapping IRIs")

    if existence:
        try:
            statuses = check_existence(syntax.valid)
        except httpx.HTTPError as exc:  # network failure → visible, non-fatal skip
            err_console.print(f"[yellow]existence check skipped: {exc}[/yellow]")
            return
        bad = {k: v for k, v in statuses.items() if v is not ExistenceStatus.OK}
        for ident, status in bad.items():
            err_console.print(f"[red]{ident}: {status.value}[/red]")
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
    from gmeow_tools.wikidata_coverage import render_report, run_coverage

    report = run_coverage(threshold=threshold)
    text = render_report(report, json_mode=json_mode)
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
    from gmeow_tools.dc_coverage import render_report, run_coverage

    report = run_coverage(threshold=threshold)
    text = render_report(report, json_mode=json_mode)
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


@app.command(name="references-backfill")
def references_backfill(
    github: bool = typer.Option(
        True,
        "--github/--no-github",
        help="Include GitHub issue, PR, comment, and review text via the gh CLI.",
    ),
    repo: str | None = typer.Option(
        None,
        "--repo",
        help="GitHub repository in owner/name form (default: current GMEOW repo).",
    ),
    candidates_file: Path | None = typer.Option(  # noqa: B008
        None,
        "--candidates-file",
        help="JSONL audit output for harvested citation candidates.",
    ),
) -> None:
    """Backfill the canonical citation ledger from local and GitHub carriers."""
    from gmeow_tools.references import (
        DEFAULT_CANDIDATES_FILE,
        DEFAULT_REPO,
        backfill_references,
    )

    try:
        report = backfill_references(
            include_github=github,
            repo=repo or DEFAULT_REPO,
            candidates_file=candidates_file or DEFAULT_CANDIDATES_FILE,
        )
    except RuntimeError as exc:
        raise _fail(f"✗ citation backfill failed: {exc}") from exc
    console.print(
        "[green]✓[/green] references backfilled: "
        f"{report.unique_candidates} unique candidates "
        f"({report.local_candidates} local, {report.github_candidates} GitHub)"
    )
    console.print(f"  ledger: {report.references_file}")
    console.print(f"  candidates: {report.candidates_file}")


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
    """Build serializations, OWL-native syntaxes, and JSON-LD context into dist/."""
    from rdflib import Graph

    from gmeow_tools import reason as reasoning
    from gmeow_tools.jsonld_context import write_context
    from gmeow_tools.runner import ToolUnavailableError
    from gmeow_tools.serialize import serialize_graph

    try:
        merged = reasoning.merge_release()
        owl_native = reasoning.convert_owl_syntaxes(merged=merged)
    except ToolUnavailableError as exc:
        raise _fail(f"tool unavailable: {exc}", code=2) from exc

    graph = Graph().parse(merged, format="turtle")
    written = serialize_graph(graph, stem="gmeow")
    context = write_context()
    for path in (*written.values(), *owl_native, context):
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
    from rdflib import Graph

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
    from gmeow_tools.acceptance import (
        corpus_recall_pct,
        default_corpus,
        render_report,
        run_acceptance,
    )

    sources = [source] if source is not None else default_corpus()
    if not sources:
        raise _fail("no source given and no external/ snapshots found")
    try:
        results = [run_acceptance(s, descend=not floor) for s in sources]
    except (OSError, ValueError, SyntaxError) as exc:
        raise _fail(str(exc)) from exc

    report = render_report(results)
    if out is not None:
        out.write_text(report, encoding="utf-8")
        err_console.print(f"[green]wrote[/green] {out}")
    else:
        console.print(report, markup=False, highlight=False)
    for fa in results:
        verdict = "[green]PASS[/green]" if fa.passed else "[red]FAIL[/red]"
        err_console.print(f"{verdict} {fa.source}")

    if min_recall is not None:
        aggregate = corpus_recall_pct(results)
        if aggregate < min_recall:
            raise _fail(
                f"✗ corpus-aggregate round-trip recall {aggregate:.2f}% is below "
                f"the floor {min_recall:.2f}% ({len(results)} source(s))"
            )
        err_console.print(
            f"[green]✓[/green] corpus-aggregate round-trip recall "
            f"{aggregate:.2f}% ≥ floor {min_recall:.2f}%"
        )


_EXPORT_PROFILES = ("croissant", "ro-crate", "dcat", "datacite", "frictionless")


@app.command()
def export(
    profile: str = typer.Argument(
        ...,
        help="Research-object profile: all|" + "|".join(_EXPORT_PROFILES) + ".",
    ),
    data: list[Path] = typer.Argument(  # noqa: B008
        ...,
        help=(
            "GMEOW instance Turtle file(s) — must include a dataset "
            "descriptor (gmeow:Dataset + gmeow:hasLicense + gmeow:title)."
        ),
    ),
    out: Path = typer.Option(  # noqa: B008
        Path("dist/research-objects"),
        "--out",
        help="Output directory.",
    ),
) -> None:
    """Export GMEOW data as research objects (#58): Croissant, RO-Crate, ….

    Generated lossy projections of the canonical instance data (P4/P5):
    Croissant JSON-LD (Google Dataset Search / HF / Kaggle), an RO-Crate
    package (WorkflowHub), DCAT (W3C catalogs), DataCite deposit XML (DOI),
    and a Frictionless datapackage.json. Each declares what it drops.
    """
    from gmeow_tools.generator import GeneratorError
    from gmeow_tools.research_objects import (
        export_research_objects,
        package_ro_crate,
    )

    profiles = _EXPORT_PROFILES if profile == "all" else (profile,)
    unknown = set(profiles) - set(_EXPORT_PROFILES)
    if unknown:
        raise _fail(f"unknown profile(s): {', '.join(sorted(unknown))}")
    stem = data[0].stem
    try:
        written = export_research_objects(data, out, profiles=profiles, stem=stem)
    except (ValueError, GeneratorError) as exc:
        raise _fail(f"✗ {exc}") from exc
    if "ro-crate" in profiles:
        written.append(package_ro_crate(out / "ro-crate", out / f"{stem}.crate.zip"))
    for path in written:
        console.print(f"[green]✓[/green] {path}")


@app.command()
def docs() -> None:
    """Generate the native static ontology documentation site (#440)."""
    from gmeow_tools.config import PROJECT_ROOT
    from gmeow_tools.ontology_docs import build_ontology_docs

    out = PROJECT_ROOT / "ontology-docs"
    build_ontology_docs(out)
    console.print(f"[green]✓[/green] ontology docs → {out}")


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


@app.command(name="compile-gts")
def compile_gts(out: Path | None = _GTS_COMPILE_OUT) -> None:
    """Compile the statement-complete GTS dist snapshot (generated/gmeow.gts).

    The CLI face of the registered ``gts`` generator — the committed,
    drift-gated snapshot every exporter consumes (the narrow waist). With
    ``--out``, writes an ad-hoc copy of the identical bytes instead.
    """
    from gmeow_tools import config, gts_gen  # noqa: F401  (register side effect)
    from gmeow_tools.generator import run

    rdf12 = config.STATEMENT_RDF12_FILE
    if not rdf12.exists():
        raise _fail(
            f"RDF 1.2 statement artifact not found: {rdf12}\n"
            "run 'gmeow regenerate' first (a statement-less dist would drop "
            "confidence/standpoint/provenance)."
        )
    run("gts")
    if out is not None:
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_bytes(config.GTS_SNAPSHOT_FILE.read_bytes())
        console.print(f"[green]✓[/green] {out}")
    size = config.GTS_SNAPSHOT_FILE.stat().st_size
    console.print(f"[green]✓[/green] {config.GTS_SNAPSHOT_FILE} ({size} bytes)")


_GTS_FULL_OUT = typer.Option(
    None, "--out", "-o", help="Output .gts path (default: dist/gmeow.gts)."
)


@app.command(name="compile-gts-full")
def compile_gts_full(
    out: Path | None = _GTS_FULL_OUT,
    sign_key: Path | None = typer.Option(  # noqa: B008
        None, "--sign-key", help="Armored Ed25519 OpenPGP secret key file."
    ),
    public_key: Path | None = typer.Option(  # noqa: B008
        None, "--public-key", help="Armored OpenPGP public key file to embed."
    ),
) -> None:
    """Compile the offline-ready unified GMEOW snapshot.

    The registered ``gts`` generator emits an unsigned snapshot to
    ``generated/dist/gmeow.gts``. This command is the release path: it compiles
    the same snapshot, optionally signs every frame, and embeds the armored
    transport public key in the first ``meta`` frame.

    When ``--sign-key`` and ``--public-key`` are supplied, the ``kid`` is the
    OpenPGP fingerprint of the secret key and the public key armor is embedded
    as the file's transport key.
    """
    from gmeow_tools.config import DIST_DIR
    from gmeow_tools.gts_gen import compile_full_snapshot

    signer: gts.Signer | None = None
    public_key_armor: str | None = None
    if sign_key is not None or public_key is not None:
        if sign_key is None or public_key is None:
            raise _fail("--sign-key and --public-key must be supplied together")
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

    data = compile_full_snapshot(signer=signer, public_key_armor=public_key_armor)
    target = out or (DIST_DIR / "gmeow.gts")
    target.parent.mkdir(parents=True, exist_ok=True)
    try:
        target.write_bytes(data)
    except OSError as exc:
        raise _fail(f"cannot write {target}: {exc}") from exc
    console.print(f"[green]✓[/green] {target} ({len(data)} bytes)")
    if signer is not None:
        console.print(f"[green]✓[/green] signed with kid {signer.kid}")


@app.command(name="mcp")
def mcp_start() -> None:
    """Start the GMEOW MCP server (stdio transport).

    Exposes validation, compilation, reasoning, and term-lookup tools plus
    ontology resources to AI agents via the Model Context Protocol.
    """
    from gmeow_tools.mcp_server import run

    run()


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
    from gmeow_tools.foundation_import import run_import

    _, budget = run_import(jsonl, out_dir, nq)
    console.print(budget.as_text())
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


@app.command(name="create-docs")
def create_docs_cmd(
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
    """Emit a browsable Markdown docs tree from a GTS snapshot (#439).

    The tree includes per-term reference pages, slice guides, project doctrine
    docs, ontology web docs (#440), an alignment summary, and a statement-layer
    summary. All content is extracted from the bundled offline snapshot or any
    other ``.gts`` file.
    """
    from gmeow_tools.config import GTS_SNAPSHOT_FILE
    from gmeow_tools.create_docs import create_docs

    path = gts_file or GTS_SNAPSHOT_FILE
    selector = _resolve_lang(lang, _gts_tag_map(path))
    try:
        create_docs(path, directory, force=force, selector=selector)
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
            "✗ gmeow_logic extension not built — run `make logic-py` "
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
    from gmeow_tools import logic_compile as _lc  # noqa: F401  (register side effect)
    from gmeow_tools.config import PROJECT_ROOT as _PROJECT_ROOT
    from gmeow_tools.generator import registry as _registry
    from gmeow_tools.generator import run
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
    from gmeow_tools.logic_frontend import parse_logic_source
    from gmeow_tools.logic_projections import (
        OverclaimError,
        build_projection_report,
        project_canonical_rdf12,
        project_datalog,
        project_gufo,
        project_n3,
        project_owl_dl,
        project_owl_el,
    )
    from gmeow_tools.mapping_dsl import CompileError

    if mode is not None and mode not in _LOGIC_MODES:
        raise _fail(f"✗ unknown --mode {mode!r} (valid: {', '.join(_LOGIC_MODES)})")

    # --check with no --mode: use the framework's drift gate directly.
    if check and mode is None:
        report = run("logic", check=True)
        if report.drifted:
            for rel in sorted(report.drifted):
                err_console.print(f"[red]drift[/red] {rel}")
            raise _fail(
                f"✗ {len(report.drifted)} logic artifact(s) out of date — "
                "run `gmeow logic compile`"
            )
        console.print(
            "[green]✓ logic: committed artifacts match source (no drift)[/green]"
        )
        return

    # --mode only (no --check or with --check): parse and emit / inspect one back-end.
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
        _mode_to_fn = {
            "owl-dl": project_owl_dl,
            "owl-el": project_owl_el,
            "datalog": project_datalog,
            "n3": project_n3,
            "gufo": project_gufo,
            "canonical-rdf12": project_canonical_rdf12,
        }

        if not LOGIC_SOURCE_FILE.exists():
            raise _fail(
                f"✗ logic: source not found: {LOGIC_SOURCE_FILE}\n"
                "Is the repo checkout complete?"
            )

        try:
            program, diagnostics = parse_logic_source(LOGIC_SOURCE_FILE)
        except Exception as exc:
            raise _fail(f"✗ logic: parse failed: {exc}") from exc

        for diag in diagnostics:
            err_console.print(
                f"[yellow]{diag.severity}[/yellow] [{diag.code}] {diag.message}"
            )

        target_file = _mode_to_file[mode]

        if mode == "report":
            # Report requires all projections — run them all, write only report.
            try:
                r_dl = project_owl_dl(program)
                r_el = project_owl_el(program)
                r_dl_r = project_datalog(program)
                r_n3 = project_n3(program)
                r_gufo = project_gufo(program)
                r_rdf12 = project_canonical_rdf12(program)
                report_fn = build_projection_report
                result = report_fn(program, [r_dl, r_el, r_dl_r, r_n3, r_gufo, r_rdf12])
            except (OverclaimError, CompileError) as exc:
                raise _fail(f"✗ {exc}") from exc

            _report_banner = (
                "# GENERATED by `gmeow logic compile` — DO NOT EDIT.\n"
                "# Preservation loss ledger for all logic: projections.\n"
            )
            from gmeow_tools.logic_projections import _serialize_graph

            if check:
                # Compare fresh content with committed
                import tempfile

                with tempfile.NamedTemporaryFile(suffix=".ttl", delete=False) as tf:
                    tmp_path = Path(tf.name)
                _text = _serialize_graph(result, _report_banner)
                tmp_path.write_text(_text, encoding="utf-8")
                gen = _registry()["logic"]
                drifts = gen.compare(tmp_path, target_file)
                tmp_path.unlink(missing_ok=True)
                if drifts:
                    for d in drifts:
                        err_console.print(f"[red]drift[/red] {d}")
                    raise _fail("✗ --mode report: committed artifact drifted")
                console.print("[green]✓ --mode report: no drift[/green]")
            else:
                target_file.parent.mkdir(parents=True, exist_ok=True)
                _text = _serialize_graph(result, _report_banner)
                target_file.write_text(_text, encoding="utf-8")
                _rel = target_file.relative_to(_PROJECT_ROOT)
                console.print(f"[green]✓[/green] {_rel}")
            return

        # Single non-report mode
        proj_fn = _mode_to_fn[mode]
        try:
            if check:
                import tempfile

                _sfx = ".ttl" if mode not in ("datalog", "n3") else f".{mode}"
                with tempfile.NamedTemporaryFile(suffix=_sfx, delete=False) as tf:
                    tmp_path = Path(tf.name)
                _proj_result = proj_fn(program, path=tmp_path)
                del _proj_result  # result only needed for side-effect (file write)
                gen = _registry()["logic"]
                drifts = gen.compare(tmp_path, target_file)
                tmp_path.unlink(missing_ok=True)
                if drifts:
                    for d in drifts:
                        err_console.print(f"[red]drift[/red] {d}")
                    raise _fail(f"✗ --mode {mode}: committed artifact drifted")
                console.print(f"[green]✓ --mode {mode}: no drift[/green]")
            else:
                _proj_result = proj_fn(program, path=target_file)
                del _proj_result  # result only needed for side-effect (file write)
                _rel = target_file.relative_to(_PROJECT_ROOT)
                console.print(f"[green]✓[/green] {_rel}")
        except (OverclaimError, CompileError) as exc:
            raise _fail(f"✗ {exc}") from exc
        return

    # Default: full render of all 7 outputs via the generator framework.
    report = run("logic", check=False)
    for path in report.written:
        console.print(f"[green]✓[/green] {path.relative_to(_PROJECT_ROOT)}")
    if report.orphans:
        for orphan in report.orphans:
            err_console.print(f"[yellow]orphan[/yellow] {orphan}")
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
            "from a sibling profile.json, else default PositiveHornProfile."
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
    2. ``semantic_profile`` in a sibling ``profile.json``, if present;
    3. ``PositiveHornProfile`` (the v1 default).
    """
    from gmeow_tools.logic_frontend import LogicParseError, parse_logic_source
    from gmeow_tools.logic_ir import SemanticProfileId
    from gmeow_tools.logic_projections import extract_nemo_rules_section, project_nemo

    if not input_path.is_file():
        raise _fail(f"✗ certify: input not found: {input_path}")
    # parse_logic_source mints a source IRI via Path.as_uri(), which requires an
    # absolute path; resolve so the command works from any cwd / relative arg.
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
            if isinstance(sibling_data, dict):
                profile_str = str(
                    sibling_data.get("semantic_profile", "PositiveHornProfile")
                )

    try:
        declared_profile = SemanticProfileId(profile_str)
    except ValueError as exc:
        raise _fail(
            f"✗ certify: unknown profile {profile_str!r}; must be one of "
            f"{[str(p) for p in SemanticProfileId]}"
        ) from exc

    try:
        program, _diagnostics = parse_logic_source(input_path)
    except LogicParseError as exc:
        raise _fail(f"✗ certify: cannot parse {input_path}: {exc}") from exc

    # Rust-authoritative certification (#497): the native certifier is the
    # reasoning authority; the Python oracle is only a secondary validator.
    try:
        import gmeow_logic
    except ImportError as exc:
        raise _fail(
            "✗ certify: gmeow_logic native extension is not installed "
            "(certification is Rust-authoritative since #497) — run 'make logic-py'."
        ) from exc
    try:
        rules_only = extract_nemo_rules_section(project_nemo(program).content)
    except (ValueError, RuntimeError) as exc:
        raise _fail(f"✗ certify: cannot project/extract NEMO rules: {exc}") from exc
    try:
        verdict = gmeow_logic.certify(rules_only, str(declared_profile))
    except (ValueError, RuntimeError) as exc:
        raise _fail(f"✗ certify: native certifier failed: {exc}") from exc
    violations = list(verdict["violations"])
    if violations:
        err_console.print(
            f"[red]✗ certify: {len(violations)} violation(s) for "
            f"{declared_profile} in {input_path.name}[/red]"
        )
        for v in violations:
            err_console.print(f"[red]  {v}[/red]")
        raise typer.Exit(code=1)

    console.print(
        f"[green]✓ certify: {input_path.name} is certified for "
        f"{declared_profile}[/green]"
    )


@app.command()
def conformance(
    cases_root: Path | None = typer.Option(  # noqa: B008
        None,
        "--cases-root",
        help=(
            "Path to conformance/logic/ directory "
            "(default: <repo-root>/conformance/logic/)."
        ),
    ),
    mode: str = typer.Option(
        "native",
        "--mode",
        help="Engine mode to run: only 'native' is supported in the v1 oracle.",
    ),
    verbose: bool = typer.Option(
        False, "--verbose", "-v", help="Print per-case summary even on pass."
    ),
) -> None:
    """Run the logic: conformance suite and fail on any mismatch.

    Discovers every case under ``conformance/logic/cases/`` that contains
    ``input.logic.ttl`` + ``profile.json``, runs the v1 monotonic oracle over
    each, diffs the actual outputs against the committed ``expected/`` files
    using the runner contract comparison rules (graph isomorphism for RDF,
    canonical JSON for JSON, cited-IRI skeleton for explanations), and exits
    non-zero on any mismatch.

    This is the machine-checked gate that enforces Principle 7 (oracle ≡ engine)
    for the Python oracle: no engine drift without a red build.
    """
    from gmeow_tools.config import PROJECT_ROOT
    from gmeow_tools.logic_runner import RunnerError, diff_case, discover_cases
    from gmeow_tools.logic_runner import run as logic_run

    root = cases_root or (PROJECT_ROOT / "conformance" / "logic")
    if not root.is_dir():
        raise _fail(f"✗ conformance root not found: {root}")

    try:
        cases = discover_cases(root)
    except RunnerError as exc:
        raise _fail(f"✗ case discovery failed: {exc}") from exc

    if not cases:
        err_console.print(
            f"[yellow]warning[/yellow] no conformance cases found under {root}/cases/ "
            "(no directory with both input.logic.ttl and profile.json)"
        )
        console.print(
            "[green]✓ conformance: 0 cases (corpus is still a scaffold)[/green]"
        )
        return

    console.print(f"Running conformance over {len(cases)} case(s) (mode={mode!r}) …")

    total_pass = 0
    total_fail = 0
    all_diffs: list[str] = []

    for case in cases:
        try:
            outputs = logic_run(case.case_dir, mode=mode)
        except RunnerError as exc:
            all_diffs.append(f"[{case.case_id}] run() failed: {exc}")
            total_fail += 1
            err_console.print(f"[red]FAIL[/red] {case.case_id}: {exc}")
            continue
        except Exception as exc:
            all_diffs.append(f"[{case.case_id}] unexpected error: {exc}")
            total_fail += 1
            err_console.print(f"[red]ERROR[/red] {case.case_id}: {exc}")
            continue

        diff_result = diff_case(outputs)
        if diff_result.passed:
            total_pass += 1
            if verbose:
                console.print(f"[green]pass[/green] {case.case_id}")
        else:
            total_fail += 1
            all_diffs.extend(diff_result.diffs)
            for d in diff_result.diffs:
                err_console.print(f"[red]FAIL[/red] {d}")

    # Summary
    console.print(
        f"\n[bold]conformance:[/bold] {total_pass} passed, {total_fail} failed "
        f"({len(cases)} total)"
    )

    if total_fail:
        raise _fail(f"✗ {total_fail} conformance case(s) failed — see diffs above")
    console.print("[green]✓ conformance: all cases passed[/green]")


i18n_app = typer.Typer(help="Internationalization commands.", no_args_is_help=True)
app.add_typer(i18n_app, name="i18n")


def _i18n_output_path(
    slice_iri: str,
    slices_by_iri: dict[str, Slice],
    output_dir: Path,
    lang: str | None,
) -> Path:
    """Return the output path for a slice or namespace grouping."""
    slice_info = slices_by_iri.get(slice_iri)
    if slice_info is not None:
        if lang is None:
            return output_dir / "slices" / slice_info.group / f"{slice_info.name}.pot"
        return (
            output_dir
            / "slices"
            / slice_info.group
            / slice_info.name
            / "i18n"
            / f"{lang}.po"
        )
    local = slice_iri.rstrip("/#").split("/")[-1] if "/" in slice_iri else slice_iri
    if not local:
        local = "_"
    safe = "".join(c if c.isalnum() or c in "-_" else "_" for c in local)[:64]
    if lang is None:
        return output_dir / "slices" / "_core" / f"{safe}.pot"
    return output_dir / "slices" / "_core" / safe / "i18n" / f"{lang}.po"


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
    from rdflib import Graph, Literal, URIRef

    from gmeow_tools.graph import load_merged_graph
    from gmeow_tools.i18n_catalog import (
        LOCALIZABLE_PREDICATES,
        build_pot,
        extract_markdown,
        extract_ontology_docs_templates,
        extract_terms,
        write_po,
        write_pot,
    )
    from gmeow_tools.i18n_sync import PoEntry
    from gmeow_tools.slices import discover_slices

    graph = load_merged_graph(include_imports=False)
    slices_by_iri: dict[str, Slice] = discover_slices(root / "slices")

    # Map each localizable (term, predicate, value) triple to the slice
    # module(s) that declare it. This lets terms reused across slices with
    # different definitions be routed to the slice that actually owns the
    # literal, while terms not declared in any slice module fall back to
    # namespace-based grouping.
    value_sources: dict[tuple[str, str, str], set[str]] = {}
    for slice_info in slices_by_iri.values():
        if not slice_info.module_path.is_file():
            continue
        module_graph = Graph()
        try:
            module_graph.parse(slice_info.module_path, format="turtle")
        except Exception:
            continue
        for subject, predicate, obj in module_graph:
            if (
                isinstance(subject, URIRef)
                and predicate in LOCALIZABLE_PREDICATES
                and isinstance(obj, Literal)
            ):
                value_sources.setdefault(
                    (str(subject), str(predicate), str(obj)), set()
                ).add(slice_info.iri)

    def _resolve_slice(term_iri: str, predicate_iri: str, lexical: str) -> str | None:
        source_slices = value_sources.get((term_iri, predicate_iri, lexical))
        if source_slices:
            return min(source_slices)
        return None

    groups: dict[str, list[Any]] = {}
    total_keys = 0
    for key in extract_terms(graph, slice_resolver=_resolve_slice):
        groups.setdefault(key.slice_iri, []).append(key)
        total_keys += 1

    for slice_iri, keys in groups.items():
        path = _i18n_output_path(slice_iri, slices_by_iri, output_dir, lang)
        path.parent.mkdir(parents=True, exist_ok=True)
        if lang:
            entries = [
                PoEntry(
                    msgctxt=f"{key.term_iri}|{key.predicate}",
                    msgid=key.english_value,
                    msgstr=key.english_value,
                )
                for key in keys
            ]
            write_po(path, entries, lang)
        else:
            path.write_text(build_pot(keys), encoding="utf-8")

    if not terms_only:
        docs_output = output_dir / "docs"
        docs_output.mkdir(parents=True, exist_ok=True)

        md_sources: list[Path] = []
        md_sources.extend(sorted(root.glob("slices/*/*/docs.md")))
        md_sources.extend(sorted((root / "docs").glob("*.md")))
        if (root / "README.md").is_file():
            md_sources.append(root / "README.md")

        for source in md_sources:
            rel = source.relative_to(root)
            entries = extract_markdown(source, rel_path=rel.as_posix())
            path = (
                docs_output / f"{rel}.{lang}.po" if lang else docs_output / f"{rel}.pot"
            )
            path.parent.mkdir(parents=True, exist_ok=True)
            if lang:
                po_entries = [
                    PoEntry(
                        msgctxt=entry.msgctxt,
                        msgid=entry.msgid,
                        msgstr=entry.msgid,
                    )
                    for entry in entries
                ]
                write_po(path, po_entries, lang)
            else:
                write_pot(path, entries)

        template_entries = extract_ontology_docs_templates()
        template_path = (
            output_dir / f"ontology-docs-templates.{lang}.po"
            if lang
            else output_dir / "ontology-docs-templates.pot"
        )
        if lang:
            po_entries = [
                PoEntry(
                    msgctxt=entry.msgctxt,
                    msgid=entry.msgid,
                    msgstr=entry.msgid,
                )
                for entry in template_entries
            ]
            write_po(template_path, po_entries, lang)
        else:
            write_pot(template_path, template_entries)

    console.print(
        f"[green]✓[/green] wrote {len(groups)} term catalog(s) "
        f"({total_keys} keys) to {output_dir}"
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
    from gmeow_tools.i18n_sync import sync_english_file

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
            report = sync_english_file(po_path, source_path, dry_run=dry_run)
            processed += 1
            changed_files.extend(report.changed_files)
            conflicts.extend(report.conflicts)
            skipped.extend(report.skipped)
            unchanged += len(report.unchanged)

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
    from gmeow_tools.graph import load_merged_graph
    from gmeow_tools.i18n_catalog import _language_from_po, merge_terms

    po_paths = sorted(root.glob("slices/*/*/i18n/*.po"))
    if lang is not None:
        lang_lower = lang.lower()
        po_paths = [
            p
            for p in po_paths
            if _language_from_po(p.read_text(encoding="utf-8")).lower() == lang_lower
        ]

    base_graph = load_merged_graph(include_imports=False)
    merged_graph = merge_terms(base_graph, po_paths)
    added = len(merged_graph) - len(base_graph)

    ttl = merged_graph.serialize(format="turtle")
    if output is None:
        console.print(ttl, end="")
        output_note = "stdout"
    else:
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(ttl, encoding="utf-8")
        output_note = str(output)

    err_console.print(
        f"[green]✓ merged {len(po_paths)} PO file(s), "
        f"{added} translated triple(s) added → {output_note}[/green]"
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
    from gmeow_tools.i18n_catalog import iter_po_catalogs, write_csv_export

    write_csv_export(iter_po_catalogs(root), output)


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
    from gmeow_tools.i18n_catalog import iter_po_catalogs, write_xliff_export

    write_xliff_export(iter_po_catalogs(root), output)


if __name__ == "__main__":  # pragma: no cover
    app()
