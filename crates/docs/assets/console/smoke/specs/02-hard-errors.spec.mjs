// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

// The two negative trees: corrupted shipped bytes, and a required asset that is not there.
//
// Both are served by the same dumb static server as the pristine tree, from real
// perturbations on disk — a truncated `console/element.mjs` and a deleted
// `assets/gmeow.gts`. Neither is a mocked fetch: what the browser sees is what a static
// host would have answered.
//
// The claim under test is the console's own: a missing asset, a failed digest or an
// unavailable engine is a VISIBLE hard error. So each case asserts three things — the
// `#error-banner` (`role=alert`) carries text, the message names what broke, and the
// console did NOT reach ready. The third is what makes it a hard-error test rather than an
// error-message test: a console that painted a banner and then quietly carried on with a
// degraded surface would pass the first two.

import { expect, test } from "../lib/test.mjs";
import { openConsoleUnchecked } from "../lib/console-page.mjs";

test("hard_error_on_truncated_element — corrupted shipped bytes raise a visible hard error", async ({
  browser,
  origin,
}) => {
  const app = await openConsoleUnchecked(browser, origin, { path: "/truncated/console/" });
  try {
    const banner = await app.waitForErrorBanner();
    expect(banner.length, "the shell must announce the failure in #error-banner").toBeGreaterThan(0);
    // `role=alert` is the reason the banner is the right surface: it is announced, not
    // merely painted.
    await expect(app.page.locator("#error-banner")).toHaveAttribute("role", "alert");

    // NOT ready, and no console surface: the element never defined, so nothing rendered.
    await expect(app.page.locator("#version-chip")).toHaveText("engine: booting…");
    const panes = await app.page.evaluate(
      () => document.querySelector("gmeow-console")?.shadowRoot?.querySelectorAll("nav button").length ?? 0,
    );
    expect(panes, "a truncated element must not produce a working pane set").toBe(0);
  } finally {
    await app.close();
  }
});

test("hard_error_on_missing_asset — a removed engine asset is refused BY NAME", async ({
  browser,
  origin,
}) => {
  const removed = process.env.GMEOW_CONSOLE_SMOKE_REMOVED_ASSET;
  expect(typeof removed, "the global setup must publish which asset it removed").toBe("string");
  const name = removed.split("/").at(-1);

  const app = await openConsoleUnchecked(browser, origin, { path: "/missing/console/" });
  try {
    const banner = await app.waitForErrorBanner();
    expect(banner, "the refusal must NAME the asset that could not be loaded").toContain(name);
    await expect(app.page.locator("#version-chip")).toHaveText("engine: booting…");

    // The element rendered the failure IN PLACE too — the banner is not the only surface,
    // because a reader looking at the pane must not see a spinner that never resolves.
    await expect(app.page.locator("gmeow-console .failure")).toContainText(
      "The console could not start",
    );

    // …and a request made after the failure is refused immediately rather than parked.
    const outcome = await app.attempt("lookup_term", { term: "gmeow:ToolCall" });
    expect(outcome.ok, `a call against a failed boot must reject: ${JSON.stringify(outcome)}`).toBe(false);
  } finally {
    await app.close();
  }
});
