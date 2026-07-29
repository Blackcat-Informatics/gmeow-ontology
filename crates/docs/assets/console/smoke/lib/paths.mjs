// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// Where the browser smoke lane reads its inputs from, and the ONE place each path is
// spelled.
//
// The lane drives the ASSEMBLED tree (`$CONSOLE_OUT`), never the build-input tree under
// `crates/docs/assets/`. That distinction is the whole point: the console that ships is the
// producer's output, and every defect this lane exists to catch — a worker importing a
// specifier that resolves only in the source tree, a generated `SHELL` that does not match
// the emitted bytes, a package whose engine payload is absent — is invisible when the
// source tree is read directly.
//
// `CONSOLE_OUT` is REQUIRED and is never defaulted. `make console-smoke CONSOLE_OUT=`
// passes it through empty on purpose, and an empty value must be as hard a failure as an
// absent one: a lane that quietly picked a directory would assert against whatever tree
// happened to be lying there.

import { accessSync, constants } from "node:fs";
import { isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** This lane's own directory (`crates/docs/assets/console/smoke/`). */
export const SMOKE_DIR = fileURLToPath(new URL("../", import.meta.url));

/** The console package's source directory — the `npm pack` origin for the installed-package witness. */
export const CONSOLE_PACKAGE_DIR = resolve(SMOKE_DIR, "..");

/** The workspace root, whose `Cargo.toml` carries the version every engine must agree with. */
export const REPO_ROOT = resolve(SMOKE_DIR, "../../../../..");

/**
 * The assembled console tree the lane drives.
 *
 * @throws if `CONSOLE_OUT` is unset, empty, or does not name a directory carrying
 *   `console/index.html` — each of which is a hard failure naming the variable and the
 *   command that produces the tree.
 */
export function consoleOut() {
  const raw = process.env.CONSOLE_OUT;
  if (raw === undefined || raw.trim().length === 0) {
    throw new Error(
      "CONSOLE_OUT is unset or empty — the browser smoke lane drives the ASSEMBLED console " +
        "tree and has no default to fall back on. Run `make console` (or " +
        "`cargo run -q -p gmeow-dev-cli -- console-assemble --out <dir>`) and pass " +
        "CONSOLE_OUT=<dir>.",
    );
  }
  const root = isAbsolute(raw) ? raw : resolve(REPO_ROOT, raw);
  const shell = join(root, "console", "index.html");
  try {
    accessSync(shell, constants.R_OK);
  } catch (cause) {
    throw new Error(
      `CONSOLE_OUT=${raw} does not carry ${shell} — the assembled console tree is missing or ` +
        "incomplete. Run `make console` to assemble it.",
      { cause },
    );
  }
  return root;
}

/** The origin the lane's static HTTP server is listening on, published by the global setup. */
export function serverOrigin() {
  const origin = process.env.GMEOW_CONSOLE_SMOKE_ORIGIN;
  if (origin === undefined || origin.length === 0) {
    throw new Error(
      "GMEOW_CONSOLE_SMOKE_ORIGIN is unset — the lane's static HTTP server was not started " +
        "by the global setup, so nothing is being served",
    );
  }
  return origin;
}

/** The scratch project the published tarball was installed into, published by the global setup. */
export function installedProjectDir() {
  const dir = process.env.GMEOW_CONSOLE_SMOKE_INSTALLED;
  if (dir === undefined || dir.length === 0) {
    throw new Error(
      "GMEOW_CONSOLE_SMOKE_INSTALLED is unset — the global setup did not install the packed " +
        "console tarball, so the published-package witness has nothing to load",
    );
  }
  return dir;
}
