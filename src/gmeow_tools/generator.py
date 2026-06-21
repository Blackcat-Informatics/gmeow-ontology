"""Unified generator framework for GMEOW artifact producers.

Every committed generated artifact is produced by a registered :class:`Generator`.
The framework provides, for free, for every registered generator:

- staging-dir lifecycle (``.gmeow-tmp-`` prefix + gitignore entry)
- post-render validation before any write reaches the tree
- atomic write, ``--check`` drift mode, **orphan detection**
- the ``GENERATED … DO NOT EDIT`` banner
- derivation of ``REGENERATED_PATHS``, ``make regenerate`` ordering,
  the ``make check`` drift targets, and the CI matrix — from the registry,
  not parallel hand-maintenance.

(CONSTITUTION Principles 4, 7, 13)
"""

from __future__ import annotations

import concurrent.futures
import hashlib
import importlib
import inspect
import json
import logging
import os
import pkgutil
import re
import shutil
from collections import deque
from collections.abc import Sequence
from dataclasses import dataclass, field
from pathlib import Path
from typing import Protocol, runtime_checkable

from rdflib import Graph

from gmeow_tools import diagnostics
from gmeow_tools.config import PROJECT_ROOT, gmeow_temp_dir, sweep_stale_gmeow_temp_dirs
from gmeow_tools.rdf_canonical import graphs_isomorphic

#: Regex that matches a generated-file banner (loose enough to catch old formats).
_GENERATED_MARKER = re.compile(r"GENERATED\s+by\s+gmeow", re.IGNORECASE)


class GeneratorError(RuntimeError):
    """A generator produced invalid output (e.g. internal-tag leakage)."""


@runtime_checkable
class Generator(Protocol):
    """Protocol for a registered artifact producer.

    A generator declares its canonical *inputs* and committed *outputs*, then
    implements :meth:`render` to write into a staging directory that mirrors the
    repository layout.  The framework handles staging lifecycle, drift detection,
    orphan detection, atomic publishing, and the generated banner.
    """

    name: str
    is_directory_output: bool = False
    allows_internal_tags: bool = False

    @property
    def inputs(self) -> Sequence[Path]:
        """Canonical inputs that determine the source hash."""
        ...

    @property
    def outputs(self) -> Sequence[Path]:
        """Committed artifact paths produced by this generator."""
        ...

    def render(self, staging: Path) -> None:
        """Render all artifacts into *staging* (absolute paths mirroring repo root)."""
        ...

    def compare(self, fresh: Path, committed: Path) -> list[str]:
        """Return drift diagnostics for one output, or ``[]`` if equal.

        Default: byte-for-byte comparison.  Override with :func:`rdf_compare` for
        RDF artifacts where a foreign serialization of an isomorphic graph is
        itself drift (CONSTITUTION Principle 7).
        """
        if not committed.exists():
            return [f"{_rel(committed)} (missing committed file)"]
        if not fresh.exists():
            return [f"{_rel(committed)} (not produced in staging)"]
        if fresh.read_bytes() != committed.read_bytes():
            return [f"{_rel(committed)}"]
        return []


# --------------------------------------------------------------------------- #
# Registry
# --------------------------------------------------------------------------- #

_REGISTRY: dict[str, Generator] = {}


def register(gen: Generator | type[Generator]) -> Generator:
    """Register a generator.  Usable as a class decorator.

    If *gen* is a class, it is instantiated before registration.

    Raises:
        ValueError: If a generator with the same name is already registered.
    """
    if isinstance(gen, type):
        gen = gen()
    if gen.name in _REGISTRY:
        raise ValueError(f"generator {gen.name!r} already registered")
    _REGISTRY[gen.name] = gen
    return gen


def registry() -> dict[str, Generator]:
    """Return a shallow copy of the current registry (name → generator)."""
    return dict(_REGISTRY)


def _staging_rel(path: Path) -> Path:
    """Map a committed output path to its location inside a staging tree.

    If *path* is under :data:`PROJECT_ROOT`, return the subpath.
    Otherwise fall back to the final component so that tests that
    monkey-patch output directories to temp paths outside the repo still
    work.
    """
    if path.is_relative_to(PROJECT_ROOT):
        return path.relative_to(PROJECT_ROOT)
    return Path(path.name)


def all_outputs() -> set[Path]:
    """Every committed artifact path declared by all registered generators."""
    return {p.resolve() for gen in _REGISTRY.values() for p in gen.outputs}


def all_regenerated_paths() -> list[str]:
    """All committed artifact paths as repo-relative strings.

    Suitable for ``git add`` and for deriving ``REGENERATED_PATHS`` in the
    Makefile.
    """
    root = PROJECT_ROOT.resolve()
    paths = {
        out.resolve().relative_to(root)
        for gen in _REGISTRY.values()
        for out in gen.outputs
    }
    return sorted(str(p) for p in paths)


def regenerate_order() -> list[str]:
    """Topological sort of registered generators by input/output dependencies.

    If generator *A* produces an output that is listed as an input of generator
    *B*, *A* is guaranteed to appear before *B*.
    """
    # Snapshot the registry once: accessing a generator's ``inputs``/``outputs``
    # can lazily import sibling modules whose ``@register`` side effects mutate
    # ``_REGISTRY``, which would otherwise raise "dictionary changed size during
    # iteration". Computing the whole order over one stable snapshot makes this
    # deterministic regardless of lazy-registration timing.
    snapshot = dict(_REGISTRY)

    # Map every declared output to the generator that produces it.
    output_to_gen: dict[Path, str] = {}
    for name, gen in snapshot.items():
        for out in gen.outputs:
            output_to_gen[out.resolve()] = name

    # Build adjacency: name -> set of names it depends on.
    deps: dict[str, set[str]] = {name: set() for name in snapshot}
    for name, gen in snapshot.items():
        for inp in gen.inputs:
            inp_resolved = inp.resolve()
            producer = output_to_gen.get(inp_resolved)
            if producer is not None and producer != name:
                deps[name].add(producer)

    # Kahn's algorithm with deterministic ordering.
    in_degree = {name: len(deps[name]) for name in snapshot}
    queue = deque(sorted(name for name, deg in in_degree.items() if deg == 0))
    result: list[str] = []

    while queue:
        name = queue.popleft()
        result.append(name)
        for other, other_deps in deps.items():
            if name in other_deps:
                in_degree[other] -= 1
                if in_degree[other] == 0:
                    queue.append(other)
        queue = deque(sorted(queue))

    if len(result) != len(snapshot):
        # Cycle detected — fall back to alphabetical so the gate still runs.
        return sorted(snapshot.keys())

    return result


# --------------------------------------------------------------------------- #
# Orchestration
# --------------------------------------------------------------------------- #


@dataclass(slots=True)
class RunReport:
    """The outcome of a generator run (write mode or ``--check`` mode)."""

    written: list[Path] = field(default_factory=list)
    drifted: list[str] = field(default_factory=list)
    orphans: list[str] = field(default_factory=list)
    problems: list[str] = field(default_factory=list)
    skipped: bool = False


def run(name: str, check: bool = False, skip_unchanged: bool = False) -> RunReport:
    """Execute one registered generator.

    1. Optionally skips rendering if inputs and implementation are unchanged.
    2. Creates a ``.gmeow-tmp-`` staging directory.
    3. Computes the source hash and exposes it on the generator as
       ``_source_hash`` for cache stamps and backward-compatible renderers.
    4. Calls ``gen.render(staging)``.
    5. Compares each declared output (fresh vs committed).
    6. Detects orphans in the output directories.
    7. In ``check`` mode: returns a report without touching the tree.
    8. In write mode: atomically copies from staging to committed paths.

    Raises:
        KeyError: If *name* is not registered.
    """
    gen = _REGISTRY[name]

    if skip_unchanged:
        stamp = _read_stamp(name)
        if stamp is not None and _stamp_matches(gen, stamp):
            # Inputs, implementation, and existing outputs are unchanged. Avoid
            # the expensive render but still check for orphaned files that a
            # human may have dropped into the generator's output directories.
            return RunReport(
                orphans=_find_orphans_from_outputs(gen),
                skipped=True,
            )

    # Expose the source hash for renderer-specific metadata and cache stamps.
    src_hash = source_hash(gen.inputs)
    object.__setattr__(gen, "_source_hash", src_hash)

    with gmeow_temp_dir() as tmp:
        staging = Path(tmp)
        gen.render(staging)

        # Internal-tag leak gate (#287): generated artifacts are projections
        # and must carry public BCP-47 tags only. The statements compilation
        # is the canonical internal form and opts out via allows_internal_tags.
        if not getattr(gen, "allows_internal_tags", False):
            leaks: list[str] = []
            for out in gen.outputs:
                root = staging / _staging_rel(out)
                files = [root] if root.is_file() else root.rglob("*")
                for f in files:
                    if not f.is_file() or f.suffix in {
                        ".png",
                        ".bin",
                        ".gts",
                        ".parquet",
                    }:
                        continue
                    try:
                        if "@x-gmeow-" in f.read_text(encoding="utf-8"):
                            leaks.append(str(f.relative_to(staging)))
                    except UnicodeDecodeError:
                        continue
            if leaks:
                raise GeneratorError(
                    f"internal x-gmeow-* language tags leaked into generated "
                    f"artifacts of {gen.name!r}: " + ", ".join(leaks)
                )

        drifted: list[str] = []
        for out in gen.outputs:
            fresh = staging / _staging_rel(out)
            drifted.extend(gen.compare(fresh, out))

        orphans = _find_orphans(gen, staging)

        if check:
            report = RunReport(drifted=drifted, orphans=orphans)
        else:
            written = _atomic_publish(staging, gen.outputs)
            for orphan in orphans:
                orphan_path = PROJECT_ROOT / orphan
                if orphan_path.exists():
                    orphan_path.unlink()
            report = RunReport(written=written, orphans=orphans)

    if skip_unchanged and (
        not check or (not report.drifted and not report.orphans and not report.problems)
    ):
        _write_stamp(name, _cache_hash(gen), _output_hash(gen))
    return report


def regenerate(
    names: Sequence[str] | None = None,
    *,
    jobs: int | None = None,
    skip_unchanged: bool = True,
) -> dict[str, RunReport]:
    """Run generators in dependency order (or the given order).

    Args:
        names: If ``None``, all registered generators are run in topologically
            sorted order.  If given, only those generators are run, in the
            order given.
        jobs: Number of parallel workers. ``1`` forces sequential execution;
            ``None`` uses a safe default based on CPU count.
        skip_unchanged: Skip generators whose inputs and implementation have
            not changed since the last successful run.

    Returns:
        Mapping of generator name to its :class:`RunReport`.
    """
    return _run_generators(names, check=False, jobs=jobs, skip_unchanged=skip_unchanged)


def check_all(
    names: Sequence[str] | None = None,
    *,
    jobs: int | None = None,
    skip_unchanged: bool = True,
) -> dict[str, RunReport]:
    """Run ``--check`` mode for generators.

    Args:
        names: If ``None``, all registered generators are checked.  Otherwise
            only the named generators are checked, in the order given.
        jobs: Number of parallel workers. ``1`` forces sequential execution;
            ``None`` uses a safe default based on CPU count.
        skip_unchanged: Skip generators whose inputs and implementation have
            not changed since the last successful run.

    Returns:
        Mapping of generator name to its :class:`RunReport`.
    """
    return _run_generators(names, check=True, jobs=jobs, skip_unchanged=skip_unchanged)


def to_diagnostics_report(
    results: dict[str, RunReport],
    *,
    tool: str = "generator",
) -> diagnostics.DiagnosticsReport:
    """Project generator ``--check`` results into the diagnostics report (#654).

    Drift, orphans, and problems are all gate-failing ``error`` findings. Each
    finding's ``tool`` is the *generator name* (``statements``, ``mappings``, …)
    so SARIF driver attribution stays per-generator, while the stable ``code`` is
    ``generator.drift`` / ``generator.orphan`` / ``generator.problem``. Because the
    statement and mapping compilers register as generators, this single mapping
    also carries their drift — no separate per-compiler fold is needed.
    """
    items: list[diagnostics.DiagnosticsFinding] = []
    for name, run_report in sorted(results.items()):
        items += [
            diagnostics.finding(
                severity="error",
                code=f"{tool}.drift",
                message=drift,
                tool=name,
            )
            for drift in run_report.drifted
        ]
        items += [
            diagnostics.finding(
                severity="error",
                code=f"{tool}.orphan",
                message=f"orphaned generated artifact: {orphan}",
                tool=name,
                path=orphan,
            )
            for orphan in run_report.orphans
        ]
        items += [
            diagnostics.finding(
                severity="error",
                code=f"{tool}.problem",
                message=problem,
                tool=name,
            )
            for problem in run_report.problems
        ]
    return diagnostics.report_from_findings(tool=tool, findings=items)


# --------------------------------------------------------------------------- #
# Parallel + incremental orchestration
# --------------------------------------------------------------------------- #


def _run_generators(
    names: Sequence[str] | None,
    *,
    check: bool,
    jobs: int | None,
    skip_unchanged: bool,
) -> dict[str, RunReport]:
    """Run the requested generators in topological levels.

    When *jobs* is greater than one, independent generators at the same
    topological level execute in parallel. A single process pool is reused
    across all levels to avoid startup/shutdown overhead. Results are returned
    in a deterministic name-sorted order.
    """
    if names is None:
        name_list = regenerate_order()
    else:
        unknown = sorted(set(names) - set(_REGISTRY))
        if unknown:
            raise ValueError(f"unknown generator: {', '.join(unknown)}")
        name_list = list(names)

    levels = _regenerate_levels(name_list)
    workers = _normalize_jobs(jobs)
    sweep_stale_gmeow_temp_dirs()
    results: dict[str, RunReport] = {}

    # No point paying process-pool overhead for a single generator.
    if workers == 1 or len(name_list) == 1:
        for level in levels:
            for name in sorted(level):
                results[name] = run(name, check=check, skip_unchanged=skip_unchanged)
        return dict(sorted(results.items()))

    with concurrent.futures.ProcessPoolExecutor(
        max_workers=workers,
        initializer=_init_worker,
    ) as executor:
        for level in levels:
            level_names = sorted(level)
            futures = {
                executor.submit(_worker_run, name, check, skip_unchanged): name
                for name in level_names
            }
            for future in concurrent.futures.as_completed(futures):
                name = futures[future]
                try:
                    results[name] = future.result()
                except Exception:
                    executor.shutdown(wait=False, cancel_futures=True)
                    raise

    return dict(sorted(results.items()))


def _regenerate_levels(names: list[str]) -> list[list[str]]:
    """Group *names* into topological levels based on declared input/output deps."""
    name_set = set(names)

    # Map every declared output to the generator that produces it.
    output_to_gen: dict[Path, str] = {}
    for name, gen in _REGISTRY.items():
        for out in gen.outputs:
            output_to_gen[out.resolve()] = name

    # Build adjacency restricted to the requested names.
    deps: dict[str, set[str]] = {name: set() for name in names}
    for name in names:
        gen = _REGISTRY[name]
        for inp in gen.inputs:
            producer = output_to_gen.get(inp.resolve())
            if producer is not None and producer in name_set and producer != name:
                deps[name].add(producer)

    # Topological sort restricted to the requested subset so dependents are
    # always ordered after their producers regardless of input order.
    in_degree = {name: len(deps[name]) for name in names}
    queue = deque(sorted(name for name, deg in in_degree.items() if deg == 0))
    topo: list[str] = []
    while queue:
        name = queue.popleft()
        topo.append(name)
        for other, other_deps in deps.items():
            if name in other_deps:
                in_degree[other] -= 1
                if in_degree[other] == 0:
                    queue.append(other)
    if len(topo) != len(names):
        # Cycle detected — fall back to a single safe level.
        return [sorted(names)]

    # Assign level = 1 + max(level of dependency); roots are level 0.
    level: dict[str, int] = {}
    for name in topo:
        level[name] = 1 + max((level[d] for d in deps[name]), default=0)

    groups: dict[int, list[str]] = {}
    for name, lv in level.items():
        groups.setdefault(lv, []).append(name)
    return [sorted(groups[lv]) for lv in sorted(groups)]


def _normalize_jobs(jobs: int | None) -> int:
    """Return a positive worker count, capping the default to avoid memory cliffs."""
    if jobs is not None:
        return max(1, jobs)
    cpus = os.cpu_count() or 1
    # Cap the default: heavy generators parse large RDF graphs, and too many
    # concurrent workers can exhaust RAM on a high-CPU host.
    return max(1, min(cpus, 8))


def _init_worker() -> None:
    """Process-pool initializer: ensure every generator module is imported."""
    _import_all_generator_modules()


def _worker_run(name: str, check: bool, skip_unchanged: bool) -> RunReport:
    """Pickle-friendly entry point used by the process pool."""
    return run(name, check=check, skip_unchanged=skip_unchanged)


def _import_all_generator_modules() -> None:
    """Import every submodule of ``gmeow_tools`` so ``@register`` side effects run."""
    import gmeow_tools

    for _, modname, ispkg in pkgutil.iter_modules(gmeow_tools.__path__):
        if ispkg or modname.startswith("_"):
            continue
        importlib.import_module(f"gmeow_tools.{modname}")


# --------------------------------------------------------------------------- #
# Source-hash stamp cache
# --------------------------------------------------------------------------- #


def _cache_hash(gen: Generator) -> str:
    """Hash of inputs plus the generator implementation, for skip decisions."""
    inputs_hash = source_hash(gen.inputs)
    impl_hash = source_hash(_generator_source_files(gen))
    h = hashlib.sha256()
    h.update(inputs_hash.encode("utf-8"))
    h.update(impl_hash.encode("utf-8"))
    return h.hexdigest()[:16]


def _generator_source_files(gen: Generator) -> list[Path]:
    """Source files that implement *gen* and the generator framework itself.

    Generators may declare additional implementation dependencies via an
    optional ``implementation_paths`` attribute (e.g. helper modules that
    affect rendered output). Changes to any of these files invalidate the
    skip-unchanged cache.
    """
    framework = Path(__file__).resolve()
    try:
        module = Path(inspect.getfile(gen.__class__)).resolve()
    except (TypeError, OSError):
        module = framework
    extra = [
        p.resolve()
        for p in getattr(gen, "implementation_paths", [])
        if isinstance(p, Path)
    ]
    return sorted({framework, module, *extra})


def _stamp_path(name: str) -> Path:
    """Path to the cached source-hash stamp for a generator."""
    return PROJECT_ROOT / ".stamps" / "generators" / f"{name}.hash"


def _read_stamp(name: str) -> dict[str, str] | None:
    """Return the cached stamp data, or ``None`` if no stamp exists."""
    path = _stamp_path(name)
    if not path.exists():
        return None
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
        if not isinstance(data, dict):
            return None
        return {str(k): str(v) for k, v in data.items()}
    except (json.JSONDecodeError, OSError):
        return None


def _write_stamp(name: str, input_hash: str, output_hash: str) -> None:
    """Persist the source-hash and output-hash stamp for a generator."""
    path = _stamp_path(name)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps({"input_hash": input_hash, "output_hash": output_hash}),
        encoding="utf-8",
    )


def _output_hash(gen: Generator) -> str:
    """Hash of the generator's committed outputs (used to detect hand edits)."""
    files: list[Path] = []
    for out in gen.outputs:
        if out.is_dir():
            files.extend(p for p in sorted(out.rglob("*")) if p.is_file())
        else:
            files.append(out)
    return source_hash(files)


def _stamp_matches(gen: Generator, stamp: dict[str, str]) -> bool:
    """Return True when the stamp matches the current inputs, code, and outputs."""
    return stamp.get("input_hash") == _cache_hash(gen) and stamp.get(
        "output_hash"
    ) == _output_hash(gen)


def _find_orphans_from_outputs(gen: Generator) -> list[str]:
    """Lightweight orphan check using declared outputs only.

    Used when a generator is skipped because its inputs are unchanged. We cannot
    compare against a staging tree (no render happened), but we can still flag
    generated files in the output directories that are not declared outputs.

    Directory outputs are skipped here; a full render is required to detect
    orphans inside a recursively-declared directory tree.
    """
    if getattr(gen, "is_directory_output", False):
        return []
    declared = {out.resolve() for out in gen.outputs}
    output_dirs = {out.parent.resolve() for out in gen.outputs}
    orphans: list[str] = []
    for dir_path in output_dirs:
        if not dir_path.exists():
            continue
        for committed_file in dir_path.iterdir():
            if not committed_file.is_file():
                continue
            if committed_file.resolve() in declared:
                continue
            if _is_generated_file(committed_file):
                orphans.append(_rel(committed_file))
    return sorted(orphans)


# --------------------------------------------------------------------------- #
# Helpers
# --------------------------------------------------------------------------- #


def _find_orphans(gen: Generator, staging: Path) -> list[str]:
    """Find committed generated files that are no longer produced.

    A file is considered an orphan when:

    1. It is inside a declared directory output, or it is a declared file output
       itself.
    2. It was *not* produced in the current staging tree.
    3. It contains a ``GENERATED by gmeow`` banner (heuristic to avoid flagging
       hand-authored companion files such as ``dsl/mappings/transforms.fno.ttl``).
    """
    orphans: list[str] = []
    is_dir_output = getattr(gen, "is_directory_output", False)

    if is_dir_output:
        # Directory output: scan the committed tree recursively.
        for out in gen.outputs:
            dir_path = out.resolve()
            if not dir_path.exists():
                continue
            for committed_file in dir_path.rglob("*"):
                if not committed_file.is_file():
                    continue
                rel = _staging_rel(committed_file.resolve())
                fresh = staging / rel
                if fresh.exists():
                    continue
                if _is_generated_file(committed_file):
                    orphans.append(str(rel))
    else:
        # File outputs: only the declared files themselves can be orphans.
        for out in gen.outputs:
            committed_file = out.resolve()
            if not committed_file.is_file():
                continue
            rel = _staging_rel(committed_file)
            fresh = staging / rel
            if fresh.exists():
                continue
            if _is_generated_file(committed_file):
                orphans.append(str(rel))

    return orphans


def _is_generated_file(path: Path) -> bool:
    """Heuristic: does *path* appear to be a generator-produced artifact?"""
    try:
        with path.open("r", encoding="utf-8", errors="ignore") as fh:
            sample = fh.read(4096)
    except OSError:
        return False
    return bool(_GENERATED_MARKER.search(sample))


def _atomic_publish(
    staging: Path, outputs: Sequence[Path], *, is_directory_output: bool = False
) -> list[Path]:
    """Copy files or directories from *staging* to committed locations."""
    written: list[Path] = []
    for out in outputs:
        fresh = staging / _staging_rel(out)
        if fresh.exists():
            out.parent.mkdir(parents=True, exist_ok=True)
            if is_directory_output or fresh.is_dir():
                if out.exists():
                    shutil.rmtree(out)
                shutil.copytree(fresh, out)
            else:
                shutil.copy2(fresh, out)
            written.append(out)
        elif out.exists():
            if is_directory_output or out.is_dir():
                shutil.rmtree(out)
            else:
                out.unlink()
    return written


def _rel(path: Path) -> str:
    try:
        return str(path.relative_to(PROJECT_ROOT))
    except ValueError:
        return str(path)


# --------------------------------------------------------------------------- #
# Source hash & banners
# --------------------------------------------------------------------------- #


def source_hash(inputs: Sequence[Path]) -> str:
    """Compute a deterministic content hash of *inputs*.

    The hash is independent of mtime (so it is stable across ``git clone``) and
    depends on the relative path, file size, and full content of every input.
    """
    h = hashlib.sha256()
    root = PROJECT_ROOT.resolve()
    for inp in sorted(p.resolve() for p in inputs):
        if not inp.exists():
            logging.warning("generator input missing: %s", inp)
            continue
        rel = str(inp.relative_to(root)) if inp.is_relative_to(root) else str(inp)
        h.update(rel.encode("utf-8"))
        h.update(str(inp.stat().st_size).encode("utf-8"))
        h.update(inp.read_bytes())
    return h.hexdigest()[:16]


_GENERATED_BANNER = (
    "# GENERATED by gmeow {name} — DO NOT EDIT.\n"
    "# https://github.com/Blackcat-Informatics/gmeow-ontology\n"
    "\n"
)


def write_text(
    path: Path,
    content: str,
    *,
    name: str = "",
    source_hash: str = "",
) -> None:
    """Write a text file, optionally prepending the generated banner.

    The banner uses ``#`` line comments, suitable for Turtle, SPARQL, YAML,
    Python, TypeScript, shell scripts, etc. ``source_hash`` is accepted for
    backward compatibility but is not embedded in generated artifacts.
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    if name:
        banner = _GENERATED_BANNER.format(name=name)
        content = banner + content
    path.write_text(content, encoding="utf-8")


def write_turtle(
    path: Path,
    graph: Graph,
    *,
    name: str = "",
    source_hash: str = "",
    tag_map: dict[str, str] | None = None,
) -> None:
    """Serialize an rdflib :class:`Graph` as Turtle with the generated banner.

    Internal ``x-gmeow-*`` language tags are retagged to public BCP-47 at this
    boundary (#287): registry artifacts are consumer-facing projections.
    ``tag_map`` lets fold-sealed callers supply the mapping explicitly — the
    default falls back to loading the merged graph, a canonical-source read
    the narrow-waist exporters must not trigger.
    """
    from gmeow_tools.language_tags import retag_graph

    path.parent.mkdir(parents=True, exist_ok=True)
    turtle = retag_graph(graph, tag_map=tag_map).serialize(format="turtle")
    if name:
        banner = _GENERATED_BANNER.format(name=name)
        turtle = banner + turtle
    path.write_text(turtle, encoding="utf-8")


# --------------------------------------------------------------------------- #
# Default comparators
# --------------------------------------------------------------------------- #


def rdf_compare(fresh: Path, committed: Path) -> list[str]:
    """Graph-isomorphism comparator for RDF/Turtle files.

    A foreign serialization of an isomorphic graph is itself drift, so a
    second writer for a canonical artifact cannot pass the gates.
    """
    rel = _rel(committed)
    if not committed.exists():
        return [f"{rel} (missing committed file)"]
    if not fresh.exists():
        return [f"{rel} (not produced in staging)"]
    try:
        a = Graph().parse(fresh, format="turtle")
        b = Graph().parse(committed, format="turtle")
    except Exception as exc:
        return [f"{rel} (parse error: {exc})"]
    if not graphs_isomorphic(a, b):
        return [f"{rel}"]
    return []
