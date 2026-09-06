/**
 * File Search: the scopes, the exclusions, the switches and the history.
 *
 * Rebuilt at v0.10 to the shape of the reference — a scope *list* with an add
 * control rather than a form pretending to be a row, exclusions as rows rather
 * than as a bag of chips, and a reset at the bottom.
 *
 * The roots are still the least-evidenced call in the design (TBC-0005), which is
 * why the live entry count sits beside them rather than in a diagnostics panel:
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
  const [pendingClear, setPendingClear] = useState<number | null>(null);
  const [pendingReset, setPendingReset] = useState(false);

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

  const reset = useCallback(async () => {
    // The response, not a guess: `index::roots::defaults()` probes the machine
    // on every read (TBC-0005), so what a reset produces is not knowable here.
    const restored = await api.resetFilesRoots();
    const snapshot = await refresh();
    setRoots(restored);
    setExcludes(snapshot.filesExcludes);
    setBangless(snapshot.filesBangless);
    setFallback(snapshot.filesFallback);
    setPendingReset(false);
  }, []);

  return (
    <>
      <Group title="Search scopes">
        <Row
          id="index-roots"
          label="Folders to index"
          description={`Walked once at startup and watched for changes after that. ${describe(report)}.`}
        >
          <AddField
            placeholder="C:\Data"
            label="Folder to index"
            width="w-52"
            onAdd={(value) => void save([...roots, value], excludes)}
          />
        </Row>

        {roots.length === 0 ? (
          <Empty>
            Nothing is indexed, so <code>!e</code> will find nothing. Add a folder
            above, or reset below to go back to the folders Takyon picks itself.
          </Empty>
        ) : (
          roots.map((root) => (
            <ListRow
              key={root}
              label={root}
              mono
              onRemove={() => void save(roots.filter((r) => r !== root), excludes)}
              removeLabel={`Stop indexing ${root}`}
            />
          ))
        )}
      </Group>

      <Group title="Ignore patterns">
        <Row
          id="index-excludes"
          label="Folder names to skip"
          description="Matched against one whole folder name, anywhere under a scope — not a path and not a glob. Skipped during the walk, so nothing inside is ever read."
        >
          <AddField
            placeholder="node_modules"
            label="Folder name to skip"
            width="w-44"
            onAdd={(value) => void save(roots, [...excludes, value])}
          />
        </Row>

        {excludes.length === 0 ? (
          <Empty>
            Nothing is skipped. A scope containing <code>node_modules</code> or a
            build directory will index every file inside it.
          </Empty>
        ) : (
          excludes.map((name) => (
            <ListRow
              key={name}
              label={name}
              mono
              onRemove={() => void save(roots, excludes.filter((e) => e !== name))}
              removeLabel={`Stop skipping ${name}`}
            />
          ))
        )}
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
          <Action onClick={() => void api.openedCount().then(setPendingClear)}>
            Clear history
          </Action>
        </Row>
        <Row
          id="files-reset"
          label="Reset to defaults"
          description="Puts the scopes and ignore patterns back to the ones Takyon picks for this machine, and turns both switches above off."
        >
          <Action onClick={() => setPendingReset(true)}>Reset</Action>
        </Row>
      </Group>

      {pendingClear !== null && (
        <Confirm
          title="This clears what Takyon remembers opening"
          consequence={`This permanently deletes ${pendingClear.toLocaleString()} ${
            pendingClear === 1 ? "entry" : "entries"
          }. Your files are not touched, and nothing here was ever sent anywhere.`}
          confirmLabel="Clear it"
          onConfirm={() => void api.clearOpened().then(() => setPendingClear(null))}
          onCancel={() => setPendingClear(null)}
        />
      )}

      {/*
        Confirmed, because it is a change you cannot read off the screen
        afterwards: the roots it restores are probed rather than listed, so
        "undo by retyping what was there" is not available.
      */}
      {pendingReset && (
        <Confirm
          title="Reset File Search to its defaults"
          consequence={`This discards ${roots.length} ${
            roots.length === 1 ? "scope" : "scopes"
          } and ${excludes.length} ${
            excludes.length === 1 ? "ignore pattern" : "ignore patterns"
          }, and rebuilds the index. Your files are not touched, and the history above is kept.`}
          confirmLabel="Reset"
          onConfirm={() => void reset()}
          onCancel={() => setPendingReset(false)}
        />
      )}
    </>
  );
}

/** One entry in a list under a group's heading row. */
function ListRow({
  label,
  mono,
  removeLabel,
  onRemove,
}: {
  label: string;
  mono?: boolean;
  removeLabel: string;
  onRemove: () => void;
}) {
  return (
    <div className="group flex items-center justify-between gap-4 px-3.5 py-2.5">
      <span className={`truncate text-[13px] text-fg/86 ${mono ? "font-mono" : ""}`}>
        {label}
      </span>
      {/*
        Always in the document and only revealed on hover or focus — never
        conditionally rendered. A control that appears on hover cannot be
        tabbed to, and this list is otherwise entirely keyboard-reachable.
      */}
      <button
        type="button"
        onClick={onRemove}
        aria-label={removeLabel}
        className="shrink-0 rounded-control px-2 py-1 text-[12.5px] text-fg/0 transition-colors hover:bg-row-hover hover:text-fg group-hover:text-fg/68 focus-visible:text-fg focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent/60"
      >
        Remove
      </button>
    </div>
  );
}

/**
 * What an empty list says.
 *
 * Naming the consequence rather than the absence: "no folders" is a fact anyone
 * can already see, and "`!e` will find nothing" is the part that decides whether
 * they should care.
 */
function Empty({ children }: { children: React.ReactNode }) {
  return (
    <p className="px-3.5 py-3 text-[12.5px] leading-snug text-fg/60">{children}</p>
  );
}

/** The add-one-item control both lists use. */
function AddField({
  placeholder,
  label,
  width,
  onAdd,
}: {
  placeholder: string;
  label: string;
  width: string;
  onAdd: (value: string) => void;
}) {
  const [draft, setDraft] = useState("");
  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        const value = draft.trim();
        if (!value) return;
        onAdd(value);
        setDraft("");
      }}
      className="flex items-center gap-2"
    >
      <input
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        placeholder={placeholder}
        aria-label={label}
        className={`${width} rounded-control border border-hairline bg-control px-2.5 py-1 text-[12.5px] text-fg outline-none transition-colors placeholder:text-fg/46 focus:border-accent/60`}
      />
      <button
        type="submit"
        disabled={!draft.trim()}
        className="rounded-control px-2 py-1 text-[12.5px] text-fg/72 transition-colors hover:bg-row-hover hover:text-fg disabled:opacity-30 disabled:hover:bg-transparent"
      >
        Add
      </button>
    </form>
  );
}

/** A plain text button, for the two that open a confirmation. */
function Action({ onClick, children }: { onClick: () => void; children: React.ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="rounded-control border border-hairline px-2.5 py-1 text-[12.5px] text-fg/80 transition-colors hover:bg-row-hover hover:text-fg"
    >
      {children}
    </button>
  );
}
