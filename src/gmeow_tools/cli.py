"""Command-line entry point for the GMEOW tooling.

The CLI is a thin orchestration layer: every subcommand delegates to a focused
module (``validate``, ``reason``, ``mappings`` …) so the command surface stays
declarative and the logic stays unit-testable. The Makefile shells into these
subcommands rather than reimplementing any behaviour.
"""

from __future__ import annotations

from pathlib import Path

import httpx
import typer
from rich.console import Console

import gts
from gmeow_tools import __version__
from gmeow_tools.projections import PROFILES as _PROFILES

app = typer.Typer(
    name="gmeow",
    help="Build, validate, reason over, and publish the GMEOW ontology.",
    no_args_is_help=True,
    add_completion=False,
)
console = Console()
err_console = Console(stderr=True)


def _fail(message: str, code: int = 1) -> typer.Exit:
    """Print an error and return an Exit to raise."""
    err_console.print(f"[red]{message}[/red]")
    return typer.Exit(code=code)


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
    """GMEOW ontology toolchain (see subcommands)."""


@app.command()
def version() -> None:
    """Print the gmeow_tools package version."""
    console.print(__version__)


@app.command()
def info() -> None:
    """Show a summary of the bundled GMEOW ontology snapshot."""
    from gmeow_tools.config import GTS_FULL_SNAPSHOT_FILE

    path = GTS_FULL_SNAPSHOT_FILE
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
    from gmeow_tools import (  # noqa: F401
        apache,
        catalog_gen,
        evals,
        export,
        frame_shapes_gen,
        gts_full_gen,
        gts_gen,
        gts_vectors_gen,
        lpg,
        mapping_compile,
        matrix,
        metadata,
        parquet_gen,
        profiles_gen,
        research_objects,
        schema_compile,
        statement_compile,
    )
    from gmeow_tools.config import PROJECT_ROOT
    from gmeow_tools.generator import regenerate as _regenerate

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
    from gmeow_tools import (  # noqa: F401
        apache,
        catalog_gen,
        evals,
        export,
        frame_shapes_gen,
        gts_full_gen,
        gts_gen,
        gts_vectors_gen,
        lpg,
        mapping_compile,
        matrix,
        metadata,
        parquet_gen,
        profiles_gen,
        research_objects,
        schema_compile,
        statement_compile,
    )
    from gmeow_tools.generator import check_all, registry

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


@app.command(name="compile-statements-pyoxigraph")
def compile_statements_pyoxigraph() -> None:
    """Cross-check statement-dsl/ against committed artifacts (pyoxigraph).

    Non-authoritative read-only mirror that uses pyoxigraph instead of Apache
    Jena for the RDF 1.2 projection and normalization. Proves the round-trip
    is engine-independent (CONSTITUTION Principle 7). Never writes — Jena
    remains the sole canonical artifact writer (Principle 4).
    """
    from gmeow_tools.mapping_dsl import CompileError
    from gmeow_tools.statement_compile_pyoxigraph import (
        compile_statements_pyoxigraph as run,
    )

    try:
        report = run()
    except CompileError as exc:
        raise _fail(f"✗ {exc}") from exc

    if report.drifted:
        for rel in sorted(report.drifted):
            err_console.print(f"[red]drift[/red] {rel}")
        raise _fail(
            f"✗ {len(report.drifted)} statement artifact(s) out of date — "
            "run `gmeow regenerate`"
        )
    console.print(
        "[green]✓ pyoxigraph cross-check: committed artifacts match "
        "statement-dsl/ (no drift)[/green]"
    )


@app.command()
def validate() -> None:
    """Validate Turtle syntax, term annotations, and SHACL conformance."""
    from gmeow_tools.validate import validate_all

    result = validate_all()
    for warning in result.warnings:
        err_console.print(f"[yellow]warning[/yellow] {warning}")
    for error in result.errors:
        err_console.print(f"[red]error[/red] {error}")
    if result.ok:
        console.print("[green]✓ validation passed[/green]")
    else:
        raise _fail(f"✗ {len(result.errors)} error(s)")


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
def compliance_report_cmd() -> None:
    """Run the in-process gates and emit the RDF compliance report (#285)."""
    from gmeow_tools.compliance import write_report

    path = write_report()
    console.print(f"[green]✓ compliance report written to {path}[/green]")


@app.command(name="crosscheck-queries")
def crosscheck_queries() -> None:
    """Prove rdflib and pyoxigraph answer every committed query identically.

    The trust anchor that licenses the test suite to run on the fast pyoxigraph
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
            f"✗ {len(diverged)} query/queries diverge between rdflib and pyoxigraph"
        )
    console.print(
        f"[green]✓ {len(checked)} queries agree across rdflib + pyoxigraph"
        f" ({len(skipped)} skipped)[/green]"
    )


@app.command()
def reason(
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
    """Merge the import closure, validate its OWL 2 profile, then reason."""
    from gmeow_tools import reason as reasoning
    from gmeow_tools.runner import ToolExecutionError, ToolUnavailableError

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
    reasoner: str = typer.Option("ELK", help="Reasoner: ELK (fast) or hermit (DL)."),
    reasoned_input: Path | None = _REASONED_INPUT_OPTION,
) -> None:
    """Run reasoned-graph negative tests (ROBOT verify over queries/verify/).

    The closed-world QC lane of the hybrid OWL+SHACL architecture: reason, then
    run each SPARQL "bad-example" query over the materialized graph. Any returned
    row is a violation (the OBO QC pattern), failing the gate.
    """
    from gmeow_tools import reason as reasoning
    from gmeow_tools.runner import ToolExecutionError, ToolUnavailableError

    try:
        reasoning.verify(reasoner=reasoner, reasoned=reasoned_input)
    except ToolUnavailableError as exc:
        raise _fail(f"tool unavailable: {exc}", code=2) from exc
    except ToolExecutionError as exc:
        raise _fail(f"verify found violations:\n{exc.output}") from exc
    console.print("[green]✓ verify: no violations on the reasoned graph[/green]")


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
) -> None:
    """Report how much of the vendored entity slice GMEOW covers."""
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


@app.command()
def crossref() -> None:
    """Generate the CrossRef DOI deposit XML (deposit schema 5.4.0)."""
    from gmeow_tools.crossref import write_deposit
    from gmeow_tools.self_desc import load_self_description

    try:
        meta = load_self_description()
    except (FileNotFoundError, ValueError) as exc:
        raise _fail(f"✗ self-description unavailable: {exc}") from exc
    path = write_deposit()
    console.print(
        f"[green]✓ {path.relative_to(path.parents[1])} (DOI {meta.doi})[/green]"
    )
    if meta.doi.startswith("10.XXXXX/"):
        err_console.print(
            "[yellow]note:[/yellow] DOI prefix is a placeholder — set "
            "the DOI in metadata/gmeow-self.ttl once CrossRef membership is finalized."
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
    profile: str = typer.Option(
        "all",
        help="Target profile: all|" + "|".join(sorted(_PROFILES)) + ".",
    ),
    data: str = typer.Option(
        "", help="GMEOW data file to project (default: the worked-example fixtures)."
    ),
) -> None:
    """Project GMEOW to a pure schema.org/GeoSPARQL/vCard/FOAF/iCal/OWL-Time profile.

    The FnO/EDOAL specs under projections/ describe the transformations; this runs
    their executable SPARQL CONSTRUCT form (pure-Python rdflib). Lossy by design.
    """
    from gmeow_tools.projections import PROFILES, project_examples, project_file

    names = list(PROFILES) if profile == "all" else [profile]
    for name in names:
        if name not in PROFILES:
            raise _fail(f"unknown profile: {name}")
    if not data:
        for path in project_examples():
            console.print(f"[green]✓[/green] {path.relative_to(path.parents[1])}")
    else:
        for name in names:
            path = project_file(Path(data), name)
            console.print(f"[green]✓[/green] {path.relative_to(path.parents[1])}")


@app.command()
def transform(
    abox: Path = typer.Argument(  # noqa: B008
        ...,
        help="Canonical GMEOW A-Box Turtle file (the single source of truth).",
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
) -> None:
    """Transpile an A-Box to MAXIMAL(G) = G + E(G) + P(G) (#34).

    One fat multi-vocabulary file family: <stem>.gts (canonical, full RDF 1.2
    provenance audit trail), index.nq (RDF 1.2), index.ttl / index.jsonld
    (asserted base triples — plain-RDF readable). Saturation materializes
    STRONG equivalences only, gated by the alignment-direction lint;
    suppression (displayable false) is honored fail-closed.
    """
    from rdflib import Graph

    from gmeow_tools.transform import TransformAbortedError, vocab_coverage
    from gmeow_tools.transform import transform as run_transform

    names = None if profiles == "all" else [p.strip() for p in profiles.split(",")]
    try:
        result = run_transform(abox, out_dir=out, profiles=names)
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
        ..., help="A non-GMEOW source RDF file (Turtle) to lift up into GMEOW."
    ),
    out: Path | None = typer.Option(  # noqa: B008
        None, "-o", "--out", help="Write the GMEOW lift here (default: stdout Turtle)."
    ),
) -> None:
    """Lift a consumer-vocabulary RDF file UP into pure GMEOW (clean-reversal, #451).

    Rewrites each term that has a mechanically-invertible alignment rule to its
    GMEOW counterpart; terms with no clean rule, or whose reverse is ambiguous
    (a many-to-one down-image), are reported and left out — never guessed.
    """
    from rdflib import Graph

    from gmeow_tools.up_projection import up_project

    try:
        src = Graph().parse(source, format="turtle")
    except (OSError, ValueError, SyntaxError) as exc:
        raise _fail(f"cannot read or parse {source}: {exc}") from exc
    try:
        result = up_project(src)
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
        f"[green]lifted[/green] {result.lifted} triples · "
        f"[yellow]gap[/yellow] {len(result.gap_terms)} terms · "
        f"[yellow]ambiguous[/yellow] {len(result.ambiguous_terms)} terms",
    )
    for term, n in sorted(result.gap_terms.items()):
        err_console.print(f"[yellow]gap[/yellow] {term} (x{n})")
    for term, n in sorted(result.ambiguous_terms.items()):
        err_console.print(f"[yellow]ambiguous[/yellow] {term} (x{n})")


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
def docs(
    widoco: bool = typer.Option(False, "--widoco", help="Also run WIDOCO (Docker)."),
) -> None:
    """Generate HTML documentation (pyLODE; optionally WIDOCO)."""
    from gmeow_tools.docs import pylode_html, widoco_available, widoco_docs

    out = pylode_html()
    console.print(f"[green]✓ pyLODE → {out}[/green]")
    if widoco:
        from gmeow_tools import reason as reasoning
        from gmeow_tools.runner import ToolUnavailableError

        if not widoco_available():
            err_console.print("[yellow]WIDOCO image absent; skipping[/yellow]")
            return
        try:
            merged = reasoning.merge_release()
            outdir = widoco_docs(merged)
        except ToolUnavailableError as exc:
            raise _fail(f"tool unavailable: {exc}", code=2) from exc
        console.print(f"[green]✓ WIDOCO → {outdir}[/green]")


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


gts_app = typer.Typer(
    name="gts",
    help="Graph Transport Substrate (GTS) reference reader and transforms.",
    no_args_is_help=True,
)
app.add_typer(gts_app, name="gts")


def _default_gts_file() -> Path:
    """The bundled full ontology snapshot (offline wheel default)."""
    from gmeow_tools.config import GTS_FULL_SNAPSHOT_FILE

    return GTS_FULL_SNAPSHOT_FILE


@gts_app.command("info")
def gts_info(
    file: Path | None = typer.Argument(  # noqa: B008
        None,
        help="GTS file to summarise (default: bundled gmeow-full.gts).",
    ),
    no_verify: bool = typer.Option(
        False, "--no-verify", help="Skip embedded-signature verification."
    ),
) -> None:
    """Summarise a GTS file: terms/quads/blobs counts and any diagnostics.

    When the file contains signatures, verification is run automatically
    (cheap because the embedded transport key is folded into the first meta
    frame).  Use ``--no-verify`` to inspect a damaged or partially trusted file.
    """
    from gts.verify import verify_file

    path = file or _default_gts_file()
    graph = _read_gts_or_fail(path)
    console.print(
        f"[bold]{path.name}[/bold]: {len(graph.terms)} terms, "
        f"{len(graph.quads)} quads, {len(graph.reifiers)} reifiers, "
        f"{len(graph.annotations)} annotations, {len(graph.blobs)} blobs, "
        f"{len(graph.opaque)} opaque"
    )
    if not no_verify:
        result = verify_file(path.read_bytes(), require_signatures=False)
        if result.signed:
            status = "[green]valid[/green]" if result.ok else "[red]FAILED[/red]"
            console.print(
                f"signatures: {result.signed} signed, {result.valid} valid, "
                f"{result.invalid} invalid, {result.unverified} unverified — {status}"
            )
            if result.fingerprint:
                console.print(
                    f"transport key: [bold]{result.fingerprint}[/bold]  "
                    f"{result.emojihash}"
                )
        else:
            console.print("signatures: none")
        for err in result.errors:
            err_console.print(f"[red]{err}[/red]")
        if result.signed and not result.ok:
            raise _fail("signature verification failed")
    for diag in graph.diagnostics:
        err_console.print(f"[yellow]{diag.code}[/yellow]: {diag.detail}")


@gts_app.command("verify")
def gts_verify(
    file: Path | None = typer.Argument(  # noqa: B008
        None,
        help="GTS file to verify (default: bundled gmeow-full.gts).",
    ),
    trusted_key: Path | None = typer.Option(  # noqa: B008
        None,
        "--trusted-key",
        help="Out-of-band armored OpenPGP public key (overrides embedded key).",
    ),
) -> None:
    """Verify every embedded COSE signature against the transport key.

    By default the transport key embedded in the file's first ``meta`` frame is
    used.  Pass ``--trusted-key`` to verify against a key obtained out of band
    (e.g. the release key published in the repository).
    """
    from gts.verify import verify_file

    path = file or _default_gts_file()
    try:
        data = path.read_bytes()
    except OSError as exc:
        raise _fail(f"cannot read {path}: {exc}") from exc
    armored: str | None = None
    if trusted_key is not None:
        try:
            armored = trusted_key.read_text(encoding="utf-8")
        except OSError as exc:
            raise _fail(f"cannot read --trusted-key {trusted_key}: {exc}") from exc
    try:
        result = verify_file(data, armored_key=armored, require_signatures=True)
    except Exception as exc:
        raise _fail(f"verification failed: {exc}") from exc

    if result.fingerprint:
        console.print(f"transport key: [bold]{result.fingerprint}[/bold]")
        if result.emojihash:
            console.print(f"emojihash:     {result.emojihash}")
        if result.randomart:
            console.print(result.randomart)
    console.print(
        f"signatures: {result.signed} signed, {result.valid} valid, "
        f"{result.invalid} invalid, {result.unverified} unverified"
    )
    for err in result.errors:
        err_console.print(f"[red]{err}[/red]")
    for diag in result.diagnostics:
        err_console.print(f"[yellow]{diag.code}[/yellow]: {diag.detail}")
    if not result.ok:
        raise _fail("verification failed")
    console.print("[green]✓ verification passed[/green]")


@gts_app.command("extract-key")
def gts_extract_key(
    file: Path | None = typer.Argument(  # noqa: B008
        None,
        help="GTS file to extract the key from (default: bundled gmeow-full.gts).",
    ),
    out: Path | None = typer.Option(  # noqa: B008
        None, "--out", "-o", help="Write the armored public key here (else stdout)."
    ),
) -> None:
    """Extract the embedded OpenPGP transport public key from a GTS file."""
    from gts.verify import extract_transport_key

    path = file or _default_gts_file()
    graph = _read_gts_or_fail(path)
    transport = extract_transport_key(graph)
    if transport is None:
        raise _fail("no gts:transportKey found in file metadata")
    armor = transport["gpg"]
    if out is None:
        console.print(armor, end="")
        return
    try:
        out.write_text(armor, encoding="utf-8")
    except OSError as exc:
        raise _fail(f"cannot write {out}: {exc}") from exc
    console.print(f"[green]✓[/green] {out}")


_GTS_OUT_OPTION = typer.Option(
    None, "--out", "-o", help="Write N-Quads here (else stdout)."
)


@gts_app.command("to-nq")
def gts_to_nq(
    file: Path | None = typer.Argument(  # noqa: B008
        None,
        help="GTS file to project (default: bundled gmeow-full.gts).",
    ),
    out: Path | None = _GTS_OUT_OPTION,
) -> None:
    """Transform a GTS file to N-Quads (§14 of GTS-SPEC.md)."""
    path = file or _default_gts_file()
    graph = _read_gts_or_fail(path)
    text = gts.to_nquads(graph)
    if out is None:
        console.print(text, end="")
        return
    try:
        out.write_text(text, encoding="utf-8")
    except OSError as exc:
        raise _fail(f"cannot write {out}: {exc}") from exc
    console.print(f"[green]✓[/green] {out}")


_GTS_GTS_OUT = typer.Option(
    None, "--out", "-o", help="Output .gts path (default: <input>.gts)."
)
_GTS_DB_OUT = typer.Option(None, "--out", "-o", help="Output database path.")


@gts_app.command("from-rdf")
def gts_from_rdf(file: Path, out: Path | None = _GTS_GTS_OUT) -> None:
    """Produce a GTS dist snapshot from an RDF file (Turtle/N-Triples/N-Quads/…)."""
    from rdflib import Dataset, Graph
    from rdflib.util import guess_format

    from gmeow_tools.gts_producer import gts_from_graph

    fmt = guess_format(str(file))
    source: Graph = Dataset() if fmt in {"nquads", "trig", "json-ld"} else Graph()
    try:
        source.parse(str(file), format=fmt)
    except (OSError, ValueError, SyntaxError) as exc:
        raise _fail(f"cannot read or parse {file}: {exc}") from exc
    data = gts_from_graph(source)
    target = out or file.with_suffix(".gts")
    try:
        target.write_bytes(data)
    except OSError as exc:
        raise _fail(f"cannot write {target}: {exc}") from exc
    console.print(f"[green]✓[/green] {target} ({len(data)} bytes)")


def _gts_to_db(file: Path, out: Path | None, suffix: str, kind: str) -> None:
    """Shared body for the gts → {sqlite,duckdb} transforms."""
    from gmeow_tools.gts_db import to_duckdb, to_sqlite

    graph = _read_gts_or_fail(file)
    target = out or file.with_suffix(suffix)
    writer = to_sqlite if kind == "sqlite" else to_duckdb
    try:
        writer(graph, target)
    except OSError as exc:
        raise _fail(f"cannot write {target}: {exc}") from exc
    console.print(f"[green]✓[/green] {target}")


@gts_app.command("to-sqlite")
def gts_to_sqlite(file: Path, out: Path | None = _GTS_DB_OUT) -> None:
    """Transform a GTS file to a SQLite database (dictionary-encoded tables)."""
    _gts_to_db(file, out, ".sqlite", "sqlite")


@gts_app.command("to-duckdb")
def gts_to_duckdb(file: Path, out: Path | None = _GTS_DB_OUT) -> None:
    """Transform a GTS file to a DuckDB database (dictionary-encoded tables)."""
    _gts_to_db(file, out, ".duckdb", "duckdb")


@gts_app.command("compile")
def gts_compile(out: Path | None = _GTS_GTS_OUT) -> None:
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
    None, "--out", "-o", help="Output .gts path (default: dist/gmeow-full.gts)."
)


@gts_app.command("compile-full")
def gts_compile_full(
    out: Path | None = _GTS_FULL_OUT,
    sign_key: Path | None = typer.Option(  # noqa: B008
        None, "--sign-key", help="Armored Ed25519 OpenPGP secret key file."
    ),
    public_key: Path | None = typer.Option(  # noqa: B008
        None, "--public-key", help="Armored OpenPGP public key file to embed."
    ),
) -> None:
    """Compile the offline-ready GMEOW full snapshot.

    The registered ``gts-full`` generator already emits an unsigned snapshot to
    ``generated/dist/gmeow-full.gts``. This command is the release path: it
    compiles the same snapshot, optionally signs every frame, and embeds the
    armored transport public key in the first ``meta`` frame.

    When ``--sign-key`` and ``--public-key`` are supplied, the ``kid`` is the
    OpenPGP fingerprint of the secret key and the public key armor is embedded
    as the file's transport key.
    """
    from gmeow_tools.config import DIST_DIR
    from gmeow_tools.gts_full_gen import compile_full_snapshot

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
    target = out or (DIST_DIR / "gmeow-full.gts")
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
) -> None:
    """Describe a GMEOW term as useful prose (#325).

    Composes definition, stereotype, slice + tier, alignments, scope notes,
    examples, and the flat-first/reify-on-demand pairing. Works offline
    against any .gts file. Defaults to the repo graph when run inside the
    checkout; otherwise falls back to the bundled gmeow-full.gts.
    """
    from gmeow_tools.describe import describe as _describe

    gts_path = gts
    if gts_path is None:
        from gmeow_tools.config import GTS_FULL_SNAPSHOT_FILE, ONTOLOGY_FILE

        if not ONTOLOGY_FILE.exists():
            gts_path = GTS_FULL_SNAPSHOT_FILE
    text, code = _describe(term, gts_path)
    console.print(text)
    if code:
        raise typer.Exit(code=code)


@app.command(name="create-docs")
def create_docs_cmd(
    gts_file: Path | None = typer.Argument(  # noqa: B008
        None,
        help="GTS file to project (default: bundled gmeow-full.gts).",
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
) -> None:
    """Emit a browsable Markdown docs tree from a GTS snapshot (#439).

    The tree includes per-term reference pages, slice guides, project doctrine
    docs, ontology web docs (#440), an alignment summary, and a statement-layer
    summary. All content is extracted from the bundled offline snapshot or any
    other ``.gts`` file.
    """
    from gmeow_tools.create_docs import create_docs

    path = gts_file or _default_gts_file()
    try:
        create_docs(path, directory, force=force)
    except FileExistsError as exc:
        raise _fail(str(exc)) from exc
    except (OSError, ValueError) as exc:
        raise _fail(f"cannot create docs tree: {exc}") from exc
    console.print(f"[green]✓[/green] docs tree → {directory}")


if __name__ == "__main__":  # pragma: no cover
    app()
