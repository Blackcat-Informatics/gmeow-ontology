// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// Driving `<gmeow-console>` in a real browser.
//
// Every tool call in this lane goes through `element.ask("invoke", …)` — the element's own
// worker protocol. That is the whole point: the request is posted to the ASSEMBLED
// `engine.worker.mjs`, which resolves its transport through the emitted
// `console/pkg/mcp-transport.mjs` forwarder, boots the vendored core image over the served
// `assets/gmeow.gts`, verifies it against the served manifest, and demand-loads the
// reasoning segment when a tool needs it. Importing the transport into the page directly
// would exercise none of that, and every packaging defect this lane exists to catch lives
// in exactly the part it would skip.
//
// Service workers are BLOCKED in these contexts. The console registers one, and a
// cache-first worker that pre-caches the whole first-load tier on install would fetch every
// shell member a second time — which would make the byte measurement a measurement of the
// harness. The worker's own generated `SHELL` is asserted separately, over the emitted
// bytes.

/** The ready signal the shipped shell paints: the version chip flips when boot's status lands. */
const READY_CHIP = "engine: ready";

/**
 * Open a page at `origin`'s `/console/` and drive it to ready.
 *
 * @param browser a Playwright browser
 * @param origin  the static server's origin
 * @param options `{ path }` — the console shell's path, for the installed-package witness
 * @returns a handle carrying the page and the tool-call helpers
 */
export async function openConsole(browser, origin, { path = "/console/" } = {}) {
  const context = await browser.newContext({ serviceWorkers: "block" });
  const page = await context.newPage();
  const consoleErrors = [];
  page.on("pageerror", (error) => consoleErrors.push(String(error?.message ?? error)));
  // Every response the page received, with the transfer size the server declared. Recorded
  // from before the first navigation so a first-load measurement is the whole first load.
  const responses = [];
  page.on("response", (response) => {
    responses.push({
      url: response.url(),
      status: response.status(),
      bytes: Number(response.headers()["content-length"] ?? 0),
    });
  });
  await page.goto(`${origin}${path}`, { waitUntil: "load" });
  return handle({ context, page, consoleErrors, responses, origin, path });
}

/**
 * Open a page WITHOUT waiting for readiness — for the trees that must fail to boot.
 *
 * The negative tests need the page as it is, banner and all; waiting for a ready state that
 * will never arrive would turn a named hard error into a timeout.
 */
export async function openConsoleUnchecked(browser, origin, { path = "/console/" } = {}) {
  return openConsole(browser, origin, { path });
}

function handle({ context, page, consoleErrors, responses, origin, path }) {
  const element = () => page.locator("gmeow-console");

  return {
    page,
    context,
    origin,
    path,
    consoleErrors,
    responses,

    /** Wait until the shipped shell reports the engine ready and the derived nav is painted. */
    async ready(timeout = 240_000) {
      await page.waitForFunction(
        (chip) => document.getElementById("version-chip")?.textContent === chip,
        READY_CHIP,
        { timeout },
      );
      await page.waitForFunction(
        () => (document.getElementById("console")?.shadowRoot?.querySelectorAll("nav button").length ?? 0) > 0,
        undefined,
        { timeout },
      );
      return this;
    },

    /** The `#error-banner` text the shell is currently showing (`""` when clear). */
    async errorBanner() {
      return page.locator("#error-banner").innerText().catch(() => "");
    },

    /** Wait for `#error-banner` to carry text, and return it. */
    async waitForErrorBanner(timeout = 240_000) {
      await page.waitForFunction(
        () => (document.getElementById("error-banner")?.textContent ?? "").trim().length > 0,
        undefined,
        { timeout },
      );
      return (await page.locator("#error-banner").innerText()).trim();
    },

    /** Dispatch one worker operation and RESOLVE with its value, rejecting as the element does. */
    async ask(op, args = {}) {
      return page.evaluate(
        ([operation, payload]) => document.getElementById("console").ask(operation, payload),
        [op, args],
      );
    },

    /** Invoke one tool through the assembled worker, as a pane does. */
    async call(tool, args = {}) {
      return this.ask("invoke", { tool, args });
    },

    /**
     * Invoke one tool and report the OUTCOME rather than throwing.
     *
     * `{ok: true, value}` or `{ok: false, error}` — the shape an assertion over the whole
     * tool surface needs, so one refusal names itself instead of aborting the sweep.
     */
    async attempt(tool, args = {}) {
      return page.evaluate(
        async ([name, payload]) => {
          try {
            return { ok: true, value: await document.getElementById("console").ask("invoke", { tool: name, args: payload }) };
          } catch (error) {
            return { ok: false, error: String(error?.message ?? error) };
          }
        },
        [tool, args],
      );
    },

    /** Click one nav entry (a derived tool pane or a structural pane) by its id. */
    async selectPane(id) {
      await element().locator(`nav button[data-pane="${id}"]`).click();
    },

    /** The nav's pane ids, in rendered order. */
    async paneIds() {
      return page.evaluate(() =>
        [...document.getElementById("console").shadowRoot.querySelectorAll("nav button")].map(
          (button) => button.dataset.pane,
        ),
      );
    },

    async close() {
      await context.close();
    },
  };
}
