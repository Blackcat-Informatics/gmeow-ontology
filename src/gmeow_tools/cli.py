"""Consumer command-line entry point for the bundled GMEOW package.

The public ``gmeow`` CLI is the PyPI-facing surface. Every command registered
here must work from the bundled ``generated/dist/gmeow-full.gts`` snapshot,
without the source checkout, Docker, generator inputs, or repo-local query
trees. Repository maintenance lives in :mod:`gmeow_tools.cli_dev`.
"""

from __future__ import annotations

from pathlib import Path

import typer
from rich.console import Console
from rich.panel import Panel
from rich.table import Table

import gts
from gmeow_tools import __version__
from gmeow_tools.config import GTS_FULL_SNAPSHOT_FILE, NAMESPACE
from gmeow_tools.gts_views import FoldView

app = typer.Typer(
    name="gmeow",
    help="Inspect, verify, and export the bundled GMEOW ontology snapshot.",
    no_args_is_help=True,
    add_completion=False,
)
console = Console()
err_console = Console(stderr=True)

_DEFAULT_OUT_ROOT = Path("dist")


def _fail(message: str, code: int = 1) -> typer.Exit:
    """Print an error and return an Exit to raise."""
    err_console.print(f"[red]{message}[/red]")
    return typer.Exit(code=code)


def _default_gts_file() -> Path:
    """The bundled full ontology snapshot."""
    return GTS_FULL_SNAPSHOT_FILE


def _read_gts_or_fail(path: Path) -> gts.Graph:
    """Read a GTS file, converting I/O and parse errors into a CLI failure."""
    try:
        return gts.read(path.read_bytes())
    except OSError as exc:
        raise _fail(f"cannot read {path}: {exc}") from exc
    except Exception as exc:
        raise _fail(f"cannot parse GTS file {path}: {exc}") from exc


def _read_bytes_or_fail(path: Path) -> bytes:
    """Read bytes from *path* or fail with a clean CLI error."""
    try:
        return path.read_bytes()
    except OSError as exc:
        raise _fail(f"cannot read {path}: {exc}") from exc


def _bundle_view(path: Path | None = None) -> FoldView:
    """Return a fold view for the bundled or user-supplied GTS file."""
    return FoldView(_read_gts_or_fail(path or _default_gts_file()))


@app.callback()
def main() -> None:
    """Consumer-safe GMEOW commands backed by gmeow-full.gts."""


@app.command()
def version() -> None:
    """Print the gmeow package version."""
    console.print(__version__)


@app.command()
def info(
    file: Path | None = typer.Argument(  # noqa: B008
        None,
        help="GTS snapshot to inspect (default: bundled gmeow-full.gts).",
    ),
) -> None:
    """Show a summary of the bundled GMEOW ontology snapshot."""
    path = file or _default_gts_file()
    graph = _read_gts_or_fail(path)
    table = Table.grid(padding=(0, 2))
    table.add_column(style="bold")
    table.add_column(justify="right")
    table.add_row("terms", str(len(graph.terms)))
    table.add_row("quads", str(len(graph.quads)))
    table.add_row("reifiers", str(len(graph.reifiers)))
    table.add_row("annotations", str(len(graph.annotations)))
    table.add_row("docs blobs", str(len(graph.blobs)))
    table.add_row("opaque frames", str(len(graph.opaque)))
    console.print(Panel(table, title=path.name, border_style="cyan"))
    for diag in graph.diagnostics:
        err_console.print(f"[yellow]{diag.code}[/yellow]: {diag.detail}")


def _bundle_checks(graph: gts.Graph) -> list[tuple[str, bool, str]]:
    """Run Docker-free, source-free checks over a folded GTS snapshot."""
    from gmeow_tools.export import collect_terms

    view = FoldView(graph)
    terms = collect_terms(view)
    missing_label = [t.curie for t in terms if not t.label]
    missing_definition = [t.curie for t in terms if not t.definition]
    has_namespace = any(
        term.value and term.value.startswith(NAMESPACE)
        for term in graph.terms
        if term.kind is gts.TermKind.IRI
    )
    return [
        (
            "reader diagnostics",
            not graph.diagnostics,
            f"{len(graph.diagnostics)} found",
        ),
        ("ontology namespace", has_namespace, NAMESPACE),
        ("term catalog", bool(terms), f"{len(terms)} terms"),
        ("labels", not missing_label, f"{len(missing_label)} missing"),
        (
            "definitions",
            not missing_definition,
            f"{len(missing_definition)} missing",
        ),
        ("documentation blobs", bool(graph.blobs), f"{len(graph.blobs)} blobs"),
    ]


@app.command()
def verify(
    file: Path | None = typer.Argument(  # noqa: B008
        None,
        help="GTS snapshot to verify (default: bundled gmeow-full.gts).",
    ),
    trusted_key: Path | None = typer.Option(  # noqa: B008
        None,
        "--trusted-key",
        help="Out-of-band armored OpenPGP public key (overrides embedded key).",
    ),
    allow_unsigned: bool = typer.Option(
        False,
        "--allow-unsigned",
        help="Permit unsigned local snapshots; release wheels should not need this.",
    ),
) -> None:
    """Verify the bundled GTS signatures and source-free ontology checks."""
    from gts.verify import format_fingerprint, verify_file

    path = file or _default_gts_file()
    data = _read_bytes_or_fail(path)
    armored: str | None = None
    if trusted_key is not None:
        try:
            armored = trusted_key.read_text(encoding="utf-8")
        except OSError as exc:
            raise _fail(f"cannot read --trusted-key {trusted_key}: {exc}") from exc

    try:
        signature = verify_file(
            data, armored_key=armored, require_signatures=not allow_unsigned
        )
    except Exception as exc:
        raise _fail(f"verification failed: {exc}") from exc
    graph = _read_gts_or_fail(path)
    checks = _bundle_checks(graph)

    sig_table = Table.grid(padding=(0, 2))
    sig_table.add_column(style="bold")
    sig_table.add_column()
    sig_table.add_row("snapshot", str(path))
    sig_table.add_row(
        "signatures",
        f"{signature.signed} signed, {signature.valid} valid, "
        f"{signature.invalid} invalid, {signature.unverified} unverified",
    )
    if signature.fingerprint:
        sig_table.add_row("transport key", format_fingerprint(signature.fingerprint))
    if signature.emojihash:
        sig_table.add_row("emoji hash", signature.emojihash)
    if signature.emojihash_labels:
        sig_table.add_row("emoji labels", signature.emojihash_labels)
    console.print(Panel(sig_table, title="GTS Signature Verification"))
    if signature.randomart:
        console.print(signature.randomart)

    check_table = Table(title="Bundled Ontology Checks", show_header=True)
    check_table.add_column("Check")
    check_table.add_column("Status")
    check_table.add_column("Detail")
    ok = signature.ok
    for name, passed, detail in checks:
        ok = ok and passed
        status = "[green]pass[/green]" if passed else "[red]fail[/red]"
        check_table.add_row(name, status, detail)
    console.print(check_table)

    for err in signature.errors:
        err_console.print(f"[red]{err}[/red]")
    for diag in signature.diagnostics:
        err_console.print(f"[yellow]{diag.code}[/yellow]: {diag.detail}")
    if not ok:
        raise _fail("verification failed")
    console.print("[green]verification passed[/green]")


@app.command()
def describe(
    term: str = typer.Argument(
        ..., help="A GMEOW term: gmeow:X, local name, or prefix."
    ),
    gts_file: Path | None = typer.Option(  # noqa: B008
        None, "--gts", help="Describe from this .gts package instead of the bundle."
    ),
) -> None:
    """Describe a GMEOW term as useful prose from a GTS snapshot."""
    from gmeow_tools.describe import describe as _describe

    text, code = _describe(term, gts_file or _default_gts_file())
    console.print(text)
    if code:
        raise typer.Exit(code=code)


@app.command()
def build(
    out: Path = typer.Option(  # noqa: B008
        _DEFAULT_OUT_ROOT / "bundle",
        "--out",
        "-o",
        help="Output directory for derived serializations.",
    ),
    file: Path | None = typer.Option(  # noqa: B008
        None, "--gts", help="GTS snapshot to serialize (default: bundled snapshot)."
    ),
) -> None:
    """Build derived serializations from a GTS snapshot."""
    from gmeow_tools.describe import load_graph_from_gts
    from gmeow_tools.graph import bind_prefixes

    path = file or _default_gts_file()
    graph = _read_gts_or_fail(path)
    out.mkdir(parents=True, exist_ok=True)

    nq_path = out / "gmeow-full.nq"
    nq_path.write_text(gts.to_nquads(graph), encoding="utf-8")
    console.print(f"[green]wrote[/green] {nq_path}")

    plain = load_graph_from_gts(path)
    bind_prefixes(plain)
    for suffix, fmt in (("ttl", "turtle"), ("nt", "nt"), ("jsonld", "json-ld")):
        target = out / f"gmeow-full.{suffix}"
        plain.serialize(destination=target, format=fmt)
        console.print(f"[green]wrote[/green] {target}")


@app.command()
def project(
    file: Path | None = typer.Argument(  # noqa: B008
        None,
        help="GTS snapshot to filter (default: bundled gmeow-full.gts).",
    ),
    profile: str = typer.Option(
        "gmeow",
        "--profile",
        help="View/profile: gmeow, all, maximal, or a compiled vocabulary profile.",
    ),
    out: Path = typer.Option(  # noqa: B008
        _DEFAULT_OUT_ROOT / "project", "--out", "-o", help="Output directory."
    ),
) -> None:
    """Project an already-bundled snapshot to one vocabulary view."""
    from gmeow_tools.projections import (
        GTS_VIEW_ALL,
        GTS_VIEW_GMEOW,
        PROFILES,
        project_gts_subset,
    )

    valid = set(PROFILES) | {GTS_VIEW_GMEOW, *GTS_VIEW_ALL}
    if profile not in valid:
        raise _fail(f"unknown view: {profile} (vocab | gmeow | all | maximal)")
    path = project_gts_subset(file or _default_gts_file(), profile, dist_dir=out)
    console.print(f"[green]wrote[/green] {path}")


@app.command()
def export(
    out: Path = typer.Option(  # noqa: B008
        _DEFAULT_OUT_ROOT / "export", "--out", "-o", help="Output directory."
    ),
    file: Path | None = typer.Option(  # noqa: B008
        None, "--gts", help="GTS snapshot to export (default: bundled snapshot)."
    ),
) -> None:
    """Export flat consumer views from a GTS snapshot."""
    from gmeow_tools.export import (
        collect_terms,
        fold_meta,
        write_csvs,
        write_csvw,
        write_jsonl,
        write_llms_txt,
        write_markdown,
        write_nquads,
        write_obographs,
        write_shex,
        write_skos,
        write_statements_jsonl,
        write_trig,
    )

    view = _bundle_view(file)
    title, version = fold_meta(view)
    terms = collect_terms(view)
    out.mkdir(parents=True, exist_ok=True)
    written = [
        *write_csvs(terms, out),
        write_csvw(out, title=title),
        write_jsonl(terms, out),
        write_markdown(terms, out, title=title, version=version),
        write_llms_txt(terms, out, title=title, version=version),
        write_nquads(view, out),
        write_trig(view, out),
        write_statements_jsonl(view, out),
        write_skos(view, out, title=title, version=version),
        write_obographs(view, out, version=version),
        write_shex(view, out),
    ]
    for path in written:
        console.print(f"[green]wrote[/green] {path}")


@app.command()
def docs(
    directory: Path = typer.Option(  # noqa: B008
        ..., "--directory", "-d", help="Output directory for the docs tree."
    ),
    file: Path | None = typer.Argument(  # noqa: B008
        None,
        help="GTS snapshot to document (default: bundled gmeow-full.gts).",
    ),
    force: bool = typer.Option(
        False,
        "--force",
        help="Write into a non-empty output directory.",
    ),
) -> None:
    """Emit a browsable Markdown docs tree from a GTS snapshot."""
    from gmeow_tools.create_docs import create_docs

    try:
        create_docs(file or _default_gts_file(), directory, force=force)
    except FileExistsError as exc:
        raise _fail(str(exc)) from exc
    except (OSError, ValueError) as exc:
        raise _fail(f"cannot create docs tree: {exc}") from exc
    console.print(f"[green]wrote[/green] docs tree -> {directory}")


@app.command()
def crossref(
    out: Path = typer.Option(  # noqa: B008
        _DEFAULT_OUT_ROOT / "crossref-deposit.xml",
        "--out",
        "-o",
        help="Output XML path.",
    ),
    file: Path | None = typer.Option(  # noqa: B008
        None, "--gts", help="GTS snapshot with self-description metadata."
    ),
) -> None:
    """Generate CrossRef DOI deposit XML from bundled self-description data."""
    from gmeow_tools.crossref import write_deposit
    from gmeow_tools.describe import load_graph_from_gts
    from gmeow_tools.self_desc import load_self_description_from_graph

    graph = load_graph_from_gts(file or _default_gts_file())
    try:
        meta = load_self_description_from_graph(graph)
    except ValueError as exc:
        raise _fail(f"self-description unavailable in GTS snapshot: {exc}") from exc
    path = write_deposit(path=out, meta=meta)
    console.print(f"[green]wrote[/green] {path} (DOI {meta.doi})")


@app.command(name="mcp")
def mcp_start() -> None:
    """Start the consumer-safe GMEOW MCP server (stdio transport)."""
    from gmeow_tools.mcp_server_consumer import run

    run()


gts_app = typer.Typer(
    name="gts",
    help="Graph Transport Substrate (GTS) reader and transforms.",
    no_args_is_help=True,
)
app.add_typer(gts_app, name="gts")


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
    """Summarise a GTS file and any signature diagnostics."""
    from gts.verify import format_fingerprint, verify_file

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
                f"{result.invalid} invalid, {result.unverified} unverified - {status}"
            )
            if result.fingerprint:
                console.print(
                    f"transport key: [bold]"
                    f"{format_fingerprint(result.fingerprint)}[/bold]"
                )
            if result.emojihash:
                console.print(f"emoji hash:    {result.emojihash}")
            if result.emojihash_labels:
                console.print(f"emoji labels:  {result.emojihash_labels}")
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
    """Verify every embedded COSE signature against the transport key."""
    from gts.verify import format_fingerprint, verify_file

    path = file or _default_gts_file()
    data = _read_bytes_or_fail(path)
    armored: str | None = None
    if trusted_key is not None:
        try:
            armored = trusted_key.read_text(encoding="utf-8")
        except OSError as exc:
            raise _fail(f"cannot read --trusted-key {trusted_key}: {exc}") from exc
    result = verify_file(data, armored_key=armored, require_signatures=True)
    if result.fingerprint:
        console.print(
            f"transport key: [bold]{format_fingerprint(result.fingerprint)}[/bold]"
        )
        if result.emojihash:
            console.print(f"emoji hash:    {result.emojihash}")
        if result.emojihash_labels:
            console.print(f"emoji labels:  {result.emojihash_labels}")
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
    console.print("[green]verification passed[/green]")


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

    graph = _read_gts_or_fail(file or _default_gts_file())
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
    console.print(f"[green]wrote[/green] {out}")


@gts_app.command("to-nq")
def gts_to_nq(
    file: Path | None = typer.Argument(  # noqa: B008
        None,
        help="GTS file to project (default: bundled gmeow-full.gts).",
    ),
    out: Path | None = typer.Option(  # noqa: B008
        None, "--out", "-o", help="Write N-Quads here (else stdout)."
    ),
) -> None:
    """Transform a GTS file to N-Quads."""
    graph = _read_gts_or_fail(file or _default_gts_file())
    text = gts.to_nquads(graph)
    if out is None:
        console.print(text, end="")
        return
    try:
        out.write_text(text, encoding="utf-8")
    except OSError as exc:
        raise _fail(f"cannot write {out}: {exc}") from exc
    console.print(f"[green]wrote[/green] {out}")


@gts_app.command("from-rdf")
def gts_from_rdf(
    file: Path,
    out: Path | None = typer.Option(  # noqa: B008
        None, "--out", "-o", help="Output .gts path (default: <input>.gts)."
    ),
) -> None:
    """Produce a GTS dist snapshot from an RDF file."""
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
    console.print(f"[green]wrote[/green] {target} ({len(data)} bytes)")


def _gts_to_db(file: Path | None, out: Path | None, suffix: str, kind: str) -> None:
    """Shared body for the gts -> {sqlite,duckdb} transforms."""
    from gmeow_tools.gts_db import to_duckdb, to_sqlite

    path = file or _default_gts_file()
    graph = _read_gts_or_fail(path)
    target = out or (
        _DEFAULT_OUT_ROOT / path.with_suffix(suffix).name
        if file is None
        else path.with_suffix(suffix)
    )
    writer = to_sqlite if kind == "sqlite" else to_duckdb
    try:
        target.parent.mkdir(parents=True, exist_ok=True)
        writer(graph, target)
    except OSError as exc:
        raise _fail(f"cannot write {target}: {exc}") from exc
    console.print(f"[green]wrote[/green] {target}")


@gts_app.command("to-sqlite")
def gts_to_sqlite(
    file: Path | None = typer.Argument(  # noqa: B008
        None,
        help="GTS file to convert (default: bundled gmeow-full.gts).",
    ),
    out: Path | None = typer.Option(  # noqa: B008
        None, "--out", "-o", help="Output database path."
    ),
) -> None:
    """Transform a GTS file to a SQLite database."""
    _gts_to_db(file, out, ".sqlite", "sqlite")


@gts_app.command("to-duckdb")
def gts_to_duckdb(
    file: Path | None = typer.Argument(  # noqa: B008
        None,
        help="GTS file to convert (default: bundled gmeow-full.gts).",
    ),
    out: Path | None = typer.Option(  # noqa: B008
        None, "--out", "-o", help="Output database path."
    ),
) -> None:
    """Transform a GTS file to a DuckDB database."""
    _gts_to_db(file, out, ".duckdb", "duckdb")


if __name__ == "__main__":  # pragma: no cover
    app()
