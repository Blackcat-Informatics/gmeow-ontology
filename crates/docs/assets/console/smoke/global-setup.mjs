// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// Everything the lane serves, built once, before any spec runs.
//
// Four trees go up behind one plain static server:
//
//   `/`           the pristine assembled console tree (`$CONSOLE_OUT`);
//   `/truncated/` the same tree with `console/element.mjs` cut off mid-file;
//   `/missing/`   the same tree with a required first-load engine asset deleted;
//   `/installed/` a scratch project into which the REAL `npm pack` tarball was installed,
//                 carrying the page the shipped README's Install section prescribes.
//
// The last one is the reason this setup does real work rather than pointing at a directory.
// A published package that cannot boot is invisible to every in-tree lane — the source tree
// resolves specifiers the tarball does not ship — so the witness has to be the tarball: it
// is packed by the package's own `prepack`, installed by name through `node_modules`, and
// loaded exactly the way the README tells a consumer to load it.
//
// Nothing here is optional and nothing degrades: a failure to assemble, pack, install or
// listen aborts the run naming what failed and what to do about it.

import { execFileSync } from "node:child_process";
import { promises as fs } from "node:fs";
import { join } from "node:path";

import { CONSOLE_PACKAGE_DIR, REPO_ROOT, consoleOut } from "./lib/paths.mjs";
import { startStaticServer } from "./lib/http-server.mjs";
import { generatedShell, perturbedTree, removeFile, shellEntryPaths, truncateFile } from "./lib/tree.mjs";

/** Where the perturbed trees, the tarball and the scratch project are built. */
const SCRATCH = join(REPO_ROOT, "target", "console-smoke");

/**
 * The console's own shipped Install snippet, read out of the ASSEMBLED README.
 *
 * Derived, never restated: the witness loads the element the way the published document
 * says to, so a README that starts prescribing something else moves this page with it.
 */
async function installSnippet(root) {
  const readme = await fs.readFile(join(root, "console", "README.md"), "utf8");
  const install = readme.slice(readme.indexOf("\n## Install"));
  const block = /```html\n([\s\S]*?)```/.exec(install);
  if (block === null) {
    throw new Error(
      "the assembled console README's Install section carries no HTML block — the " +
        "published-package witness has no prescribed way to load the element",
    );
  }
  return block[1].trim();
}

/** Pack the console package and install the tarball into a scratch project. */
async function installPackedTarball(root) {
  const project = join(SCRATCH, "installed");
  await fs.rm(project, { recursive: true, force: true });
  await fs.mkdir(project, { recursive: true });

  // The REAL tarball: `npm pack` runs the package's own `prepack`, which stages the engine
  // payload out of `gmeow-dev console-assemble`. A tarball built any other way would not be
  // the artifact a consumer installs.
  const packed = execFileSync("npm", ["pack", "--silent", "--pack-destination", SCRATCH], {
    cwd: CONSOLE_PACKAGE_DIR,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "inherit"],
  })
    .trim()
    .split("\n")
    .at(-1);
  const tarball = join(SCRATCH, packed);
  const size = (await fs.stat(tarball)).size;
  if (size === 0) throw new Error(`npm pack produced an empty tarball at ${tarball}`);

  await fs.writeFile(
    join(project, "package.json"),
    `${JSON.stringify(
      {
        name: "gmeow-console-installed-witness",
        version: "0.0.0",
        private: true,
        type: "module",
        description:
          "Scratch project the browser smoke lane installs the packed console tarball into.",
      },
      null,
      2,
    )}\n`,
  );
  // BY NAME, through `node_modules` — the resolution a consumer gets, not a path import.
  execFileSync("npm", ["install", "--no-audit", "--no-fund", "--silent", tarball], {
    cwd: project,
    stdio: ["ignore", "inherit", "inherit"],
  });

  const snippet = await installSnippet(root);
  await fs.writeFile(
    join(project, "index.html"),
    `<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>installed @blackcatinformatics/gmeow-console</title>
  </head>
  <body>
${snippet
  .split("\n")
  .map((line) => `    ${line}`)
  .join("\n")}
  </body>
</html>
`,
  );
  return { project, tarball, snippet };
}

/** Build the two perturbed trees the negative tests drive. */
async function perturbedTrees(root) {
  const shell = await generatedShell(root);
  // The removed asset is chosen out of the GENERATED first-load set rather than named
  // here: whatever the producer says the console must have before a pane can run is what
  // gets taken away. The ontology snapshot is the one the transport fetches through its
  // verified reader, so its absence is the case where a NAMED refusal is the contract.
  const removable = shell.map(shellEntryPaths).find((entry) => entry.file.endsWith("/gmeow.gts"));
  if (removable === undefined) {
    throw new Error("the generated SHELL names no ontology snapshot — nothing to remove");
  }

  const truncated = await perturbedTree(root, join(SCRATCH, "truncated"), async (copy) => {
    // Cut the element off inside a class body: the bytes are still served, they just stop.
    const bytes = await fs.readFile(join(copy, "console", "element.mjs"));
    await truncateFile(copy, "console/element.mjs", Math.floor(bytes.length / 2));
  });
  const missing = await perturbedTree(root, join(SCRATCH, "missing"), async (copy) => {
    await removeFile(copy, removable.file);
  });
  return { truncated, missing, removed: removable };
}

export default async function globalSetup() {
  const root = consoleOut();
  await fs.mkdir(SCRATCH, { recursive: true });

  const { truncated, missing, removed } = await perturbedTrees(root);
  const { project, tarball, snippet } = await installPackedTarball(root);

  const server = await startStaticServer({
    "/": root,
    "/truncated/": truncated,
    "/missing/": missing,
    "/installed/": project,
  });

  process.env.GMEOW_CONSOLE_SMOKE_ORIGIN = server.origin;
  process.env.GMEOW_CONSOLE_SMOKE_INSTALLED = project;
  process.env.GMEOW_CONSOLE_SMOKE_TARBALL = tarball;
  process.env.GMEOW_CONSOLE_SMOKE_REMOVED_ASSET = removed.url;
  process.env.GMEOW_CONSOLE_SMOKE_INSTALL_SNIPPET = snippet;

  // eslint-disable-next-line no-console -- the lane's own provisioning report
  console.log(
    `console-smoke: serving ${root} at ${server.origin} (+ /truncated/, /missing/, ` +
      `/installed/ from ${tarball})`,
  );

  return async () => {
    await server.close();
  };
}
