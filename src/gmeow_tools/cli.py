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


@app.callback()
def main() -> None:
    """GMEOW ontology toolchain (see subcommands)."""


@app.command()
def version() -> None:
    """Print the gmeow_tools package version."""
    console.print(__version__)


@app.command()
def regenerate(
    names: list[str] | None = typer.Argument(  # noqa: B008
        None,
        help="Generator names to run (default: all in dependency order).",
    ),
) -> None:
    """Rebuild all checked-in generated artifacts from canonical sources.

    Runs every registered generator in topologically sorted order, or the
    named generators in the order given.
    """
    # Import all generator modules to trigger @register side effects.
    from gmeow_tools import (  # noqa: F401
        apache,
        export,
        lpg,
        mapping_compile,
        metadata,
        schema_compile,
        statement_compile,
    )
    from gmeow_tools.config import PROJECT_ROOT
    from gmeow_tools.generator import regenerate as _regenerate

    results = _regenerate(names or None)
    for _name, report in results.items():
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
) -> None:
    """Drift + orphan check for every registered generator.

    Runs ``--check`` mode for all registered generators (or the named ones)
    and exits non-zero if any drift or orphans are found.
    """
    # Import all generator modules to trigger @register side effects.
    from gmeow_tools import (  # noqa: F401
        apache,
        export,
        lpg,
        mapping_compile,
        metadata,
        schema_compile,
        statement_compile,
    )
    from gmeow_tools.generator import check_all

    results = check_all(names or None)
    total_drift = 0
    total_orphans = 0
    for name, report in results.items():
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
) -> None:
    """Merge the import closure, validate its OWL 2 profile, then reason."""
    from gmeow_tools import reason as reasoning
    from gmeow_tools.runner import ToolExecutionError, ToolUnavailableError

    try:
        reasoning.merge_release()
        console.print("[green]✓ merged import closure[/green]")
        reasoning.validate_profile(profile)
        console.print(f"[green]✓ OWL 2 {profile} profile[/green]")
        reasoning.reason(reasoner)
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
) -> None:
    """Run reasoned-graph negative tests (ROBOT verify over queries/verify/).

    The closed-world QC lane of the hybrid OWL+SHACL architecture: reason, then
    run each SPARQL "bad-example" query over the materialized graph. Any returned
    row is a violation (the OBO QC pattern), failing the gate.
    """
    from gmeow_tools import reason as reasoning
    from gmeow_tools.runner import ToolExecutionError, ToolUnavailableError

    try:
        reasoning.verify(reasoner=reasoner)
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
    """Build serializations and JSON-LD context into dist/."""
    from rdflib import Graph

    from gmeow_tools import reason as reasoning
    from gmeow_tools.jsonld_context import write_context
    from gmeow_tools.runner import ToolUnavailableError
    from gmeow_tools.serialize import serialize_graph

    try:
        merged = reasoning.merge_release()
    except ToolUnavailableError as exc:
        raise _fail(f"tool unavailable: {exc}", code=2) from exc

    graph = Graph().parse(merged, format="turtle")
    written = serialize_graph(graph, stem="gmeow")
    context = write_context()
    for path in (*written.values(), context):
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


@gts_app.command("info")
def gts_info(file: Path) -> None:
    """Summarise a GTS file: terms/quads/blobs counts and any diagnostics."""
    from gmeow_tools import gts

    try:
        data = file.read_bytes()
    except OSError as exc:
        raise _fail(f"cannot read {file}: {exc}") from exc
    graph = gts.read(data)
    console.print(
        f"[bold]{file.name}[/bold]: {len(graph.terms)} terms, "
        f"{len(graph.quads)} quads, {len(graph.reifiers)} reifiers, "
        f"{len(graph.annotations)} annotations, {len(graph.blobs)} blobs, "
        f"{len(graph.opaque)} opaque"
    )
    for diag in graph.diagnostics:
        err_console.print(f"[yellow]{diag.code}[/yellow]: {diag.detail}")


_GTS_OUT_OPTION = typer.Option(
    None, "--out", "-o", help="Write N-Quads here (else stdout)."
)


@gts_app.command("to-nq")
def gts_to_nq(file: Path, out: Path | None = _GTS_OUT_OPTION) -> None:
    """Transform a GTS file to N-Quads (§14 of GTS-SPEC.md)."""
    from gmeow_tools import gts

    try:
        graph = gts.read(file.read_bytes())
    except OSError as exc:
        raise _fail(f"cannot read {file}: {exc}") from exc
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

    from gmeow_tools import gts

    fmt = guess_format(str(file))
    source: Graph = Dataset() if fmt in {"nquads", "trig", "json-ld"} else Graph()
    try:
        source.parse(str(file), format=fmt)
    except (OSError, ValueError, SyntaxError) as exc:
        raise _fail(f"cannot read or parse {file}: {exc}") from exc
    data = gts.gts_from_graph(source)
    target = out or file.with_suffix(".gts")
    try:
        target.write_bytes(data)
    except OSError as exc:
        raise _fail(f"cannot write {target}: {exc}") from exc
    console.print(f"[green]✓[/green] {target} ({len(data)} bytes)")


def _gts_to_db(file: Path, out: Path | None, suffix: str, kind: str) -> None:
    """Shared body for the gts → {sqlite,duckdb} transforms."""
    from gmeow_tools import gts

    try:
        graph = gts.read(file.read_bytes())
    except OSError as exc:
        raise _fail(f"cannot read {file}: {exc}") from exc
    target = out or file.with_suffix(suffix)
    writer = gts.to_sqlite if kind == "sqlite" else gts.to_duckdb
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
    """Compile a statement-complete GTS dist snapshot (dist/gmeow.gts).

    Merges the RDF 1.1 base graph with the RDF 1.2 statement layer
    (``statements/gmeow.rdf12.ttl``, via pyoxigraph) so confidence / standpoint /
    provenance survive into the shipped artifact.
    """
    from gmeow_tools import config, gts
    from gmeow_tools.graph import load_merged_graph

    source = load_merged_graph(include_imports=False)
    rdf12 = config.STATEMENTS_DIR / "gmeow.rdf12.ttl"
    if not rdf12.exists():
        raise _fail(
            f"RDF 1.2 statement artifact not found: {rdf12}\n"
            "run 'gmeow regenerate' first (a statement-less dist would drop "
            "confidence/standpoint/provenance)."
        )
    data = gts.compile_gts(source, rdf12)
    target = out or (config.DIST_DIR / "gmeow.gts")
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(data)
    console.print(f"[green]✓[/green] {target} ({len(data)} bytes)")


@app.command(name="mcp")
def mcp_start() -> None:
    """Start the GMEOW MCP server (stdio transport).

    Exposes validation, compilation, reasoning, and term-lookup tools plus
    ontology resources to AI agents via the Model Context Protocol.
    """
    from gmeow_tools.mcp_server import run

    run()


if __name__ == "__main__":  # pragma: no cover
    app()
