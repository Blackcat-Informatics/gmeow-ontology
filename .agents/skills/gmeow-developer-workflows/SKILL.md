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

- **Refresh Checked-In Generated Artifacts**:
  Regenerate all committed generated files from their canonical sources when you suspect drift:

  ```bash
  make regenerate
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
