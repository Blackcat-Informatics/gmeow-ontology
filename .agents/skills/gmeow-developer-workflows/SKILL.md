---
name: gmeow-developer-workflows
description: >-
  Helps run developer workflows in the gmeow-ontology repository.
  Use when you need to sync dependencies, format code, run lints, run tests, or execute the quality gate check.
---

# GMEOW Developer Workflows

This skill guides the agent in running common development task runner commands.

## Guidelines

- All commands must be executed using the `Makefile` targets.
- Ensure `uv` is installed and environment is synchronized first.

## Actionable Instructions

- **Synchronize Environment**:

  ```bash
  make install
  ```

- **Code Formatting ( Ruff )**:

  ```bash
  make fmt
  ```

- **Linting & Typing**:

  ```bash
  make lint
  ```

- **Run Unit and Competency Tests**:

  ```bash
  make test
  ```

- **Run Full Gate Check**:
  Always run the full quality gate check before proposing a commit or submitting a pull request:

  ```bash
  make check
  ```

- **Refresh Generated Artifacts**:
  `make check` above already materializes them (its DAG runs the single producer in
  update mode before anything reads `generated/`), so refreshing is not a separate
  step. When you want the artifacts WITHOUT the gate — a bootstrap, a docs render —
  drive the producer directly; never do both, since they share one host-global lock:

  ```bash
  make check-sync SYNC_MODE=update                      # artifacts only
  make check-sync SYNC_MODE=update SYNC_OUTPUTS=docs    # + external docs fanout
  ```

- **Commit with Auto-Refresh**:
  Regenerate all checked-in generated artifacts, stage them, and commit in one step:

  ```bash
  make commit
  make commit MESSAGE="feat: add foaf alignment"  # custom message
  ```

  `make commit` only stages the generated artifacts. If you also modified canonical source files, stage them with `git add` before running `make commit`, or amend the commit afterward.

- **Clean Generated Artifacts**:

  ```bash
  make clean
  ```
