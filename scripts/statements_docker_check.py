#!/usr/bin/env python3
"""Repo-local Jena/ROBOT-backed statement check runner."""

from __future__ import annotations

import sys

from gmeow_tools.oracles.statements_docker_check import run_all
from gmeow_tools.runner import ToolExecutionError, ToolUnavailableError


def main() -> int:
    """Run the repo-local statement Docker checks and return an exit code."""
    try:
        completed = run_all()
    except ToolUnavailableError as exc:
        print(f"tool unavailable: {exc}", file=sys.stderr)
        return 2
    except ToolExecutionError as exc:
        print(f"statement Docker check failed:\n{exc.output}", file=sys.stderr)
        return 2
    except AssertionError as exc:
        print(f"statement Docker check failed: {exc}", file=sys.stderr)
        return 2
    for name in completed:
        print(f"ok: {name}")
    print("ok: statement Docker checks passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
