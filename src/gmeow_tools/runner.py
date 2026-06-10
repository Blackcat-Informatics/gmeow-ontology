"""Subprocess helpers for the pinned Docker toolchain (ROBOT, WIDOCO, Jena).

The Java tools run as pinned Docker images rather than local installs. Every
container mounts the repository at ``/work`` and runs as the invoking user
(``--user uid:gid``) so generated artifacts are never owned by root.

Two failure modes are distinguished:

* A *required* tool that is missing raises :class:`ToolUnavailableError`
  (fail-fast — ETHOS: no silent degradation).
* A *gated* tool (one a command may skip with a visible warning, e.g. WIDOCO for
  the optional rich documentation) is checked with :func:`image_available` so the
  caller can skip with a visible warning. (Jena is **not** gated — RDF 1.2 is a
  required, verified output, so the RDF 1.2 codec hard-fails without it.)
"""

from __future__ import annotations

import contextlib
import os
import shutil
import subprocess
import time
from collections.abc import Sequence
from pathlib import Path

from gmeow_tools.config import PROJECT_ROOT


class ToolUnavailableError(RuntimeError):
    """Raised when a required external tool (Docker or an image) is missing."""


class ToolExecutionError(RuntimeError):
    """Raised when an external tool runs but exits non-zero."""

    def __init__(self, command: Sequence[str], returncode: int, output: str) -> None:
        """Store the failed command, its exit code, and combined output."""
        self.command = list(command)
        self.returncode = returncode
        self.output = output
        super().__init__(
            f"command exited {returncode}: {' '.join(self.command)}\n{output}"
        )


def docker_available(timeout: float = 15.0) -> bool:
    """Return whether the Docker CLI is on PATH and the daemon responds."""
    if shutil.which("docker") is None:
        return False
    try:
        result = subprocess.run(
            ["docker", "info"],
            capture_output=True,
            text=True,
            check=False,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        return False
    return result.returncode == 0


def image_available(image: str, timeout: float = 15.0) -> bool:
    """Return whether a Docker image is present locally (does not pull)."""
    if shutil.which("docker") is None:
        return False
    try:
        result = subprocess.run(
            ["docker", "image", "inspect", image],
            capture_output=True,
            text=True,
            check=False,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        return False
    return result.returncode == 0


def pull_image(image: str, timeout: float = 300.0) -> None:
    """Pull a Docker image, raising :class:`ToolUnavailableError` on failure."""
    if not docker_available():
        raise ToolUnavailableError("Docker is not available")
    try:
        result = subprocess.run(
            ["docker", "pull", image],
            capture_output=True,
            text=True,
            check=False,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired as exc:
        raise ToolUnavailableError(
            f"could not pull {image}: timed out after {timeout}s"
        ) from exc
    if result.returncode != 0:
        raise ToolUnavailableError(f"could not pull {image}: {result.stderr.strip()}")


def _user_spec() -> str:
    """Return the ``uid:gid`` of the current process for ``docker --user``."""
    return f"{os.getuid()}:{os.getgid()}"


def _container_name() -> str:
    """Return a unique container name for the current process."""
    return f"gmeow-{os.getpid()}-{time.monotonic_ns()}"


def run_container(
    image: str,
    args: Sequence[str],
    *,
    workdir: Path = PROJECT_ROOT,
    network: bool = False,
    hostname: str | None = None,
    check: bool = True,
    timeout: float | None = 900,
) -> subprocess.CompletedProcess[str]:
    """Run a command in a pinned Docker image with the repo mounted at /work.

    Args:
        image: The pinned image reference (see ``config``).
        args: The command and arguments to run inside the container.
        workdir: Host directory to mount at ``/work`` (defaults to the repo root).
        network: If ``False`` (default), run with ``--network none`` for
            hermetic, deterministic builds. Set ``True`` only for steps that
            must reach the network.
        hostname: Fix the container hostname. Some JVM tools (Apache Jena) call
            ``InetAddress.getLocalHost()`` during init, which throws under
            ``--network none`` when the random container hostname is unresolvable;
            passing ``"localhost"`` makes it resolve via ``/etc/hosts`` while
            staying fully hermetic.
        check: Raise :class:`ToolExecutionError` on a non-zero exit.
        timeout: Seconds before the container is killed.

    Returns:
        The completed process (stdout/stderr captured as text).

    Raises:
        ToolUnavailableError: If Docker or the image is unavailable.
        ToolExecutionError: If ``check`` is set and the command exits non-zero,
            or if the container times out.
    """
    if not docker_available():
        raise ToolUnavailableError("Docker is not available")
    if not image_available(image):
        raise ToolUnavailableError(f"Docker image not present locally: {image}")

    name = _container_name()
    docker_cmd = [
        "docker",
        "run",
        "--rm",
        "--name",
        name,
        "--user",
        _user_spec(),
        "--volume",
        f"{workdir}:/work",
        "--workdir",
        "/work",
    ]
    if hostname is not None:
        docker_cmd += ["--hostname", hostname]
    if not network:
        docker_cmd += ["--network", "none"]
    docker_cmd.append(image)
    docker_cmd += list(args)

    try:
        result = subprocess.run(
            docker_cmd,
            capture_output=True,
            text=True,
            check=False,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired as exc:
        # The docker client died but the container keeps running.
        # Kill it explicitly so --rm fires.
        with contextlib.suppress(Exception):
            subprocess.run(
                ["docker", "kill", name],
                capture_output=True,
                text=True,
                check=False,
                timeout=10.0,
            )
        # Ensure removal even if kill failed.
        with contextlib.suppress(Exception):
            subprocess.run(
                ["docker", "rm", "-f", name],
                capture_output=True,
                text=True,
                check=False,
                timeout=10.0,
            )
        _stdout = (
            exc.stdout.decode(errors="replace")
            if isinstance(exc.stdout, bytes)
            else (exc.stdout or "")
        )
        _stderr = (
            exc.stderr.decode(errors="replace")
            if isinstance(exc.stderr, bytes)
            else (exc.stderr or "")
        )
        raise ToolExecutionError(
            docker_cmd,
            -1,
            f"container timed out after {timeout}s: {exc}\n"
            f"stdout: {_stdout}\n"
            f"stderr: {_stderr}",
        ) from exc

    if check and result.returncode != 0:
        raise ToolExecutionError(
            docker_cmd, result.returncode, result.stdout + result.stderr
        )
    return result
