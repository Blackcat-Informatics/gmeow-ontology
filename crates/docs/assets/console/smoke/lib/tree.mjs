// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// Reading the assembled tree, and building the PERTURBED copies the negative tests drive.
//
// The perturbations are made on DISK, against a real copy of the assembled tree, and then
// served by the same dumb static server as the pristine one — never faked by a server that
// rewrites a response in flight. A response rewriter would prove something about the test
// harness; a truncated file on a static host is the failure a reader actually hits.
//
// The copy is a HARDLINK farm. The assembled tree is ~56 MB, almost all of it one
// `gmeow.gts` snapshot, and each negative test needs its own root; hardlinking shares the
// bytes and costs a few inodes. The perturbed file is UNLINKED before it is rewritten, so
// the new bytes land on a fresh inode and the pristine tree is untouched — which is
// checked, not assumed, by `assertPristine`.
//
// A hardlink cannot cross a filesystem, and the two roots here are chosen independently:
// the perturbed trees are built under `target/`, while the source is `$CONSOLE_OUT`, which
// `make console-smoke` documents as overridable and a caller may point at a scratch mount
// (`/tmp` is a tmpfs on most Linux hosts, `target/` is not). So the farm falls back to a
// byte COPY on `EXDEV`, announcing which strategy it used. That is a change of COST, not of
// contract: a copied tree is byte-identical to a linked one, every perturbation below is
// still applied on disk to a real file, and `assertPristine` proves the source untouched
// either way. Only the inode sharing — an optimisation — is given up.

import { promises as fs } from "node:fs";
import { dirname, join, relative, sep } from "node:path";

/** Every file under `root`, as tree-relative POSIX paths, sorted. */
export async function listFiles(root) {
  const out = [];
  const walk = async (dir) => {
    for (const entry of await fs.readdir(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) await walk(path);
      else out.push(relative(root, path).split(sep).join("/"));
    }
  };
  await walk(root);
  return out.sort();
}

/**
 * Hardlink every file under `from` into `to`, creating directories as needed.
 *
 * The first `EXDEV` — `from` and `to` are on different filesystems, which is what
 * `CONSOLE_OUT=/tmp/…` against a `target/` on disk produces — switches the whole farm to
 * `copyFile` and says so once, naming both roots. Any other error is the caller's to see.
 *
 * @returns `"link"` or `"copy"` — the strategy the tree was actually built with.
 */
export async function hardlinkTree(from, to) {
  let strategy = "link";
  for (const name of await listFiles(from)) {
    const source = join(from, name);
    const target = join(to, name);
    await fs.mkdir(dirname(target), { recursive: true });
    if (strategy === "copy") {
      await fs.copyFile(source, target);
      continue;
    }
    try {
      await fs.link(source, target);
    } catch (error) {
      if (error?.code !== "EXDEV") throw error;
      strategy = "copy";
      // eslint-disable-next-line no-console -- the lane reports the strategy it fell back to
      console.log(
        `console-smoke: ${from} and ${to} are on different filesystems (EXDEV) — building the ` +
          "perturbed trees by copy instead of by hardlink. Same bytes, more of them.",
      );
      await fs.copyFile(source, target);
    }
  }
  return strategy;
}

/** Every file under `root` as `path → size`, the shape [`assertPristine`] compares. */
async function fileSizes(root) {
  const sizes = new Map();
  for (const name of await listFiles(root)) {
    sizes.set(name, (await fs.stat(join(root, name))).size);
  }
  return sizes;
}

/**
 * Fail if `root`'s file set or any file's size changed since `before`.
 *
 * The perturbations write through a hardlink farm, and writing INTO a hardlink writes into
 * the pristine tree the whole positive lane drives. That is a silent, whole-run corruption,
 * so it is checked rather than argued about.
 *
 * @throws naming the first path whose bytes moved.
 */
async function assertPristine(root, before) {
  const after = await fileSizes(root);
  for (const [name, size] of before) {
    const now = after.get(name);
    if (now === undefined) {
      throw new Error(`perturbing a copy of ${root} DELETED ${name} from the pristine tree`);
    }
    if (now !== size) {
      throw new Error(
        `perturbing a copy of ${root} rewrote the pristine ${name} (${size} → ${now} bytes) — ` +
          "a perturbation wrote THROUGH a hardlink instead of replacing the inode",
      );
    }
  }
  for (const name of after.keys()) {
    if (!before.has(name)) throw new Error(`perturbing a copy of ${root} ADDED ${name} to it`);
  }
}

/**
 * A hardlinked copy of `from` at `to`, with `perturb(root)` applied to it.
 *
 * `perturb` receives the copy's root and must break exactly one thing. Use
 * [`truncateFile`] / [`removeFile`] rather than writing over a hardlink directly — and that
 * discipline is verified, not trusted: `from` is stat'd before and after, and a perturbation
 * that reached through a shared inode fails the setup naming the file it damaged.
 */
export async function perturbedTree(from, to, perturb) {
  await fs.rm(to, { recursive: true, force: true });
  const before = await fileSizes(from);
  await hardlinkTree(from, to);
  await perturb(to);
  await assertPristine(from, before);
  return to;
}

/** Replace `relPath` with `bytes` on a FRESH inode, leaving every hardlinked peer intact. */
export async function replaceFile(root, relPath, bytes) {
  const path = join(root, ...relPath.split("/"));
  await fs.unlink(path);
  await fs.writeFile(path, bytes);
}

/** Truncate `relPath` to its first `keep` bytes, on a fresh inode. */
export async function truncateFile(root, relPath, keep) {
  const path = join(root, ...relPath.split("/"));
  const original = await fs.readFile(path);
  await replaceFile(root, relPath, original.subarray(0, keep));
}

/** Delete `relPath` outright. */
export async function removeFile(root, relPath) {
  await fs.unlink(join(root, ...relPath.split("/")));
}

/**
 * The generated service worker's `SHELL` array, read back out of the ASSEMBLED tree.
 *
 * The producer generates that array from the emitted key set, so it is the single authority
 * for "what the worker pre-caches at install". Every assertion in this lane that needs the
 * pre-cached tiers reads it from here rather than restating the partition in JavaScript,
 * which would be a second source of truth for exactly the thing the producer exists to own.
 */
export async function generatedShell(root) {
  const source = await fs.readFile(join(root, "console", "sw.mjs"), "utf8");
  const open = source.indexOf("const SHELL = [");
  if (open < 0) throw new Error("the assembled console/sw.mjs declares no SHELL array");
  const start = open + "const SHELL = [".length;
  const close = source.indexOf("]", start);
  if (close < 0) throw new Error("the assembled console/sw.mjs SHELL array does not close");
  const entries = source
    .slice(start, close)
    .split(",")
    .map((entry) => entry.trim())
    .filter((entry) => entry.length > 0)
    .map((entry) => JSON.parse(entry));
  if (entries.length === 0) {
    throw new Error("the assembled console/sw.mjs SHELL array is empty — nothing would be pre-cached");
  }
  return entries;
}

/**
 * A `SHELL` entry as the site path the server answers, and the file it must equal.
 *
 * `SHELL` is written relative to `console/sw.mjs` (`./element.mjs`, `../assets/gmeow.gts`),
 * because the worker resolves it against its own URL. This turns one entry into the
 * `{ url, file }` pair the assertions need.
 */
export function shellEntryPaths(entry) {
  const url = new URL(entry, "http://127.0.0.1/console/sw.mjs");
  return { url: url.pathname, file: url.pathname.replace(/^\//, "") };
}

/** The assembled console README, as text. */
async function assembledReadme(root) {
  return fs.readFile(join(root, "console", "README.md"), "utf8");
}

/**
 * The four generated numbers, parsed out of the assembled README's measured table.
 *
 * They are separate numbers because they answer separate questions. The document used to
 * publish ONE — headed "First load — everything fetched before any pane runs" over a table
 * that also carried 8 MB of vendored purrdf, a PWA manifest and four icons, none of which a
 * page load fetches. `pageLoadTotal` is what a reader pays to open the console;
 * `precacheTotal` is what the worker stores at install, and the ceiling bounds that.
 */
export async function publishedByteBudget(root) {
  const readme = await assembledReadme(root);
  const number = (pattern, what) => {
    const found = pattern.exec(readme);
    if (found === null) {
      throw new Error(`the assembled console README publishes no ${what} — the measured byte table did not render`);
    }
    return Number(found[1].replace(/\s/g, ""));
  };
  const total = (label, what) =>
    number(new RegExp(`\\|\\s*\\*\\*${label}\\*\\*\\s*\\|\\s*\\*\\*([\\d\\s]+)\\*\\*\\s*\\|`), what);
  return {
    pageLoadTotal: total("Page-load total", "page-load total"),
    installOnlyTotal: total("Install-only total", "install-only total"),
    precacheTotal: total("Install pre-cache total", "install pre-cache total"),
    ceiling: number(/install pre-cache ceiling is \*\*([\d\s]+)\*\*/, "install pre-cache ceiling"),
  };
}

/**
 * The PAGE-LOAD table's rows, as `{ key, bytes }` — the set the producer publishes as what a
 * first visit fetches.
 *
 * Read back out of the shipped document rather than recomputed, because the assertion this
 * feeds is that the published claim and a real browser agree. A row published here that no
 * page load asks for is exactly the phantom the one-directional check let through.
 */
export async function publishedPageLoadAssets(root) {
  const readme = await assembledReadme(root);
  const start = readme.indexOf("**Page load** —");
  if (start < 0) {
    throw new Error("the assembled console README publishes no page-load table");
  }
  const next = readme.indexOf("\n**", start + "**Page load**".length);
  const section = readme.slice(start, next < 0 ? undefined : next);
  const rows = [...section.matchAll(/^\|\s*`([^`]+)`\s*\|\s*([\d\s]+?)\s*\|$/gm)].map((row) => ({
    key: row[1],
    bytes: Number(row[2].replace(/\s/g, "")),
  }));
  if (rows.length === 0) {
    throw new Error("the assembled console README's page-load table has no rows");
  }
  return rows;
}
