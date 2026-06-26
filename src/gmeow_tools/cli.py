"""Consumer command-line entry point for the bundled GMEOW package.

The public ``gmeow`` CLI is the PyPI-facing surface. Every command registered
here must work from the bundled ``generated/dist/gmeow.gts`` snapshot,
without the source checkout, Docker, generator inputs, or repo-local query
trees. Repository maintenance lives in :mod:`gmeow_tools.cli_dev`.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import TYPE_CHECKING, Any

import gts
import typer
from rich.console import Console
from rich.panel import Panel
from rich.table import Table

from gmeow_tools import __version__
from gmeow_tools.config import (
    GTS_GRAPH_METADATA,
    GTS_SNAPSHOT_FILE,
    NAMESPACE,
    SCHEMAS_DIR,
)
from gmeow_tools.gts_views import FoldView

if TYPE_CHECKING:
    from gmeow_rdf.compat.rdflib import Graph

    from gmeow_tools.language_tags import LangSelector

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
    """The bundled ontology snapshot."""
    return GTS_SNAPSHOT_FILE


def _default_schema_bytes() -> bytes:
    """Read the default GMEOW JSON Schema (the SHACL-derived gmeow.schema.json).

    Prefers the GTS-bundled schema (REP_SCHEMAS) so ``gmeow validate`` works
    repo-free, falling back to ``SCHEMAS_DIR / "gmeow.schema.json"`` in a dev
    checkout (#700).
    """
    from gmeow_tools import bundle

    blob = bundle.bundled_schema()
    if blob is not None:
        return blob
    # dev/repo fallback: the on-disk generated schema.
    path = SCHEMAS_DIR / "gmeow.schema.json"
    if not path.is_file():
        raise _fail(
            f"no bundled JSON Schema at {path}; pass one with --schema, "
            "or run `make regenerate` to emit generated/schemas/gmeow.schema.json"
        )
    return _read_bytes_or_fail(path)


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


def _resolve_lang(lang: str | None, view: FoldView) -> LangSelector:
    """Resolve CLI/env input against the languages present in the snapshot."""
    from gmeow_tools.language_tags import UnknownLanguageError, resolve_lang_input

    try:
        return resolve_lang_input(
            lang if lang is not None else os.environ.get("GMEOW_LANG"),
            view.tag_map(),
            available=view.available_languages(),
        )
    except UnknownLanguageError as exc:
        raise _fail(str(exc)) from exc


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
    stem = "stdin" if stdin else source.stem
    return graph, stem


@app.callback()
def main() -> None:
    """Consumer-safe GMEOW commands backed by gmeow.gts."""


@app.command()
def version() -> None:
    """Print the gmeow package version."""
    console.print(__version__)


@app.command()
def info(
    file: Path | None = typer.Argument(  # noqa: B008
        None,
        help="GTS snapshot to inspect (default: bundled gmeow.gts).",
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
        help="GTS snapshot to verify (default: bundled gmeow.gts).",
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


@app.command(name="verify-release-bundle")
def verify_release_bundle(
    bundle: Path = typer.Option(  # noqa: B008
        ...,
        "--bundle",
        exists=True,
        file_okay=True,
        dir_okay=False,
        readable=True,
        help="Signed release bundle to verify.",
    ),
    public_key: Path | None = typer.Option(  # noqa: B008
        None,
        "--public-key",
        help=(
            "Optional out-of-band trusted Ed25519 OpenPGP PUBLIC certificate. "
            "When given, the signature is checked against it, not just the "
            "bundle's embedded transport key."
        ),
    ),
) -> None:
    """Consumer verification of a signed release bundle (#673, §18).

    Verifies the COSE signature + trust policy AND walks the
    ``graph/attestations`` frames, hard-failing if any attested artifact's bytes
    are absent — so a consumer confirms exactly which checks ran over exactly
    which bytes, not merely that *something* was signed. This Python layer does
    NO verification logic; it only marshals paths + bytes into the native
    ``gmeow_native.pipeline.verify_release_bundle_native``.
    """
    try:
        import gmeow_native.pipeline as _pipeline
    except ImportError as exc:
        raise _fail(
            "✗ the native pipeline is unavailable: "
            f"`import gmeow_native.pipeline` failed ({exc}). Install or upgrade "
            "the gmeow package with native extensions, then retry."
        ) from exc

    try:
        bundle_bytes = bundle.read_bytes()
    except OSError as exc:
        raise _fail(f"✗ release bundle {bundle} is unreadable: {exc}") from exc

    expected_armor: str | None = None
    if public_key is not None:
        try:
            expected_armor = public_key.read_text(encoding="utf-8")
        except OSError as exc:
            raise _fail(f"✗ public key {public_key} is unreadable: {exc}") from exc

    try:
        signed, valid, kid, fingerprint, artifacts = (
            _pipeline.verify_release_bundle_native(bundle_bytes, expected_armor)
        )
    except ValueError as exc:
        raise _fail(f"✗ release verification failed: {exc}") from exc

    key_line = f", key {kid}" if kid else ""
    fp_line = f", fingerprint {fingerprint}" if fingerprint else ""
    console.print(
        f"[green]✓ release verified: {bundle} "
        f"({valid}/{signed} valid signature(s){key_line}{fp_line}, "
        f"{artifacts} attested artifact(s) present)[/green]"
    )


@app.command()
def describe(
    term: str = typer.Argument(
        ..., help="A GMEOW term: gmeow:X, local name, or prefix."
    ),
    gts_file: Path | None = typer.Option(  # noqa: B008
        None, "--gts", help="Describe from this .gts package instead of the bundle."
    ),
    lang: str | None = _lang_option(),
) -> None:
    """Describe a GMEOW term as useful prose from a GTS snapshot."""
    from gmeow_tools.describe import describe as _describe

    view = _bundle_view(gts_file)
    selector = _resolve_lang(lang, view)
    text, code = _describe(term, gts_file or _default_gts_file(), selector=selector)
    console.print(text)
    if code:
        raise typer.Exit(code=code)


@app.command()
def validate(
    instance: Path = typer.Argument(  # noqa: B008
        ...,
        help="Instance file to validate (.json or .yaml/.yml).",
    ),
    schema: Path | None = typer.Option(  # noqa: B008
        None,
        "--schema",
        "-s",
        help="JSON Schema to validate against (default: bundled gmeow.schema.json).",
    ),
) -> None:
    """Validate a JSON/YAML instance against a JSON Schema (the SHACL-derived one).

    The instance format is detected from its extension. Validation runs in the
    Rust ``gmeow_validate.validate_instance`` engine (#700): an empty violation
    set passes, any violation hard-fails with a non-zero exit.
    """
    import gmeow_validate

    suffix = instance.suffix.lower()
    # JSON-LD is JSON to the validator (the projector emits JSON-LD instances, and
    # docs/schema-projections.md documents a `.jsonld` example), so accept it here.
    if suffix in (".json", ".jsonld"):
        fmt = "json"
    elif suffix in (".yaml", ".yml"):
        fmt = "yaml"
    else:
        raise _fail(
            f"cannot infer format from {instance.name}: "
            "expected a .json, .jsonld, .yaml, or .yml extension"
        )

    instance_bytes = _read_bytes_or_fail(instance)
    schema_bytes = (
        _read_bytes_or_fail(schema) if schema is not None else _default_schema_bytes()
    )

    try:
        report = gmeow_validate.validate_instance(instance_bytes, fmt, schema_bytes)
    except ValueError as exc:
        raise _fail(f"validation error: {exc}") from exc

    violations = report["errors"]
    if not violations:
        console.print("[green]validation passed[/green]")
        return
    for violation in violations:
        err_console.print(f"[red]{violation}[/red]")
    raise typer.Exit(code=1)


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

    nq_path = out / "gmeow.nq"
    nq_path.write_text(gts.to_nquads(graph), encoding="utf-8")
    console.print(f"[green]wrote[/green] {nq_path}")

    plain = load_graph_from_gts(path)
    bind_prefixes(plain)
    # RDF 1.1 serializations: Turtle and N-Triples.
    for suffix, fmt in (("ttl", "turtle"), ("nt", "nt")):
        target = out / f"gmeow.{suffix}"
        plain.serialize(destination=target, format=fmt)
        console.print(f"[green]wrote[/green] {target} (RDF 1.1)")

    # RDF 1.2-star serializations: serialize from the same snapshot N-Quads
    # via the Rust pipeline serializer (repo-free; no bundle required — #699).
    import gmeow_native.pipeline as _pipeline

    nquads_bytes = gts.to_nquads(graph).encode("utf-8")
    for suffix, fmt in (("jsonld", "jsonld"), ("yamlld", "yamlld")):
        target = out / f"gmeow.{suffix}"
        data: bytes = _pipeline.serialize_yaml_ld(nquads_bytes, fmt)
        target.write_bytes(data)
        console.print(f"[green]wrote[/green] {target} (RDF 1.2-star)")


@app.command()
def project(
    source: Path | None = typer.Argument(  # noqa: B008
        None,
        help="A GMEOW data file (.ttl) to project, a transpiled .gts to filter, "
        "or '-' to emit the bundled YAML-LD-star snapshot; "
        "default: the bundled snapshot.",
    ),
    profile: str = typer.Option(
        "gmeow",
        "--profile",
        help="View/profile: gmeow, all, maximal, or a compiled vocabulary profile.",
    ),
    out: Path = typer.Option(  # noqa: B008
        _DEFAULT_OUT_ROOT / "project", "--out", "-o", help="Output directory."
    ),
    fmt: str = typer.Option(
        "turtle",
        "--format",
        "-f",
        help="Output serialization: turtle or yaml-ld (#699).",
    ),
    lang: str | None = _lang_option(),
) -> None:
    """Project GMEOW to a pure schema.org / FOAF / vCard / … profile.

    Two input kinds, both running from the bundle (no repo):

    * A **GMEOW data file** (.ttl): runs the per-profile CONSTRUCT (the FnO/EDOAL
      executor, lossy by design) — ``gmeow project mydata.ttl --profile foaf``.
    * A **.gts** snapshot (or the default bundle): a *view filter* —
      ``--profile foaf`` emits the FOAF subset already in the snapshot, ``gmeow``
      the pure-GMEOW base, ``all``/``maximal`` the whole thing.

    Use ``--format yaml-ld`` to emit the bundled RDF-1.2-star YAML-LD snapshot
    instead of a Turtle view filter (#699).
    """
    from gmeow_tools.projections import (
        GTS_VIEW_ALL,
        GTS_VIEW_GMEOW,
        PROFILES,
        project_file,
        project_gts_subset,
    )

    fmt_lower = fmt.lower()
    if fmt_lower == "yaml-ld":
        if source is not None and str(source) != "-":
            raise _fail(
                "--format yaml-ld reads the bundled snapshot only; "
                "do not pass a source file"
            )
        from gmeow_tools import bundle

        yamlld = bundle.bundled_yaml_ld().get("gmeow.yamlld")
        if yamlld is None:
            raise _fail("bundled YAML-LD snapshot not found")
        out.mkdir(parents=True, exist_ok=True)
        target = out / "gmeow.yamlld"
        target.write_bytes(yamlld)
        console.print(f"[green]wrote[/green] {target}")
        return

    if fmt_lower not in ("turtle", "ttl"):
        raise _fail(f"unknown --format: {fmt}")

    # Resolve the language selector against the input the user actually gave us
    # (the target graph), not the hard-coded bundled snapshot.
    if source is None or source.suffix.lower() == ".gts":
        view = _bundle_view(source)
    else:
        view = _bundle_view(None)
    selector = _resolve_lang(lang, view)

    # A user GMEOW data file → run the CONSTRUCT; a .gts (or the bundle) → view.
    if source is not None and source.suffix.lower() != ".gts":
        if profile not in PROFILES:
            raise _fail(f"unknown projection profile: {profile} (a vocabulary profile)")
        path = project_file(source, profile, dist_dir=out, selector=selector)
        console.print(f"[green]wrote[/green] {path}")
        return

    valid = set(PROFILES) | {GTS_VIEW_GMEOW, *GTS_VIEW_ALL}
    if profile not in valid:
        raise _fail(f"unknown view: {profile} (vocab | gmeow | all | maximal)")
    path = project_gts_subset(
        source or _default_gts_file(), profile, dist_dir=out, selector=selector
    )
    console.print(f"[green]wrote[/green] {path}")


@app.command()
def transpile(
    source: Path = typer.Argument(  # noqa: B008
        ...,
        help=(
            "A non-GMEOW source RDF file (Turtle or JSON-LD-star / YAML-LD-star), "
            "an OKF bundle directory, or '-' to read Turtle from stdin."
        ),
    ),
    out: Path | None = typer.Option(  # noqa: B008
        None, "-o", "--out", help="Output directory (default dist/transpile/<stem>/)."
    ),
    profiles: str = typer.Option(
        "all", "--profiles", help="Projection profiles for the maximal pass: all|name,…"
    ),
    floor: bool = typer.Option(
        False,
        "--floor",
        help="Use the per-term floor instead of the context-aware descent.",
    ),
    lang: str | None = _lang_option(),
) -> None:
    """Transpile consumer RDF → pure GMEOW → MAXIMAL multi-vocab (#448).

    The full pipeline: up-project the source into pure GMEOW (the context-aware
    descent), write that draft, then run MAXIMAL(G) = G + E(G) + P(G) over it —
    the canonical base, its strong-equivalence saturation, and every projection
    profile — into one fat, provenance-audited multi-vocabulary file family.
    Runs from the bundled snapshot alone (no repo); reads stdin when <source> is
    '-' (``cat src | gmeow transpile -``).
    """
    from gmeow_tools.projections import PROFILES
    from gmeow_tools.transpile import transpile as run_transpile
    from gmeow_tools.transpile import transpile_graph

    selector = _resolve_lang(lang, _bundle_view(None))

    names = None if profiles == "all" else [p.strip() for p in profiles.split(",")]
    if names is not None:
        unknown = sorted(set(names) - set(PROFILES))
        if unknown:
            raise _fail(f"unknown projection profile(s): {', '.join(unknown)}")

    # An OKF bundle directory is ingested through the Rust `gts from-okf` codec,
    # then lifted like any other consumer source (#780).
    if str(source) != "-" and source.is_dir():
        from gmeow_tools.okf_import import OkfBinaryNotFoundError, transpile_okf

        try:
            okf_result = transpile_okf(
                source, out_dir=out, profiles=names, selector=selector
            )
        except OkfBinaryNotFoundError as exc:
            raise _fail(str(exc)) from exc
        except (OSError, ValueError, RuntimeError) as exc:
            raise _fail(str(exc)) from exc
        err_console.print(
            f"[green]lifted[/green] {okf_result.lift.lifted} okf facts · "
            f"[yellow]retained[/yellow] {okf_result.lift.retained} okf annotation(s) · "
            f"[cyan]subjects[/cyan] {okf_result.lift.subjects}",
        )
        err_console.print(f"[green]draft[/green] {okf_result.draft_path}")
        for path in okf_result.transform.written:
            err_console.print(f"[green]wrote[/green] {path}")
        return

    try:
        if str(source) == "-":
            graph, stem = _read_turtle(source)
            result = transpile_graph(
                graph,
                stem,
                out_dir=out,
                profiles=names,
                descend=not floor,
                selector=selector,
            )
        elif source.suffix.lower() in (".jsonld", ".yamlld", ".yaml-ld", ".yld"):
            from gmeow_tools.yaml_ld import jsonld_star_to_graph, yaml_ld_to_graph

            suffix = source.suffix.lower()
            raw = source.read_bytes()
            graph = (
                jsonld_star_to_graph(raw)
                if suffix == ".jsonld"
                else yaml_ld_to_graph(raw)
            )
            result = transpile_graph(
                graph,
                source.stem,
                out_dir=out,
                profiles=names,
                descend=not floor,
                selector=selector,
            )
        else:
            result = run_transpile(
                source,
                out_dir=out,
                profiles=names,
                descend=not floor,
                selector=selector,
            )
    except (OSError, ValueError, SyntaxError) as exc:
        raise _fail(str(exc)) from exc

    err_console.print(
        f"[green]lifted[/green] {result.lifted} facts · "
        f"[cyan]claimed[/cyan] {result.claimed} inferred · "
        f"[magenta]context[/magenta] {result.context_resolved} by-type · "
        f"[blue]bridged[/blue] {result.tag_resolved} QID-tag · "
        f"[yellow]gap[/yellow] {result.gap_terms} · "
        f"[yellow]ambiguous[/yellow] {result.ambiguous_terms}",
    )
    err_console.print(
        f"[green]maximal[/green] asserted {result.transform.asserted} · "
        f"saturated {result.transform.saturated} · "
        f"projected {result.transform.projected} · "
        f"[dim]{result.transform.wall_clock_s:.1f}s[/dim]",
    )
    err_console.print(f"[green]draft[/green] {result.draft_path}")
    err_console.print(f"[green]gaps[/green] {result.gap_report_path}")
    for path in result.transform.written:
        err_console.print(f"[green]wrote[/green] {path}")


@app.command()
def export(
    out: Path = typer.Option(  # noqa: B008
        _DEFAULT_OUT_ROOT / "export", "--out", "-o", help="Output directory."
    ),
    file: Path | None = typer.Option(  # noqa: B008
        None, "--gts", help="GTS snapshot to export (default: bundled snapshot)."
    ),
    lang: str | None = _lang_option(),
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
    selector = _resolve_lang(lang, view)
    title, version = fold_meta(view)
    terms = collect_terms(view, selector=selector)
    out.mkdir(parents=True, exist_ok=True)
    written = [
        *write_csvs(terms, out, selector=selector),
        write_csvw(out, title=title, selector=selector),
        write_jsonl(terms, out),
        write_markdown(terms, out, title=title, version=version),
        write_llms_txt(terms, out, title=title, version=version),
        write_nquads(view, out),
        write_trig(view, out, selector=selector),
        write_statements_jsonl(view, out),
        write_skos(view, out, title=title, version=version, selector=selector),
        write_obographs(view, out, version=version, selector=selector),
        write_shex(view, out),
    ]
    for path in written:
        console.print(f"[green]wrote[/green] {path}")


@app.command()
def convert(
    source: Path = typer.Argument(  # noqa: B008
        ..., help="Input RDF document, or '-' to read from stdin."
    ),
    from_: str = typer.Option(
        ...,
        "--from",
        help=(
            "Source codec: turtle|ntriples|nquads|trig|jsonld|rdfxml|gts|owl-rdf12. "
            "(jsonld-star/yaml-ld-star and projection targets are output-only.)"
        ),
    ),
    to: str = typer.Option(
        ...,
        "--to",
        help=(
            "Target codec: any source codec plus jsonld-star|yaml-ld-star and the "
            "projections owl-dl|owl-el|datalog|n3|nemo|gufo|canonical-rdf12."
        ),
    ),
    out: Path | None = typer.Option(  # noqa: B008
        None, "--out", "-o", help="Output path (default: stdout)."
    ),
    loss_report: Path | None = typer.Option(  # noqa: B008
        None,
        "--loss-report",
        help="Write the realized loss ledger (JSON) here (default: stderr summary).",
    ),
    base_iri: str | None = typer.Option(
        None, "--base", help="Base IRI for relative-IRI resolution."
    ),
) -> None:
    """Transcode any RDF-1.2 syntax/projection to any other, recording loss.

    Every lossy conversion records what it dropped (the projection doctrine):
    the realized loss ledger lists each declared loss class and the number of
    items actually dropped from this document.
    """
    import gmeow_native.pipeline as _pipeline

    data = sys.stdin.buffer.read() if str(source) == "-" else source.read_bytes()
    try:
        out_bytes, loss_json = _pipeline.transcode(
            data, from_=from_, to=to, base_iri=base_iri
        )
    except ValueError as exc:
        raise _fail(str(exc)) from exc

    if out is not None:
        out.write_bytes(out_bytes)
        console.print(f"[green]wrote[/green] {out}")
    else:
        sys.stdout.buffer.write(out_bytes)

    if loss_report is not None:
        loss_report.write_text(loss_json)
        err_console.print(f"[green]loss[/green] {loss_report}")
    elif loss_json.strip() not in ("", "[]"):
        err_console.print(f"[yellow]loss[/yellow] {loss_json}")


@app.command(name="extract-docs")
def extract_docs(
    directory: Path = typer.Option(  # noqa: B008
        ..., "--directory", "-d", help="Output directory for the docs tree."
    ),
    file: Path | None = typer.Argument(  # noqa: B008
        None,
        help="GTS snapshot to document (default: bundled gmeow.gts).",
    ),
    force: bool = typer.Option(
        False,
        "--force",
        help="Write into a non-empty output directory.",
    ),
    lang: str | None = _lang_option(),
) -> None:
    """Extract the browsable docs tree from a GTS snapshot.

    The tree is the full ontology-docs site (per-term reference pages, slice
    guides, alignment + linkage indexes), unpacked verbatim from the
    ``ontology-docs`` blob baked into the bundle. The site is rendered natively
    at ``regenerate`` time (``gmeow_docs::render_site_lang``) and embedded, not
    re-projected here — run ``regenerate`` to refresh the stored tree.
    """
    from gmeow_tools.gts_views import extract_docs_site

    view = _bundle_view(file)
    selector = _resolve_lang(lang, view)
    try:
        extract_docs_site(view, directory, selector=selector, force=force)
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
    from gmeow_tools.crossref import lint_deposit, write_deposit
    from gmeow_tools.describe import load_graph_from_gts
    from gmeow_tools.self_desc import load_self_description_from_graph

    graph = load_graph_from_gts(
        file or _default_gts_file(), graph_names={GTS_GRAPH_METADATA}
    )
    try:
        meta = load_self_description_from_graph(graph)
    except ValueError as exc:
        raise _fail(f"self-description unavailable in GTS snapshot: {exc}") from exc

    problems = lint_deposit(meta)
    if problems:
        for problem in problems:
            err_console.print(f"[red]doi-lint[/red] {problem}")
        raise _fail(
            f"✗ {len(problems)} doi-lint problem(s) — fix metadata/gmeow-self.ttl"
        )

    path = write_deposit(path=out, meta=meta)
    console.print(f"[green]wrote[/green] {path} (DOI {meta.doi})")


@app.command(name="mcp")
def mcp_start() -> None:
    """Start the consumer-safe GMEOW MCP server (stdio transport)."""
    from gmeow_tools.mcp_server_consumer import run

    run()


@app.command(
    name="gts",
    context_settings={"allow_extra_args": True, "ignore_unknown_options": True},
)
def gts_command(ctx: typer.Context) -> None:
    """Dispatch to the external Graph Transport Substrate (GTS) CLI.

    ``gmeow gts`` is a thin shim around the ``gts`` binary installed with the
    ``gmeow-gts`` package. It forwards all arguments unchanged, automatically
    injecting the bundled snapshot path for subcommands that expect a file.
    """
    exe = shutil.which("gts")
    if exe is None:
        raise _fail(
            "gts binary not found. Install gmeow-gts: pip install gmeow-gts "
            "(or cargo install gmeow-gts, etc.)"
        )
    forwarded = list(ctx.args)
    if not forwarded:
        forwarded = ["--help"]
    elif forwarded[0] in {"info", "verify", "ls", "fold", "extract-key"}:
        tail = forwarded[1:]
        if "--" in tail:
            marker = tail.index("--")
            has_file_arg = marker + 1 < len(tail)
        else:
            has_file_arg = any(not arg.startswith("-") for arg in tail)
        if not has_file_arg:
            forwarded.insert(1, str(GTS_SNAPSHOT_FILE))
    try:
        result = subprocess.run([exe, *forwarded], check=False)
    except OSError as exc:
        raise _fail(f"failed to run gts: {exc}") from exc
    sys.exit(result.returncode)


@app.command(
    name="music",
    context_settings={"allow_extra_args": True, "ignore_unknown_options": True},
)
def music_command(ctx: typer.Context) -> None:
    """Dispatch to the gmeow-music extension CLI.

    ``gmeow music`` does not import extension code at module load time; it
    delegates to the ``gmeow-music`` console script installed with the package
    (or via ``pip install gmeow[music]``).
    """
    exe = shutil.which("gmeow-music")
    if exe is None:
        raise _fail(
            "gmeow-music not found. Install the music extra: pip install gmeow[music]"
        )
    forwarded = list(ctx.args)
    try:
        result = subprocess.run([exe, *forwarded], check=False)
    except OSError as exc:
        raise _fail(f"failed to run gmeow-music: {exc}") from exc
    sys.exit(result.returncode)


if __name__ == "__main__":  # pragma: no cover
    app()
