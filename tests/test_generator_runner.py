"""Tests for the parallel + incremental generator runner."""

from __future__ import annotations

from pathlib import Path

import pytest

from gmeow_tools import generator as generator_mod
from gmeow_tools.generator import Generator, check_all, regenerate, register


class _DummyGenerator(Generator):
    """A generator that writes a deterministic marker file."""

    name: str = "dummy"

    def __init__(self, root: Path, marker: str = "hello") -> None:
        self._root = root
        self._marker = marker
        self.render_calls = 0

    @property
    def inputs(self) -> list[Path]:
        return [self._root / "input.txt"]

    @property
    def outputs(self) -> list[Path]:
        return [self._root / "generated" / "out.txt"]

    def render(self, staging: Path) -> None:
        self.render_calls += 1
        out = staging / "generated" / "out.txt"
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(self._marker, encoding="utf-8")


class _DependentGenerator(Generator):
    """A generator that depends on the output of _DummyGenerator."""

    name: str = "dependent"

    def __init__(self, root: Path) -> None:
        self._root = root
        self.render_calls = 0

    @property
    def inputs(self) -> list[Path]:
        return [self._root / "generated" / "out.txt"]

    @property
    def outputs(self) -> list[Path]:
        return [self._root / "generated" / "dep.txt"]

    def render(self, staging: Path) -> None:
        self.render_calls += 1
        src = self._root / "generated" / "out.txt"
        out = staging / "generated" / "dep.txt"
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(src.read_text(encoding="utf-8") + "-dep", encoding="utf-8")


@pytest.fixture
def isolated_registry(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> Path:
    """Replace the generator registry and project root with an isolated tree."""
    monkeypatch.setattr(generator_mod, "_REGISTRY", {})
    monkeypatch.setattr(generator_mod, "PROJECT_ROOT", tmp_path)
    return tmp_path


def test_regenerate_writes_artifacts(isolated_registry: Path) -> None:
    root = isolated_registry
    root.mkdir(parents=True, exist_ok=True)
    (root / "input.txt").write_text("input", encoding="utf-8")
    gen = _DummyGenerator(root)
    register(gen)

    regenerate()

    assert (root / "generated" / "out.txt").read_text(encoding="utf-8") == "hello"
    assert gen.render_calls == 1


def test_skip_unchanged_avoids_render(isolated_registry: Path) -> None:
    root = isolated_registry
    (root / "input.txt").write_text("input", encoding="utf-8")
    gen = _DummyGenerator(root)
    register(gen)

    regenerate()
    assert gen.render_calls == 1

    results = regenerate()

    assert gen.render_calls == 1
    assert results["dummy"].skipped is True


def test_input_change_invalidates_cache(isolated_registry: Path) -> None:
    root = isolated_registry
    input_file = root / "input.txt"
    input_file.write_text("first", encoding="utf-8")
    gen = _DummyGenerator(root)
    register(gen)

    regenerate()
    assert gen.render_calls == 1

    input_file.write_text("second", encoding="utf-8")
    regenerate()

    assert gen.render_calls == 2


def test_check_all_reports_drift(isolated_registry: Path) -> None:
    root = isolated_registry
    (root / "input.txt").write_text("input", encoding="utf-8")
    gen = _DummyGenerator(root)
    register(gen)

    regenerate()
    (root / "generated" / "out.txt").write_text("mutated", encoding="utf-8")

    results = check_all()

    assert results["dummy"].drifted == ["generated/out.txt"]


def test_parallel_levels_run_dependents_after_parents(
    isolated_registry: Path,
) -> None:
    root = isolated_registry
    (root / "input.txt").write_text("input", encoding="utf-8")
    dummy = _DummyGenerator(root)
    dep = _DependentGenerator(root)
    register(dummy)
    register(dep)

    results = regenerate(jobs=2)

    assert results["dummy"].skipped is False
    assert results["dependent"].skipped is False
    assert (root / "generated" / "dep.txt").read_text(encoding="utf-8") == "hello-dep"
