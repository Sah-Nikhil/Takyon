/**
 * File Search: the roots, the exclusions, the two toggles and the history.
 *
 * The roots are the least-evidenced call in the design (TBC-0005), which is why
 * the live entry count sits beside them rather than in a diagnostics panel —
 * under ~20k means the scope is too narrow to justify the feature and over ~400k
 * means an exclusion is missing, and neither is visible without the number.
 */

import { useCallback, useEffect, useState } from "react";
import type { FileIndexReport } from "@takyon/shared";
import * as api from "@/api";
import { preferences, refresh, setFilesBangless, setFilesFallback } from "@/prefs";
import { Confirm, Group, Row, Switch, useApplied } from "../controls";

/** How often the index status is re-read while the page is open. */
const POLL_MS = 2000;

/** What the status row says, in the user's terms rather than the enum's. */
function describe(report: FileIndexReport | null): string {
  if (!report) return "Checking…";
  switch (report.state) {
    case "building":
      return "Building the index…";
    case "stale":
      // Never silently: an index that quietly misses files teaches the user not
      // to trust it, which is worse than having no index (ADR-0007).
      return "Some changes were missed. Rescanning — results may be incomplete.";
    default:
      return `${report.entries.toLocaleString()} files and folders indexed`;
  }
}

export function FileSearch() {
  const [bangless, setBangless] = useState(() => preferences().filesBangless);
  const [fallback, setFallback] = useState(() => preferences().filesFallback);
  const [roots, setRoots] = useState<string[]>(() => preferences().filesRoots);
  const [excludes, setExcludes] = useState<string[]>(() => preferences().filesExcludes);
  const [report, setReport] = useState<FileIndexReport | null>(null);
  const [rootDraft, setRootDraft] = useState("");
  const [excludeDraft, setExcludeDraft] = useState("");
  const [pendingClear, setPendingClear] = useState<number | null>(null);

  const banglessApplied = useApplied(setFilesBangless, async () => (await refresh()).filesBangless);
  const fallbackApplied = useApplied(setFilesFallback, async () => (await refresh()).filesFallback);

  // Polled rather than pushed: the count moves when a walk lands, which is on
  // the index's schedule and not on any user action this window can hook.
  useEffect(() => {
    let live = true;
    const read = () => {
      void api.fileIndexStatus().then((r) => {
        if (live) setReport(r);
      });
    };
    read();
    const timer = setInterval(read, POLL_MS);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, []);

  const save = useCallback(async (nextRoots: string[], nextExcludes: string[]) => {
    setRoots(nextRoots);
    setExcludes(nextExcludes);
    await api.setFilesRoots(nextRoots, nextExcludes);
  }, []);

  const clear = useCallback(async () => {
    await api.clearOpened();
    setPendingClear(null);
  }, []);

  return (
    <>
      <Group title="Where to look">
        <Row
          id="index-roots"
          label="Indexed folders"
          description={describe(report)}
        >
          <form
            onSubmit={(e) => {
              e.preventDefault();
              if (rootDraft.trim()) {
                void save([...roots, rootDraft.trim()], excludes);
                setRootDraft("");
              }
            }}
            className="flex items-center gap-2"
          >
            <input
              value={rootDraft}
              onChange={(e) => setRootDraft(e.target.value)}
              placeholder="C:\Data"
              aria-label="Folder to index"
              className="w-52 rounded-control bg-control px-2.5 py-1 text-[12.5px] text-fg outline-none placeholder:text-fg/30"
            />
            <button
              type="submit"
              disabled={!rootDraft.trim()}
              className="rounded-control px-2 py-1 text-[12.5px] text-fg/60 transition-colors hover:bg-row-hover hover:text-fg disabled:opacity-30"
            >
              Add
            </button>
          </form>
        </Row>

        {roots.map((root) => (
          <div key={root} className="flex items-center justify-between gap-4 px-3.5 py-2.5">
            <span className="truncate font-mono text-[13px] text-fg/80">{root}</span>
            <button
              type="button"
              onClick={() => void save(roots.filter((r) => r !== root), excludes)}
              className="shrink-0 rounded-control px-2 py-1 text-[12.5px] text-fg/50 transition-colors hover:bg-row-hover hover:text-fg"
            >
              Remove
            </button>
          </div>
        ))}
      </Group>

      <Group title="What to skip">
        <Row
          id="index-excludes"
          label="Skipped folder names"
          description="Matched against one whole folder name, anywhere under a root. Skipped during the walk, so nothing inside is ever read."
        >
          <form
            onSubmit={(e) => {
              e.preventDefault();
              if (excludeDraft.trim()) {
                void save(roots, [...excludes, excludeDraft.trim()]);
                setExcludeDraft("");
              }
            }}
            className="flex items-center gap-2"
          >
            <input
              value={excludeDraft}
              onChange={(e) => setExcludeDraft(e.target.value)}
              placeholder="node_modules"
              aria-label="Folder name to skip"
              className="w-40 rounded-control bg-control px-2.5 py-1 text-[12.5px] text-fg outline-none placeholder:text-fg/30"
            />
            <button
              type="submit"
              disabled={!excludeDraft.trim()}
              className="rounded-control px-2 py-1 text-[12.5px] text-fg/60 transition-colors hover:bg-row-hover hover:text-fg disabled:opacity-30"
            >
              Add
            </button>
          </form>
        </Row>

        <div className="flex flex-wrap gap-1.5 px-3.5 pb-3">
          {excludes.map((name) => (
            <button
              key={name}
              type="button"
              onClick={() => void save(roots, excludes.filter((e) => e !== name))}
              title={`Stop skipping ${name}`}
              className="rounded-control bg-control px-2 py-0.5 font-mono text-[12px] text-fg/70 transition-colors hover:bg-row-hover hover:text-fg"
            >
              {name} ×
            </button>
          ))}
        </div>
      </Group>

      <Group title="How to reach it">
        <Row
          id="files-bangless"
          label="Show files without typing !e"
          applied={banglessApplied.applied}
          error={banglessApplied.error}
          description="File Entries join ordinary results, always below applications. !e works either way."
        >
          <Switch
            label="Show files without typing !e"
            checked={bangless}
            onChange={(on) => void banglessApplied.apply(on, setBangless)}
          />
        </Row>
        <Row
          id="files-fallback"
          label="Also ask Windows Search"
          applied={fallbackApplied.applied}
          error={fallbackApplied.error}
          description="Covers folders you have not indexed, using Windows' own index. Slower, and it finds only what Windows happens to have indexed — which on this machine excludes many code folders."
        >
          <Switch
            label="Also ask Windows Search"
            checked={fallback}
            onChange={(on) => void fallbackApplied.apply(on, setFallback)}
          />
        </Row>
      </Group>

      <Group title="History">
        <Row
          id="clear-opened"
          label="Files you opened through Takyon"
          description="Recorded here only, never read from Windows. Shown when you type !e with nothing after it."
        >
          <button
            type="button"
            onClick={() => void api.openedCount().then(setPendingClear)}
            className="rounded-control px-2 py-1 text-[12.5px] text-fg/60 transition-colors hover:bg-row-hover hover:text-fg"
          >
            Clear history
          </button>
        </Row>
      </Group>

      {pendingClear !== null && (
        <Confirm
          title="This clears what Takyon remembers opening"
          consequence={`This permanently deletes ${pendingClear.toLocaleString()} ${
            pendingClear === 1 ? "entry" : "entries"
          }. Your files are not touched, and nothing here was ever sent anywhere.`}
          confirmLabel="Clear it"
          onConfirm={() => void clear()}
          onCancel={() => setPendingClear(null)}
        />
      )}
    </>
  );
}
