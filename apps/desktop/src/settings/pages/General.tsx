/**
 * General: the switches that are about Takyon rather than about one feature.
 *
 * One control, and that is not an oversight. Appearance moved to its own page at
 * v0.10 — it grew a five-theme picker and a window-mode control, which is a page
 * rather than a group — and everything else here belongs to a feature and lives
 * on that feature's page. A General page that collected things because they had
 * nowhere else to go is exactly what the two-tier navigation exists to prevent.
 */

import { useCallback, useEffect, useState } from "react";
import * as api from "@/api";
import { Group, Row, Switch, useApplied } from "../controls";

/**
 * Dev builds must never register autostart: a debug registration writes a `Run`
 * key pointing at `target\debug\` that survives uninstalling the real app.
 *
 * `inTauri` tightens this rather than loosening it — outside Tauri the write
 * goes to the mock and can reach no registry at all.
 */
const DEV = import.meta.env.DEV && api.inTauri;

export function General() {
  const [autostart, setAutostart] = useState<boolean | null>(null);

  // Read the OS on mount, every mount. Autostart state lives in the OS and is
  // never mirrored into `settings.db`: Task Manager flips it behind the app's
  // back with no event to observe, so a cached copy would be confidently wrong
  // (ADR-0015).
  useEffect(() => {
    void api.autostartIsEnabled().then(setAutostart);
  }, []);

  const startup = useApplied(api.autostartSetEnabled, api.autostartIsEnabled);

  const toggleAutostart = useCallback(
    (on: boolean) => {
      if (DEV) return;
      void startup.apply(on, (next) => setAutostart(next));
    },
    [startup],
  );

  return (
    <Group>
      <Row
        id="autostart"
        label="Start Takyon when I log in"
        applied={startup.applied}
        error={startup.error}
        description={
          DEV
            ? "Unavailable in a development build. Registering here would write a startup entry pointing at target\\debug\\, which would survive uninstalling the real app."
            : "The launcher needs to already be running for the hotkey to answer."
        }
      >
        <Switch
          label="Start Takyon when I log in"
          checked={autostart ?? false}
          disabled={DEV || autostart === null}
          onChange={toggleAutostart}
        />
      </Row>
    </Group>
  );
}
