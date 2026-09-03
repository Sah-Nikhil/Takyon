/**
 * General: the switches that are about Takyon rather than about one feature.
 *
 * Appearance sits here as a group rather than as its own page, which is both the
 * plan's fixed tier-one set and what Raycast does. The light/dark override and
 * the pinned interface sizes join this group in v0.6's appearance slice.
 */

import { useCallback, useEffect, useState } from "react";
import type { Theme, UiSize } from "@takyon/shared";
import * as api from "@/api";
import {
  preferences,
  reduceMotion,
  refresh,
  setReduceMotion,
  setTheme,
  setUiSize,
  systemReducesMotion,
} from "@/prefs";
import { Chips, Group, Row, Switch, useApplied } from "../controls";

/**
 * Dev builds must never register autostart: a debug registration writes a `Run`
 * key pointing at `target\debug\` that survives uninstalling the real app.
 *
 * `inTauri` tightens this rather than loosening it — outside Tauri the write
 * goes to the mock and can reach no registry at all.
 */
const DEV = import.meta.env.DEV && api.inTauri;

const THEMES: ReadonlyArray<{ value: Theme; label: string }> = [
  { value: "system", label: "System" },
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

const SIZES: ReadonlyArray<{ value: UiSize; label: string }> = [
  { value: "small", label: "Small" },
  { value: "default", label: "Default" },
  { value: "large", label: "Large" },
];

export function General() {
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [still, setStill] = useState(reduceMotion);
  const [theme, setThemeState] = useState(() => preferences().theme);
  const [size, setSizeState] = useState(() => preferences().uiSize);
  // Read once. Windows can change it mid-session, but this line is copy, not
  // behaviour — the media query in styles.css enforces the OS setting either way.
  const [osStill] = useState(systemReducesMotion);

  // Read the OS on mount, every mount. Autostart state lives in the OS and is
  // never mirrored into `settings.db`: Task Manager flips it behind the app's
  // back with no event to observe, so a cached copy would be confidently wrong
  // (ADR-0015).
  useEffect(() => {
    void api.autostartIsEnabled().then(setAutostart);
  }, []);

  const startup = useApplied(api.autostartSetEnabled, api.autostartIsEnabled);
  // Re-reads Rust rather than the in-process cache, so a refused write settles
  // the switch on what is stored — the same rule autostart follows against the OS.
  const motion = useApplied(setReduceMotion, async () => (await refresh()).reduceMotion);
  const themeApplied = useApplied(setTheme, async () => (await refresh()).theme);
  const sizeApplied = useApplied(setUiSize, async () => (await refresh()).uiSize);

  const toggleAutostart = useCallback(
    (on: boolean) => {
      if (DEV) return;
      void startup.apply(on, (next) => setAutostart(next));
    },
    [startup],
  );

  return (
    <>
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

      <Group title="Appearance">
        <Row
          id="theme"
          label="Appearance"
          applied={themeApplied.applied}
          error={themeApplied.error}
          description="Follows Windows unless you override it, and the override wins in both directions."
        >
          <Chips
            label="Appearance"
            value={theme}
            options={THEMES}
            onChange={(next) => void themeApplied.apply(next, setThemeState)}
          />
        </Row>
        <Row
          id="ui-size"
          label="Interface size"
          applied={sizeApplied.applied}
          error={sizeApplied.error}
          description="Scales the whole interface, Palette included. The window resizes with it rather than after it."
        >
          <Chips
            label="Interface size"
            value={size}
            options={SIZES}
            onChange={(next) => void sizeApplied.apply(next, setSizeState)}
          />
        </Row>
        <Row
          id="motion"
          label="Turn off animations"
          applied={motion.applied}
          error={motion.error}
          description={
            osStill
              ? "Windows is already set to reduce motion, so the mark is holding still regardless of this switch."
              : "The mark breathes while the Palette is open and waiting for a query. Nothing else in Takyon moves."
          }
        >
          <Switch
            label="Turn off animations"
            checked={still}
            onChange={(on) => void motion.apply(on, setStill)}
          />
        </Row>
      </Group>
    </>
  );
}
