/**
 * Advanced: the crash-log folder, and removing everything Takyon stored.
 *
 * ADR-0010 — logs are written locally and **nothing is ever sent**. The button
 * opens a folder. There is no upload path in Takyon for it to use.
 *
 * The removal below is macOS-only on purpose: dragging an app to the Trash runs
 * nothing, so the data directory and the Keychain item survive an "uninstall".
 * Windows has an NSIS uninstaller that already deletes both.
 */

import { useCallback, useEffect, useState } from "react";
import type { RemovalReport } from "@takyon/shared";
import * as api from "@/api";
import { Confirm, Group, Row } from "../controls";

export function Advanced() {
  const [error, setError] = useState<string | null>(null);
  const [platform, setPlatform] = useState<string>("other");
  const [confirming, setConfirming] = useState(false);
  const [removed, setRemoved] = useState<RemovalReport | null>(null);

  useEffect(() => {
    void api.settingsSnapshot().then((s) => setPlatform(s.platform));
  }, []);

  const open = useCallback(async () => {
    setError(null);
    try {
      await api.openCrashLogs();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  const remove = useCallback(async () => {
    setConfirming(false);
    setError(null);
    try {
      setRemoved(await api.removeAllData());
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  return (
    <>
      <Group title="Diagnostics">
        <Row
          id="crash-logs"
          label="Crash logs"
          error={error}
          description="A panic in a release build is otherwise silent — no console, nothing on screen. Written here, and never sent anywhere."
        >
          <button
            type="button"
            onClick={() => void open()}
            className="rounded-control bg-control px-2.5 py-1 text-[12.5px] text-fg/86 transition-colors hover:text-fg"
          >
            Open folder
          </button>
        </Row>
      </Group>

      {platform === "macos" && (
        <Group title="Data">
          <Row
            id="remove-all-data"
            label="Remove all Takyon data"
            description={describe(removed)}
          >
            <button
              type="button"
              onClick={() => setConfirming(true)}
              className="rounded-control bg-warning/90 px-2.5 py-1 text-[12.5px] font-medium text-plate transition-colors hover:bg-warning"
            >
              Remove
            </button>
          </Row>
        </Group>
      )}

      {confirming && (
        <Confirm
          title="This deletes everything Takyon has stored"
          consequence="Your clipboard history, what Takyon has learned about the apps you open, your settings and the key that encrypts the history. Permanent, and it cannot be undone. Your own files are not touched."
          confirmLabel="Delete it all"
          onConfirm={() => void remove()}
          onCancel={() => setConfirming(false)}
        />
      )}
    </>
  );
}

/**
 * The sentence under the button, before and after.
 *
 * A partial removal is named rather than rounded to "done": a Keychain entry
 * that survived is the one the user most needs to hear about, and it is the
 * failure a bare success message would hide.
 */
function describe(report: RemovalReport | null): string {
  if (!report) {
    return "Dragging Takyon to the Trash runs nothing, so its data outlives it. This deletes the lot while the app can still reach it.";
  }
  if (report.problems.length > 0) {
    return `Some of it could not be removed: ${report.problems.join(" ")}`;
  }
  const where = report.dataDir ? ` ${report.dataDir} is gone.` : "";
  return `Removed.${where} Quit Takyon and drag it to the Trash to finish.`;
}
