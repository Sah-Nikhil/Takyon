/**
 * `bun run bench` — the four performance budgets from IMPLEMENTATION_PLAN.md §10.
 *
 * A regression here is a failing test, not a nice-to-have: these numbers *are* the
 * product. v0.1 exists to produce them, because ADR-0003 (keep the Palette warm,
 * trim it on hide) was reasoned rather than measured, and TBC-0002 records it as
 * the least-evidenced load-bearing decision in the project.
 *
 * ## What this measures, and what it does not
 *
 * Every span is timed inside Rust, on one clock — see `src-tauri/src/bench.rs`.
 * The frontend's only contribution is echoing back an id once it has painted. So
 * `show_to_first_pixel` **includes** one IPC hop and **excludes** DWM's final
 * composition. That gap is real and is closed by a one-off high-FPS capture,
 * recorded alongside these numbers in `docs/tbc/0002`.
 *
 * ## The measurement that actually decides ADR-0003
 *
 * `--idle <minutes>`. A benchmark run in a tight loop never sees the case a real
 * user hits: Windows has had thirty minutes to reclaim the trimmed working set,
 * and the first show after that is a different event from the second. Running only
 * the warm loop would produce four flattering numbers and answer nothing.
 *
 * Usage:
 *   bun run bench                  # 30 warm shows + idle memory
 *   bun run bench --runs 100
 *   bun run bench --idle 35        # one show after 35 minutes idle
 *   bun run bench --dev            # measure the debug build (slower; not a budget)
 */

import { mkdirSync, readFileSync, existsSync } from "node:fs";
import { join, resolve } from "node:path";

const ROOT = resolve(import.meta.dir, "..");
const RESULTS = join(ROOT, "bench", "results");

/** IMPLEMENTATION_PLAN.md §10. The `!e` p95 budget arrives with file search at v0.7. */
const BUDGETS = {
  show_to_first_pixel: { ms: 50, label: "Hotkey to first pixel" },
  start_to_hotkey_ready: { ms: 500, label: "Process start to hotkey responsive" },
  idle_rss_mb: { mb: 150, label: "Idle RSS (warm, trimmed)" },
} as const;

type Args = { runs: number; idle: number; dev: boolean };

function parseArgs(argv: string[]): Args {
  const get = (name: string, fallback: number) => {
    const i = argv.indexOf(`--${name}`);
    if (i === -1) return fallback;
    const n = Number(argv[i + 1]);
    if (!Number.isFinite(n)) throw new Error(`--${name} needs a number`);
    return n;
  };
  return {
    runs: get("runs", 30),
    idle: get("idle", 0),
    dev: argv.includes("--dev"),
  };
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function powershell(script: string, args: string[] = []): Promise<string> {
  const proc = Bun.spawn(
    ["powershell", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", script, ...args],
    { stdout: "pipe", stderr: "pipe" },
  );
  const [out, err, code] = await Promise.all([
    new Response(proc.stdout).text(),
    new Response(proc.stderr).text(),
    proc.exited,
  ]);
  if (code !== 0) throw new Error(`${script} failed (${code}): ${err.trim()}`);
  return out.trim();
}

type Record_ = { event: string; ms: number; ts: number };

function readLog(path: string): Record_[] {
  if (!existsSync(path)) return [];
  return readFileSync(path, "utf8")
    .split("\n")
    .filter((l) => l.trim())
    .map((l) => JSON.parse(l) as Record_);
}

/** Wait until `predicate` holds over the log, or give up. */
async function waitFor(
  path: string,
  predicate: (rows: Record_[]) => boolean,
  timeoutMs: number,
  what: string,
): Promise<Record_[]> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const rows = readLog(path);
    if (predicate(rows)) return rows;
    if (Date.now() > deadline) throw new Error(`timed out waiting for ${what}`);
    await sleep(20);
  }
}

function stats(values: number[]) {
  const s = [...values].sort((a, b) => a - b);
  const at = (q: number) => s[Math.min(s.length - 1, Math.floor(q * s.length))]!;
  return {
    n: s.length,
    min: s[0]!,
    p50: at(0.5),
    p95: at(0.95),
    max: s[s.length - 1]!,
    mean: s.reduce((a, b) => a + b, 0) / s.length,
  };
}

const fmt = (n: number) => n.toFixed(1).padStart(7);

function verdict(actual: number, budget: number) {
  return actual <= budget ? "PASS" : "OVER BUDGET";
}

async function main() {
  const args = parseArgs(Bun.argv.slice(2));

  const profile = args.dev ? "debug" : "release";
  const exe = join(ROOT, "apps", "desktop", "src-tauri", "target", profile, "takyon.exe");
  if (!existsSync(exe)) {
    console.error(
      `No ${profile} binary at ${exe}\n` +
        (args.dev
          ? "Run `bun run dev` once to produce one."
          : "Run `bun run build` first. Benching a debug build measures the compiler, not the product — pass --dev only if that is what you meant."),
    );
    process.exit(1);
  }

  mkdirSync(RESULTS, { recursive: true });
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const logPath = join(RESULTS, `${stamp}.jsonl`);

  console.log(`Takyon bench — ${profile} build`);
  console.log(`  binary: ${exe}`);
  console.log(`  log:    ${logPath}\n`);

  const child = Bun.spawn([exe], {
    env: { ...process.env, TAKYON_BENCH_LOG: logPath },
    stdout: "inherit",
    stderr: "inherit",
  });

  let failed = false;
  try {
    await waitFor(
      logPath,
      (rows) => rows.some((r) => r.event === "start_to_hotkey_ready"),
      15_000,
      "the hotkey to be registered",
    );

    // The window exists but has never been shown, so its first show includes
    // WebView2's first paint. That is a real cost but it is not the cost being
    // budgeted, so it is spent here and discarded.
    await sleep(1500);
    await show(ROOT, logPath, 0);
    await hide(ROOT);
    await sleep(400);

    const warmup = readLog(logPath).filter((r) => r.event === "show_to_first_pixel").length;

    if (args.idle > 0) {
      console.log(
        `Idling ${args.idle} minutes before a single show. This is the measurement\n` +
          `that decides ADR-0003 — a tight loop never sees Windows reclaim the\n` +
          `trimmed working set, which is exactly what a real user hits.\n`,
      );
      await sleep(args.idle * 60_000);
    } else {
      for (let i = 0; i < args.runs; i++) {
        await show(ROOT, logPath, warmup + i);
        await hide(ROOT);
        // Long enough for the trim thread to finish, so the next show pays the
        // page faults the model says it should.
        await sleep(250);
        process.stdout.write(`\r  shows: ${i + 1}/${args.runs}`);
      }
      process.stdout.write("\n\n");
    }

    if (args.idle > 0) {
      await show(ROOT, logPath, warmup);
      await hide(ROOT);
    }

    // Settle before measuring memory: the trim happens on a background thread
    // after hide, and reading immediately would measure the untrimmed state.
    await sleep(3000);
    const mem = JSON.parse(
      await powershell(join(ROOT, "scripts", "bench-mem.ps1"), ["-RootPid", String(child.pid)]),
    ) as { processes: number; workingSet: number; privateBytes: number };

    const rows = readLog(logPath);
    const shows = rows
      .filter((r) => r.event === "show_to_first_pixel")
      .slice(warmup)
      .map((r) => r.ms);
    const startup = rows.find((r) => r.event === "start_to_hotkey_ready")?.ms ?? NaN;
    const rssMb = mem.workingSet / 1024 / 1024;
    const privMb = mem.privateBytes / 1024 / 1024;

    if (shows.length === 0) {
      throw new Error(
        "no shows were recorded. The hotkey almost certainly failed to register — " +
          "PowerToys Run holds Alt+Space by default.",
      );
    }

    const s = stats(shows);
    console.log("Results");
    console.log("-------");
    console.log(`${BUDGETS.show_to_first_pixel.label} (budget ${BUDGETS.show_to_first_pixel.ms} ms)`);
    console.log(`  n=${s.n}  min ${fmt(s.min)}  p50 ${fmt(s.p50)}  p95 ${fmt(s.p95)}  max ${fmt(s.max)}  ms`);
    console.log(`  ${verdict(s.p95, BUDGETS.show_to_first_pixel.ms)} on p95\n`);

    console.log(`${BUDGETS.start_to_hotkey_ready.label} (budget ${BUDGETS.start_to_hotkey_ready.ms} ms)`);
    console.log(`  ${fmt(startup)} ms   ${verdict(startup, BUDGETS.start_to_hotkey_ready.ms)}`);
    console.log(`  Process start only. Session start to process start belongs to`);
    console.log(`  Windows and needs a reboot — see the manual script.\n`);

    console.log(`${BUDGETS.idle_rss_mb.label} (budget ${BUDGETS.idle_rss_mb.mb} MB)`);
    console.log(`  ${fmt(rssMb)} MB working set across ${mem.processes} processes`);
    console.log(`  ${fmt(privMb)} MB committed`);
    console.log(`  ${verdict(rssMb, BUDGETS.idle_rss_mb.mb)}`);
    console.log(`  The gap between the two is how much the trim on hide actually`);
    console.log(`  released. If they are close, trimming is not buying what`);
    console.log(`  ADR-0003 assumed and TBC-0002's first trigger has fired.\n`);

    console.log("Hotkey to first Entry (budget 30 ms)");
    console.log("  Not measurable in v0.1: there is no query pipeline yet. This");
    console.log("  budget starts being measured at v0.2, with the first Source.\n");

    if (args.idle > 0) {
      console.log(
        `This was the post-idle measurement (${args.idle} min). Compare it against a\n` +
          `warm run: if the first show after idle is dramatically slower, TBC-0002's\n` +
          `third trigger has fired and the warm-window model needs revisiting.\n`,
      );
    }

    console.log(`Write these into docs/tbc/0002 with the machine and the date.`);

    failed = s.p95 > BUDGETS.show_to_first_pixel.ms || rssMb > BUDGETS.idle_rss_mb.mb;
  } finally {
    // The Palette is a hidden window with no taskbar button, so a bench run that
    // exits without this leaves an invisible process holding Alt+Space — and the
    // next run then measures nothing and blames the hotkey.
    child.kill();
    await child.exited;
  }

  if (failed) {
    console.error("\nAt least one budget was missed. Treat this as a failing test.");
    process.exit(1);
  }
}

async function show(root: string, logPath: string, alreadySeen: number) {
  await powershell(join(root, "scripts", "bench-input.ps1"), ["-Key", "AltSpace"]);
  await waitFor(
    logPath,
    (rows) => rows.filter((r) => r.event === "show_to_first_pixel").length > alreadySeen,
    5_000,
    "the Palette to report a painted frame",
  );
}

async function hide(root: string) {
  await powershell(join(root, "scripts", "bench-input.ps1"), ["-Key", "Escape"]);
}

await main();
