"""Command-line entry point for the GMEOW tooling.

The CLI is a thin orchestration layer: every subcommand delegates to a focused
module (``validate``, ``reason``, ``mappings`` …) so the command surface stays
declarative and the logic stays unit-testable. The Makefile shells into these
subcommands rather than reimplementing any behaviour.
"""

from __future__ import annotations

from pathlib import Path

import typer
from rich.console import Console

from gmeow_tools import __version__

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
) -> None:
    """Validate Wikidata QIDs/PIDs used in the mappings (syntax; optional live)."""
    from gmeow_tools.mappings import collect_wikidata_ids, load_mappings
    from gmeow_tools.wikidata import ExistenceStatus, check_existence, check_syntax

    ids = collect_wikidata_ids(load_mappings())
    syntax = check_syntax(ids)
    console.print(f"[green]✓ {len(syntax.valid)} id(s) valid syntax[/green]")
    if not syntax.ok:
        raise _fail(f"✗ invalid ids: {syntax.invalid}")
    if existence:
        try:
            statuses = check_existence(syntax.valid)
        except Exception as exc:  # network failure → visible, non-fatal skip
            err_console.print(f"[yellow]existence check skipped: {exc}[/yellow]")
            return
        bad = {k: v for k, v in statuses.items() if v is not ExistenceStatus.OK}
        for ident, status in bad.items():
            err_console.print(f"[red]{ident}: {status.value}[/red]")
        if bad:
            raise _fail(f"✗ {len(bad)} id(s) failed existence check")
        console.print(f"[green]✓ {len(statuses)} id(s) resolve on Wikidata[/green]")


@app.command()
def metadata() -> None:
    """Generate the VoID and DCAT dataset descriptions."""
    from gmeow_tools.metadata import write_metadata

    void_path, dcat_path = write_metadata()
    console.print(f"[green]✓ {void_path.relative_to(void_path.parents[1])}[/green]")
    console.print(f"[green]✓ {dcat_path.relative_to(dcat_path.parents[1])}[/green]")


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
def apache() -> None:
    """Render the Apache content-negotiation include."""
    from gmeow_tools.apache import write_conf

    path = write_conf()
    console.print(f"[green]✓ {path.relative_to(path.parents[1])}[/green]")


@app.command()
def crossref() -> None:
    """Generate the CrossRef DOI deposit XML (deposit schema 5.4.0)."""
    from gmeow_tools.config import CROSSREF_DOI_PREFIX, full_doi
    from gmeow_tools.crossref import write_deposit

    path = write_deposit()
    console.print(
        f"[green]✓ {path.relative_to(path.parents[1])} (DOI {full_doi()})[/green]"
    )
    if CROSSREF_DOI_PREFIX == "10.XXXXX":
        err_console.print(
            "[yellow]note:[/yellow] DOI prefix is a placeholder — set "
            "CROSSREF_DOI_PREFIX in config once CrossRef membership is finalized."
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
    """Build serializations, JSON-LD context, apache.conf, and exports into dist/."""
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
    from gmeow_tools.apache import write_conf
    from gmeow_tools.export import export_all
    from gmeow_tools.projections import project_examples

    conf = write_conf()
    exports = export_all()
    projected = project_examples()
    for path in (*written.values(), context, conf, *exports, *projected):
        console.print(f"[green]✓[/green] {path.relative_to(path.parents[1])}")


@app.command()
def export() -> None:
    """Generate flattened export views (CSV/CSVW, Markdown, JSONL, llms.txt).

    Pure-Python (no reasoning/Docker): projects the asserted vocabulary +
    alignments into broadly-consumable tabular and LLM-ingestable forms in dist/.
    """
    from gmeow_tools.export import export_all

    for path in export_all():
        console.print(f"[green]✓[/green] {path.relative_to(path.parents[1])}")


@app.command()
def project(
    profile: str = typer.Option(
        "all", help="Target profile: all|schema-org|geosparql|vcard|foaf."
    ),
    data: str = typer.Option(
        "", help="GMEOW data file to project (default: the worked-example fixtures)."
    ),
) -> None:
    """Project GMEOW data to a pure schema.org / GeoSPARQL / vCard / FOAF profile.

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
def rdf12() -> None:
    """Project the OWL axiom annotations into the RDF 1.2 preview (Jena, gated)."""
    from gmeow_tools import rdf12 as rdf12_mod
    from gmeow_tools import reason as reasoning
    from gmeow_tools.runner import ToolUnavailableError

    try:
        merged = reasoning.merge_release()
        out = rdf12_mod.project_rdf12(merged=merged)
    except ToolUnavailableError as exc:
        err_console.print(f"[yellow]skipped (gated): {exc}[/yellow]")
        return
    console.print(f"[green]✓ RDF 1.2 preview → {out.name}[/green]")


@app.command()
def quality(
    foops_url: str = typer.Option(
        "", "--foops-url", help="Published ontology URL to assess with FOOPS!."
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
    except Exception as exc:  # network/service failure → visible skip
        err_console.print(f"[yellow]OOPS! skipped: {exc}[/yellow]")

    if foops_url:
        try:
            result = run_foops(foops_url)
            console.print(
                f"[green]✓ FOOPS! score {result.score:.2f} "
                f"({result.checks_passed}/{result.checks_total})[/green]"
            )
        except Exception as exc:
            err_console.print(f"[yellow]FOOPS! skipped: {exc}[/yellow]")


if __name__ == "__main__":  # pragma: no cover
    app()
