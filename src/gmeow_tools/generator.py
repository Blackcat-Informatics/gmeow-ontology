"""Unified generator framework for GMEOW artifact producers.

Every committed generated artifact is produced by a registered :class:`Generator`.
The framework provides, for free, for every registered generator:

- staging-dir lifecycle (``.gmeow-tmp-`` prefix + gitignore entry)
- post-render validation before any write reaches the tree
- atomic write, ``--check`` drift mode, **orphan detection**
- the ``GENERATED … DO NOT EDIT`` banner with a **source-hash stamp**
- derivation of ``REGENERATED_PATHS``, ``make regenerate`` ordering,
  the ``make check`` drift targets, and the CI matrix — from the registry,
  not parallel hand-maintenance.

(CONSTITUTION Principles 4, 7, 13)
"""

from __future__ import annotations

import hashlib
import logging
import re
import shutil
from collections import deque
from collections.abc import Sequence
from dataclasses import dataclass, field
from pathlib import Path
from typing import Protocol, runtime_checkable

from rdflib import Graph
from rdflib.compare import isomorphic

from gmeow_tools.config import PROJECT_ROOT, gmeow_temp_dir, sweep_stale_gmeow_temp_dirs

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
    # Map every declared output to the generator that produces it.
    output_to_gen: dict[Path, str] = {}
    for name, gen in _REGISTRY.items():
        for out in gen.outputs:
            output_to_gen[out.resolve()] = name

    # Build adjacency: name -> set of names it depends on.
    deps: dict[str, set[str]] = {name: set() for name in _REGISTRY}
    for name, gen in _REGISTRY.items():
        for inp in gen.inputs:
            inp_resolved = inp.resolve()
            producer = output_to_gen.get(inp_resolved)
            if producer is not None and producer != name:
                deps[name].add(producer)

    # Kahn's algorithm with deterministic ordering.
    in_degree = {name: len(deps[name]) for name in _REGISTRY}
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

    if len(result) != len(_REGISTRY):
        # Cycle detected — fall back to alphabetical so the gate still runs.
        return sorted(_REGISTRY.keys())

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


def run(name: str, check: bool = False) -> RunReport:
    """Execute one registered generator.

    1. Sweeps stale temp dirs.
    2. Creates a ``.gmeow-tmp-`` staging directory.
    3. Computes the source hash and exposes it on the generator as
       ``_source_hash`` so that renderers can embed it in banners.
    4. Calls ``gen.render(staging)``.
    5. Compares each declared output (fresh vs committed).
    6. Detects orphans in the output directories.
    7. In ``check`` mode: returns a report without touching the tree.
    8. In write mode: atomically copies from staging to committed paths.

    Raises:
        KeyError: If *name* is not registered.
    """
    gen = _REGISTRY[name]
    sweep_stale_gmeow_temp_dirs()

    # Expose source hash to the generator for banner injection.
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
                f = staging / _staging_rel(out)
                if not f.is_file() or f.suffix in {".png", ".bin", ".gts", ".parquet"}:
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
            return RunReport(drifted=drifted, orphans=orphans)

        written = _atomic_publish(staging, gen.outputs)
        for orphan in orphans:
            orphan_path = PROJECT_ROOT / orphan
            if orphan_path.exists():
                orphan_path.unlink()

    return RunReport(written=written, orphans=orphans)


def regenerate(names: Sequence[str] | None = None) -> dict[str, RunReport]:
    """Run generators in dependency order (or the given order).

    Args:
        names: If ``None``, all registered generators are run in topologically
            sorted order.  If given, only those generators are run, in the
            order given.

    Returns:
        Mapping of generator name to its :class:`RunReport`.
    """
    if names is None:
        names = regenerate_order()
    else:
        for name in names:
            if name not in _REGISTRY:
                raise ValueError(f"unknown generator: {name!r}")

    results: dict[str, RunReport] = {}
    for name in names:
        results[name] = run(name)
    return results


def check_all(names: Sequence[str] | None = None) -> dict[str, RunReport]:
    """Run ``--check`` mode for generators.

    Args:
        names: If ``None``, all registered generators are checked.  Otherwise
            only the named generators are checked, in the order given.

    Returns:
        Mapping of generator name to its :class:`RunReport`.
    """
    if names is None:
        names = regenerate_order()
    else:
        for name in names:
            if name not in _REGISTRY:
                raise ValueError(f"unknown generator: {name!r}")

    results: dict[str, RunReport] = {}
    for name in names:
        results[name] = run(name, check=True)
    return results


# --------------------------------------------------------------------------- #
# Helpers
# --------------------------------------------------------------------------- #


def _find_orphans(gen: Generator, staging: Path) -> list[str]:
    """Find committed generated files that are no longer produced.

    A file is considered an orphan when:

    1. It lives in a directory that contains declared outputs of this generator.
    2. It was *not* produced in the current staging tree.
    3. It contains a ``GENERATED by gmeow`` banner (heuristic to avoid flagging
       hand-authored companion files such as ``dsl/mappings/transforms.fno.ttl``).
    """
    orphans: list[str] = []

    # Directories that this generator writes into.
    output_dirs: set[Path] = {out.parent.resolve() for out in gen.outputs}

    for dir_path in output_dirs:
        if not dir_path.exists():
            continue
        for committed_file in dir_path.iterdir():
            if not committed_file.is_file():
                continue
            # Map to the equivalent path in staging
            rel = _staging_rel(committed_file.resolve())
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


def _atomic_publish(staging: Path, outputs: Sequence[Path]) -> list[Path]:
    """Copy files from *staging* to committed locations, removing obsolete ones."""
    written: list[Path] = []
    for out in outputs:
        fresh = staging / _staging_rel(out)
        if fresh.exists():
            out.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(fresh, out)
            written.append(out)
        elif out.exists():
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
    "# Source hash: {hash}\n"
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
    Python, TypeScript, shell scripts, etc.
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    if name and source_hash:
        banner = _GENERATED_BANNER.format(name=name, hash=source_hash)
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
    if name and source_hash:
        banner = _GENERATED_BANNER.format(name=name, hash=source_hash)
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
    if not isomorphic(a, b):
        return [f"{rel}"]
    return []
