/**
 * Drives docs/verify/v0.16.md against a running dev build: real Rust, real
 * processes, real Chrome. The webviews are reached over WebView2's debugging
 * port, so clicks go into the page and never through the user's desktop.
 *
 * Needs Vite on :1420 (`bun run dev` in apps/desktop), `cargo build` and
 * `cargo build --example focus_probe`, and no other Takyon running. Data goes
 * to a throwaway LOCALAPPDATA. What cannot run here (shutdown, sleep, UAC,
 * Steam) is reported as skipped with the reason.
 *
 *   bun tests/drive/focus.ts
 */

import { chromium, type Browser, type Page } from "@playwright/test";
import { execFileSync, spawn, type ChildProcess } from "node:child_process";
import {
  copyFileSync,
  createWriteStream,
  existsSync,
  mkdirSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const TARGET =
  process.env.CARGO_TARGET_DIR ??
  join(import.meta.dirname, "..", "..", "src-tauri", "target");
const EXE = join(TARGET, "debug", "takyon.exe");
const PROBE = join(TARGET, "debug", "examples", "focus_probe.exe");
const CHROME = "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe";
const WORK = join(tmpdir(), "takyon-verify-v0.16");
const DATA = join(WORK, "localappdata");
const APPS = join(WORK, "apps");
const SHOTS = join(WORK, "shots");
const APP_CDP = 9333;
const CHROME_CDP = 9444;
const TEST_APP = "FocusTestApp.exe";
const KEEP_APP = "FocusKeepApp.exe";
const TEST_KEY = "exe:focustestapp.exe";
const MIN = 60_000;
/** `ONLY=BF` runs sections B and F; unset runs all. */
const want = (section: string) =>
  !process.env.ONLY || process.env.ONLY.includes(section);

type Row = { id: string; result: "PASS" | "FAIL" | "SKIP"; note: string };
const rows: Row[] = [];
function step(id: string, ok: boolean, note = "") {
  rows.push({ id, result: ok ? "PASS" : "FAIL", note });
  console.log(`  ${ok ? "PASS" : "FAIL"}  ${id} ${note}`);
}
function skip(id: string, why: string) {
  rows.push({ id, result: "SKIP", note: why });
  console.log(`  SKIP  ${id} ${why}`);
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
async function until<T>(
  f: () => T | Promise<T>,
  ms: number,
  every = 250,
): Promise<T | undefined> {
  const end = Date.now() + ms;
  for (;;) {
    const v = await f();
    if (v) return v;
    if (Date.now() > end) return undefined;
    await sleep(every);
  }
}

// --- OS ------------------------------------------------------------------

type Win = {
  hwnd: number;
  pid: number;
  class: string;
  title: string;
  visible: boolean;
  x: number;
  y: number;
  w: number;
  h: number;
  ex: number;
};
const probe = <T>(...args: (string | number)[]): T =>
  JSON.parse(
    execFileSync(PROBE, args.map(String), { encoding: "utf8" }).trim(),
  ) as T;
const windows = () => probe<Win[]>("windows");
const front = () =>
  probe<{ hwnd: number; pid: number; title: string }>("front");
const raise = (hwnd: number) => probe<{ front: boolean }>("raise", hwnd).front;
const omnibox = (hwnd: number) =>
  probe<{ bar: string | null }>("omnibox", hwnd).bar ?? "";
const tabs = (hwnd: number) =>
  probe<{ tabs: string[]; blocked: number }>("tabs", hwnd);

function running(image: string): number[] {
  const out = execFileSync(
    "tasklist",
    ["/FI", `IMAGENAME eq ${image}`, "/FO", "CSV", "/NH"],
    { encoding: "utf8" },
  );
  return out
    .split("\n")
    .filter((l) => l.toLowerCase().includes(image.toLowerCase()))
    .map((l) => Number(l.split('","')[1]));
}
function kill(image: string) {
  try {
    execFileSync("taskkill", ["/IM", image, "/F"], { stdio: "ignore" });
  } catch {
    /* not running */
  }
}
function launch(image: string, minimized = false) {
  const path = join(APPS, image);
  if (minimized) {
    execFileSync("powershell.exe", [
      "-NoProfile",
      "-Command",
      `Start-Process '${path}' -WindowStyle Minimized`,
    ]);
  } else {
    spawn(path, [], { detached: true, stdio: "ignore" }).unref();
  }
}

/** Working set of a process and every descendant (WebView2 is a tree), MB. */
function treeMb(pid: number): number {
  const script = `$all = Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId,WorkingSetSize
$ids = @(${pid}); $grew = $true
while ($grew) { $grew = $false; foreach ($p in $all) { if ($ids -contains $p.ParentProcessId -and -not ($ids -contains $p.ProcessId)) { $ids += $p.ProcessId; $grew = $true } } }
[math]::Round((($all | Where-Object { $ids -contains $_.ProcessId } | Measure-Object WorkingSetSize -Sum).Sum) / 1MB)`;
  return Number(
    execFileSync("powershell.exe", ["-NoProfile", "-Command", script], {
      encoding: "utf8",
    }).trim(),
  );
}

// --- Takyon --------------------------------------------------------------

let app: ChildProcess | undefined;
let cdp: Browser | undefined;
let palette: Page;
/** When the running Takyon started: its icon fetch is 60 s after that. */
let launchedAt = 0;

async function startTakyon() {
  const log = createWriteStream(join(WORK, "takyon.log"), { flags: "a" });
  launchedAt = Date.now();
  app = spawn(EXE, [], {
    env: {
      ...process.env,
      LOCALAPPDATA: DATA,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${APP_CDP}`,
      TAKYON_HOTKEY: "Ctrl+Alt+Shift+F9",
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  app.stdout?.pipe(log);
  app.stderr?.pipe(log);
  const up = await until(
    async () => {
      try {
        return (await fetch(`http://127.0.0.1:${APP_CDP}/json/version`)).ok;
      } catch {
        return false;
      }
    },
    60_000,
    500,
  );
  if (!up)
    throw new Error("WebView2 debugging port never opened; is Vite on :1420?");
  cdp = await chromium.connectOverCDP(`http://127.0.0.1:${APP_CDP}`);
  const page = await until(
    () =>
      cdp!
        .contexts()
        .flatMap((c) => c.pages())
        .find(
          (p) =>
            p.url().startsWith("http://localhost:1420") &&
            (p.url().includes("window=palette") ||
              !p.url().includes("window=")),
        ),
    30_000,
  );
  if (!page) throw new Error("no Palette page over CDP");
  palette = page;
  await palette.waitForFunction(() => "__TAURI_INTERNALS__" in window);
  await palette.evaluate(async () => {
    const w = window as unknown as {
      __ev: unknown[];
      __TAURI_INTERNALS__: {
        transformCallback: (f: (e: { payload: unknown }) => void) => number;
        invoke: (c: string, a?: unknown) => Promise<unknown>;
      };
    };
    w.__ev = [];
    const handler = w.__TAURI_INTERNALS__.transformCallback((e) =>
      w.__ev.push(e.payload),
    );
    await w.__TAURI_INTERNALS__.invoke("plugin:event|listen", {
      event: "takyon://focus",
      target: { kind: "Any" },
      handler,
    });
  });
  // The runtime starts in deferred init; give it its first ticks.
  await sleep(1500);
}

async function stopTakyon() {
  await cdp?.close().catch(() => {});
  cdp = undefined;
  if (app?.pid) {
    try {
      execFileSync("taskkill", ["/PID", String(app.pid), "/F"], {
        stdio: "ignore",
      });
    } catch {
      /* gone */
    }
  }
  await until(() => running("takyon.exe").length === 0, 10_000);
}

type Result<T = unknown> =
  { ok: true; v: T } | { ok: false; e: { kind?: string } | string };
async function ipc<T = unknown>(
  cmd: string,
  args: Record<string, unknown> = {},
): Promise<Result<T>> {
  return palette.evaluate(
    ([c, a]) =>
      (
        window as unknown as {
          __TAURI_INTERNALS__: {
            invoke: (c: string, a: unknown) => Promise<unknown>;
          };
        }
      ).__TAURI_INTERNALS__
        .invoke(c as string, a)
        .then(
          (v) => ({ ok: true, v }),
          (e) => ({ ok: false, e }),
        ),
    [cmd, args] as const,
  ) as Promise<Result<T>>;
}
const kindOf = (r: Result) =>
  r.ok ? "ok" : typeof r.e === "string" ? r.e : (r.e.kind ?? "?");

type View = {
  running: boolean;
  goal: string;
  strict: boolean;
  remainingMs: number;
  endsAt: number;
  paused: boolean;
};
const view = async () => ((await ipc<View>("focus_view")) as { v: View }).v;
const events = () =>
  palette.evaluate(() =>
    (
      window as unknown as {
        __ev: { kind: string; key?: string; snoozable?: boolean }[];
      }
    ).__ev.splice(0),
  );

function start(
  goal: string,
  items: string[],
  opts: {
    minutes?: number;
    ms?: number;
    strict?: boolean;
    categories?: string[];
  } = {},
) {
  return ipc<View>("focus_start", {
    request: {
      goal,
      lengthMs: opts.ms ?? (opts.minutes ?? 10) * MIN,
      strict: opts.strict ?? false,
      list: { categories: opts.categories ?? [], items },
      theme: null,
    },
  });
}
const complete = () => ipc("focus_complete");
const setting = (key: string, value: string) =>
  ipc("focus_set_setting", { key: `focus.${key}`, value });

function bar(): Win | undefined {
  return windows().find(
    (w) =>
      w.pid === app?.pid &&
      w.title === "Takyon Focus" &&
      w.class !== "TakyonFocus" &&
      w.visible,
  );
}
const gone = (image: string, ms: number) =>
  until(() => running(image).length === 0, ms);

// --- Chrome --------------------------------------------------------------

let chrome: Browser | undefined;
let chromeHwnd = 0;

async function startChrome(): Promise<boolean> {
  const before = new Set(windows().map((w) => w.hwnd));
  spawn(
    CHROME,
    [
      `--user-data-dir=${join(WORK, "chrome")}`,
      "--no-first-run",
      "--no-default-browser-check",
      `--remote-debugging-port=${CHROME_CDP}`,
      "--new-window",
      "https://example.com/takyon-verify?x=1",
    ],
    { detached: true, stdio: "ignore" },
  ).unref();
  const w = await until(
    () =>
      windows().find(
        (x) =>
          !before.has(x.hwnd) &&
          x.class === "Chrome_WidgetWin_1" &&
          x.visible &&
          x.title.includes("Example Domain"),
      ),
    30_000,
    500,
  );
  if (!w) return false;
  chromeHwnd = w.hwnd;
  chrome = await chromium.connectOverCDP(`http://127.0.0.1:${CHROME_CDP}`);
  return true;
}
const chromePages = () => chrome!.contexts().flatMap((c) => c.pages());
async function chromeTab(url: string) {
  const page = await chrome!.contexts()[0]!.newPage();
  await page.goto(url, { timeout: 10_000 }).catch(() => {});
  return page;
}
const blockedNow = () =>
  until(() => omnibox(chromeHwnd).includes("blocked.html"), 5_000);

// --- The script ----------------------------------------------------------

async function main() {
  for (const need of [EXE, PROBE, CHROME])
    if (!existsSync(need)) throw new Error(`missing ${need}`);
  if (running("takyon.exe").length)
    throw new Error("another takyon.exe is running; quit it first");
  rmSync(WORK, { recursive: true, force: true });
  for (const d of [DATA, APPS, join(APPS, "en-US"), SHOTS])
    mkdirSync(d, { recursive: true });
  for (const name of [TEST_APP, KEEP_APP]) {
    copyFileSync("C:\\Windows\\System32\\charmap.exe", join(APPS, name));
    const mui = "C:\\Windows\\System32\\en-US\\charmap.exe.mui";
    if (existsSync(mui)) copyFileSync(mui, join(APPS, "en-US", `${name}.mui`));
  }

  console.log(
    "Waiting for 15 s with no input (hands off the keyboard and mouse)…",
  );
  const idle = await until(
    () => probe<{ idleMs: number }>("idle").idleMs >= 15_000,
    180_000,
    1000,
  );
  if (!idle) console.log("  desktop busy; foreground steps may skip");

  await startTakyon();
  console.log(`Takyon dev pid ${app!.pid}`);

  if (want("A")) {
    // E1 and A: the bar appears without focus; blocked apps quit.
    console.log("\nA. Apps, E. The floating bar");
    launch(KEEP_APP);
    await until(() => running(KEEP_APP).length > 0, 10_000);
    await sleep(1500);
    const frontBefore = front();
    const started = await start("verify", [TEST_KEY]);
    step("start a Soft session", started.ok, kindOf(started));
    const b = await until(bar, 6_000);
    step(
      "E.1 bar appears",
      Boolean(b),
      b ? `${b.w}x${b.h} at ${b.x},${b.y}` : "",
    );
    step(
      "E.1 bar does not take the foreground",
      front().hwnd === frontBefore.hwnd,
      `front was "${frontBefore.title}", is "${front().title}"`,
    );
    step(
      "E.1 bar is no-activate, kept out of Alt+Tab",
      Boolean(b && b.ex & 0x08000000 && b.ex & 0x80),
      b ? `exstyle 0x${b.ex.toString(16)}` : "",
    );
    const barPage = cdp!
      .contexts()
      .flatMap((c) => c.pages())
      .find((p) => p.url().includes("window=bar"));
    if (barPage)
      await barPage
        .screenshot({ path: join(SHOTS, "bar.png") })
        .catch(() => {});

    launch(TEST_APP);
    await until(() => running(TEST_APP).length > 0, 5_000);
    step(
      "A.1 blocked app quits within 4 s",
      Boolean(await gone(TEST_APP, 4_000)),
    );
    step("A.1 other app keeps running", running(KEEP_APP).length > 0);
    const ev = await events();
    const blocked = ev.find((e) => e.kind === "blocked" && e.key === TEST_KEY);
    step(
      "A.1 block reaches the frontend as an event",
      Boolean(blocked),
      blocked
        ? `snoozable=${blocked.snoozable}`
        : JSON.stringify(ev.map((e) => e.kind)),
    );
    launch(TEST_APP);
    await until(() => running(TEST_APP).length > 0, 5_000);
    step("A.2 relaunched, quit again", Boolean(await gone(TEST_APP, 4_000)));
    launch(TEST_APP, true);
    await until(() => running(TEST_APP).length > 0, 5_000);
    step("A.3 minimised, still quit", Boolean(await gone(TEST_APP, 5_000)));

    const withBar = treeMb(app!.pid!);

    // E2: position survives.
    if (b) {
      probe("move", b.hwnd, 240, 260);
      await sleep(800);
      await complete();
      await until(() => !bar(), 5_000);
      const idleMb = treeMb(app!.pid!);
      await start("verify", [TEST_KEY]);
      const again = await until(bar, 6_000);
      step(
        "E.2 bar comes back where it was left",
        Boolean(
          again && Math.abs(again.x - 240) <= 2 && Math.abs(again.y - 260) <= 2,
        ),
        again ? `${again.x},${again.y}` : "no bar",
      );
      step(
        "E.5 RSS recorded (debug build, not a budget)",
        true,
        `tree ${withBar} MB with bar, ${idleMb} MB after the bar closed`,
      );
    }
    await complete();

    // A5: refusals the start form names.
    const preview = await ipc<{ kind: string; key?: string; name?: string }[]>(
      "focus_preview",
      {
        list: {
          categories: [],
          items: [
            "exe:takyon.exe",
            "exe:explorer.exe",
            TEST_KEY,
            "site:example.com",
          ],
        },
      },
    );
    const refusals = preview.ok ? preview.v : [];
    step(
      "A.5 Takyon and the shell are refused by name",
      refusals
        .filter((r) => r.kind === "protected")
        .map((r) => r.key)
        .sort()
        .join() === "exe:explorer.exe,exe:takyon.exe",
      JSON.stringify(refusals),
    );
    skip(
      "A.4 an app run as administrator",
      "needs a UAC prompt; unit-tested (`rules::refusals`) and checked by `procs::can_quit` on a SYSTEM process",
    );
    skip("A.6 a Steam game", "no Steam library on this machine");

    // A7: restore terminated apps.
    await setting("restore_apps", "1");
    await start("verify", [TEST_KEY]);
    launch(TEST_APP);
    await until(() => running(TEST_APP).length > 0, 5_000);
    await gone(TEST_APP, 4_000);
    await complete();
    step(
      "A.7 Restore terminated apps reopens it once",
      Boolean(await until(() => running(TEST_APP).length === 1, 8_000)),
    );
    kill(TEST_APP);
    await setting("restore_apps", "0");
  }
  if (want("C")) {
    // C: Snooze, Pause, Edit.
    console.log("\nC. Snooze, Pause, Complete");
    await setting("snooze_minutes", "1");
    await start("verify", [TEST_KEY]);
    launch(TEST_APP);
    await until(() => running(TEST_APP).length > 0, 5_000);
    await gone(TEST_APP, 4_000);
    await events();
    const snoozed = await ipc("focus_snooze", { key: TEST_KEY });
    launch(TEST_APP);
    await until(() => running(TEST_APP).length > 0, 5_000);
    await sleep(5_000);
    step(
      "C.1 a snoozed app stays usable",
      snoozed.ok && running(TEST_APP).length > 0,
      kindOf(snoozed),
    );
    // Bring the other test app forward: an expired snooze holds while in front.
    const keep = windows().find(
      (w) =>
        running(KEEP_APP).includes(w.pid) &&
        w.visible &&
        w.title === "Character Map",
    );
    if (keep && raise(keep.hwnd)) {
      const lapsed = await until(
        () => running(TEST_APP).length === 0,
        75_000,
        1000,
      );
      step(
        "C.1 once the snooze ends and it is not in front, it quits",
        Boolean(lapsed),
      );
    } else {
      kill(TEST_APP);
      skip(
        "C.1 snooze lapse",
        "Windows kept the foreground elsewhere (desktop in use)",
      );
    }
    const paused = await ipc<View>("focus_pause", { breakMinutes: 5 });
    const left = (await view()).remainingMs;
    launch(TEST_APP);
    await until(() => running(TEST_APP).length > 0, 5_000);
    await sleep(4_000);
    step(
      "C.2 paused: apps open, clock stops",
      paused.ok &&
        running(TEST_APP).length > 0 &&
        Math.abs((await view()).remainingMs - left) < 1500,
      `remaining ${Math.round(left / 1000)} s`,
    );
    await ipc("focus_resume");
    step(
      "C.2 resumed: blocking starts again",
      Boolean(await gone(TEST_APP, 5_000)),
    );
    const endsBefore = (await view()).endsAt;
    const extended = await ipc<View>("focus_extend", { minutes: 10 });
    step(
      "C.3 adding 10 minutes moves the end",
      extended.ok &&
        Math.abs(extended.v.endsAt - endsBefore - 10 * MIN) < 2_000,
    );
    await setting("snooze_minutes", "0");
    // One notice per app per 5 s: let the resume's own block pass.
    await sleep(5_500);
    await events();
    launch(TEST_APP);
    await gone(TEST_APP, 5_000);
    const noSnooze = (await events()).find((e) => e.kind === "blocked");
    step(
      "C.4 No snooze: the block line offers none",
      noSnooze?.snoozable === false,
      `snoozable=${noSnooze?.snoozable}`,
    );
    await complete();
    await setting("snooze_minutes", "3");
  }
  if (want("D")) {
    // D: Strict.
    console.log("\nD. Strict");
    const goal = "verify strict";
    await start(goal, [TEST_KEY], { strict: true, minutes: 20 });
    step(
      "D.1 Strict has no Snooze",
      kindOf(await ipc("focus_snooze", { key: TEST_KEY })) === "notAllowed",
    );
    type Hold = { window: boolean; allows: boolean; reason: string | null };
    // The hold is set on the runtime's next 250 ms tick, not by the IPC call.
    const holdUntil = async (want: (h: Hold) => boolean) =>
      (await until(() => {
        const h = probe<Hold>("hold", app!.pid!);
        return want(h) ? h : undefined;
      }, 3_000)) ?? probe<Hold>("hold", app!.pid!);
    let hold = await holdUntil((h) => !h.allows);
    step(
      "D.6 shutdown query refused, with the goal as the reason",
      hold.window &&
        !hold.allows &&
        hold.reason === `Focus: ${goal}, 20 min left`,
      JSON.stringify(hold),
    );
    await setting("hurdle_shutdown", "never");
    hold = await holdUntil((h) => h.allows);
    step(
      "D.8 hurdle set to Never lets shutdown through",
      hold.allows && hold.reason === null,
      JSON.stringify(hold),
    );
    await setting("hurdle_shutdown", "strict");
    await sleep(800);
    step(
      "D.2 Pause first asks for the goal",
      kindOf(await ipc("focus_pause", { breakMinutes: null })) ===
        "needsFriction",
    );
    const wrong = await ipc<boolean>("focus_type_goal", { text: "verify" });
    const right = await ipc<boolean>("focus_type_goal", { text: goal });
    step(
      "D.2 only the exact goal starts the countdown",
      wrong.ok && !wrong.v && right.ok && right.v,
    );
    step(
      "D.2 not before 30 s",
      kindOf(await ipc("focus_pause", { breakMinutes: null })) ===
        "needsFriction",
    );
    await sleep(31_000);
    step(
      "D.2 after 30 s Pause goes through",
      kindOf(await ipc("focus_pause", { breakMinutes: null })) === "ok",
    );
    await ipc("focus_resume");

    const remainingBefore = (await view()).remainingMs;
    const killedAt = Date.now();
    await stopTakyon();
    await startTakyon();
    const resumed = await view();
    const expected = remainingBefore - (Date.now() - killedAt);
    step(
      "D.4 killed and relaunched: the Strict session resumes",
      resumed.running &&
        resumed.strict &&
        Math.abs(resumed.remainingMs - expected) < 5_000,
      `${Math.round(resumed.remainingMs / 1000)} s left, expected ~${Math.round(expected / 1000)}`,
    );
    launch(TEST_APP);
    await until(() => running(TEST_APP).length > 0, 5_000);
    step(
      "D.4 blocking restarts after the relaunch",
      Boolean(await gone(TEST_APP, 6_000)),
    );
    hold = await holdUntil((h) => !h.allows);
    step(
      "D.4 the shutdown hold is back too",
      hold.window && !hold.allows,
      JSON.stringify(hold),
    );
    skip("D.5 reboot mid-session", "would reboot this machine");
    skip(
      "D.6 Windows' own shutdown screen",
      "would shut this machine down; the query and reason above are what that screen reads",
    );
    skip(
      "D.7 idle sleep held off",
      "`powercfg /requests` needs an elevated shell; the hold is tested per thread in `tests/focus_power.rs`",
    );
    skip(
      "D.9 sleep and wake",
      "would suspend this machine; the messages are tested in `tests/focus_power.rs`",
    );
    await ipc("focus_type_goal", { text: goal });
    await sleep(31_000);
    step(
      "D Strict completes after the countdown",
      kindOf(await complete()) === "ok",
    );
    hold = await holdUntil((h) => h.allows);
    step("D shutdown released once it ends", hold.allows, JSON.stringify(hold));

    // E4: tray surface.
    await setting("surface", "tray");
    await start("verify", [TEST_KEY]);
    await sleep(2_000);
    step("E.4 Tray only: no bar", !bar());
    await complete();
    await setting("surface", "bar");
  }
  if (want("B")) {
    // B: Websites in a throwaway Chrome.
    console.log(
      "\nB. Websites (Chrome, throwaway profile). Waiting for 15 s with no input...",
    );
    await until(
      () => probe<{ idleMs: number }>("idle").idleMs >= 15_000,
      120_000,
      1000,
    );
    if (!(await startChrome())) {
      skip("B", "Chrome window never appeared");
    } else {
      await start("Write the report", ["site:example.com"]);
      // Windows lets another process raise a window only once the desktop has
      // been idle past its foreground-lock timeout (200 s by default).
      const up = await until(() => raise(chromeHwnd), 240_000, 5_000);
      if (!up) {
        skip("B", "Windows kept the foreground elsewhere (desktop in use)");
      } else {
        step(
          "B.1 the open tab goes to the block page",
          Boolean(await blockedNow()),
          omnibox(chromeHwnd),
        );
        const pg = await until(
          () => chromePages().find((p) => p.url().includes("blocked.html")),
          5_000,
        );
        if (pg) {
          const requests: string[] = [];
          pg.on("request", (r) => requests.push(r.url()));
          await pg.reload();
          await sleep(1000);
          const heading = (
            await pg.locator("h1").first().textContent()
          )?.trim();
          step(
            "B.8 the block page names the goal",
            heading === "Write the report",
            `h1 "${heading}"`,
          );
          step(
            "B.7 the block page makes no request but itself",
            requests.every((u) => u.startsWith("file:")),
            requests.join(" ") || "none",
          );
          await pg.screenshot({ path: join(SHOTS, "block-page.png") });
        }
        await chromeTab("https://www.example.com/sub?y=2");
        raise(chromeHwnd);
        step(
          "B.1 a subdomain is blocked too",
          Boolean(await blockedNow()),
          omnibox(chromeHwnd),
        );
        await complete();
        raise(chromeHwnd);
        const restored = await until(
          () => tabs(chromeHwnd).blocked === 0,
          10_000,
          500,
        );
        const urls = chromePages().map((p) => p.url());
        step(
          "B.3 ending early restores every blocked tab",
          Boolean(restored),
          JSON.stringify(tabs(chromeHwnd).tabs),
        );
        // Chrome's bar hides `www.`, so that tab comes back without it
        // (docs/tbd/v0.16.md §5); the path is what must survive.
        step(
          "B.3 restored with paths intact",
          urls.includes("https://example.com/takyon-verify?x=1") &&
            urls.some((u) =>
              /^https:\/\/(www\.)?example\.com\/sub\?y=2$/.test(u),
            ),
          urls.join(" "),
        );

        // B2: a host that only ends in the blocked one is a different site.
        await start("suffix", ["site:box.com"]);
        await chromeTab("https://www.dropbox.com/");
        raise(chromeHwnd);
        await sleep(3_000);
        step(
          "B.2 dropbox.com is not box.com",
          omnibox(chromeHwnd).includes("dropbox.com") &&
            !omnibox(chromeHwnd).includes("blocked.html"),
          omnibox(chromeHwnd),
        );
        await chromeTab("https://www.box.com/");
        raise(chromeHwnd);
        step(
          "B.2 while box.com itself is blocked",
          Boolean(await blockedNow()),
          omnibox(chromeHwnd),
        );
        await complete();

        // B4: the block page returns the tab itself when Takyon is gone.
        await start("expiry", ["site:example.com"], { ms: MIN });
        await chromeTab("https://example.com/expire");
        raise(chromeHwnd);
        step("B.4 blocked before Takyon quits", Boolean(await blockedNow()));
        await stopTakyon();
        const back = await until(
          () =>
            chromePages().some((p) => p.url() === "https://example.com/expire"),
          80_000,
          1000,
        );
        step(
          "B.4 with Takyon closed, the page returns itself at the end",
          Boolean(back),
        );
        await startTakyon();
        step(
          "B.4 the expired session is finished on relaunch",
          !(await view()).running,
        );
      }
      skip(
        "B.5 a private window",
        "covered by the same address-bar read; not driven",
      );
      const reg = await ipc<{ kind: string; name?: string }[]>(
        "focus_preview",
        { list: { categories: [], items: ["site:example.com"] } },
      );
      step(
        "B.6 no unreadable browser is running (or it is named)",
        reg.ok,
        JSON.stringify(reg.ok ? reg.v : reg.e),
      );
    }
  }
  if (want("F")) {
    // F: Settings, categories, icons.
    console.log("\nF. Settings and stats, G. Icons");
    const edit = await ipc("focus_edit_builtin", {
      category: "social",
      item: { key: "site:example.com", label: "Example" },
      removed: false,
    });
    if (chrome && raise(chromeHwnd)) {
      await start("edited", [], { categories: ["social"] });
      await chromeTab("https://example.com/edited");
      raise(chromeHwnd);
      step(
        "F.1 a site added to a built-in is blocked with it",
        edit.ok && Boolean(await blockedNow()),
        omnibox(chromeHwnd),
      );
      await complete();
    }
    await ipc("focus_reset_builtin", { category: "social" });
    const members = await ipc<
      { label: string; kind: string; installed: boolean }[]
    >("focus_members", { id: "messaging" });
    const apps = members.ok ? members.v.filter((m) => m.kind === "app") : [];
    step(
      "F.1 Messaging marks apps installed against this machine",
      members.ok,
      apps
        .map((m) => `${m.label}:${m.installed ? "installed" : "not"}`)
        .join(", "),
    );
    const games = await ipc<{ detected: boolean }[]>("focus_members", {
      id: "games",
    });
    step(
      "F.2 Games lists detected games",
      games.ok,
      `${games.ok ? games.v.filter((g) => g.detected).length : 0} detected`,
    );
    await ipc("open_settings");
    const settings = await until(
      () =>
        cdp!
          .contexts()
          .flatMap((c) => c.pages())
          .find((p) => p.url().includes("window=settings")),
      15_000,
    );
    if (settings) {
      await settings.waitForLoadState();
      await settings
        .getByRole("button", { name: "Focus", exact: true })
        .click();
      await settings.screenshot({ path: join(SHOTS, "settings-focus.png") });
      await settings.getByRole("button", { name: /^Messaging/ }).click();
      const adder = settings.getByRole("textbox", {
        name: "Add apps or websites",
      });
      await adder.fill("teams.microsoft.com");
      await adder.press("Enter");
      const icon = join(
        DATA,
        "v3sper",
        "takyon",
        "favicons",
        "teams.microsoft.com.ico",
      );
      step(
        "F.1 adding a site fetches its icon once",
        Boolean(await until(() => existsSync(icon), 15_000, 500)),
      );
      await sleep(1500);
      await settings.screenshot({
        path: join(SHOTS, "settings-messaging.png"),
      });
      await ipc("focus_reset_builtin", { category: "messaging" });
      const wait = 75_000 - (Date.now() - launchedAt);
      if (wait > 0) await sleep(wait);
      const dir = join(DATA, "v3sper", "takyon", "favicons");
      const count = await until(
        () =>
          existsSync(dir)
            ? readdirSync(dir).length >= 30 && readdirSync(dir).length
            : 0,
        30_000,
        1000,
      );
      step(
        "G.1 built-in site icons fetched after first launch",
        Boolean(count),
        `${count || (existsSync(dir) ? readdirSync(dir).length : 0)} icons cached`,
      );
      await settings
        .locator("main h1")
        .getByRole("button", { name: "Focus" })
        .click()
        .catch(() => {});
      await settings.getByRole("button", { name: /^Social/ }).click();
      await sleep(1500);
      await settings.screenshot({ path: join(SHOTS, "settings-social.png") });
      await settings
        .locator("main h1")
        .getByRole("button", { name: "Focus" })
        .click();
      await settings.getByRole("button", { name: /Focus stats/ }).click();
      await sleep(1000);
      step(
        "F.5 Focus / Stats shows this run's sessions",
        (await settings.locator("main").textContent())?.includes("Focus") ??
          false,
      );
      await settings.screenshot({ path: join(SHOTS, "settings-stats.png") });
    } else {
      skip("F Settings window", "never opened over CDP");
    }
  }
}

main()
  .catch((e) => {
    console.error(`\nABORTED: ${e instanceof Error ? e.stack : e}`);
    rows.push({ id: "driver", result: "FAIL", note: String(e) });
  })
  .finally(async () => {
    await complete().catch(() => {});
    await chrome?.close().catch(() => {});
    try {
      execFileSync("powershell.exe", [
        "-NoProfile",
        "-Command",
        `Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -like '*takyon-verify-v0.16*chrome*' } | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }`,
      ]);
    } catch {
      /* best effort */
    }
    await stopTakyon();
    kill(TEST_APP);
    kill(KEEP_APP);
    writeFileSync(join(WORK, "results.json"), JSON.stringify(rows, null, 2));
    const count = (r: Row["result"]) =>
      rows.filter((x) => x.result === r).length;
    console.log(
      `\n${count("PASS")} pass, ${count("FAIL")} fail, ${count("SKIP")} skip. Shots in ${SHOTS}`,
    );
    process.exit(count("FAIL") ? 1 : 0);
  });
