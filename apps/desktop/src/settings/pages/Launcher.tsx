/**
 * Launcher: how the Palette behaves, rather than what it can find.
 *
 * The tray switch is the one control here that can be refused. Hiding the tray
 * while the hotkey is dead would leave no way in and no way out, so Rust says no
 * and the error lands beside the control like any other.
 */

import { useState } from "react";
import type { Placement } from "@takyon/shared";
import { preferences, refresh, setPlacement, setRecents, setTray } from "@/prefs";
import { Chips, Group, Row, Switch, useApplied } from "../controls";

const PLACEMENTS: ReadonlyArray<{ value: Placement; label: string }> = [
  { value: "cursor", label: "Where the cursor is" },
  { value: "primary", label: "Primary screen" },
];

export function Launcher() {
  const [recents, setRecentsState] = useState(() => preferences().recents);
  const [tray, setTrayState] = useState(() => preferences().tray);
  const [placement, setPlacementState] = useState(() => preferences().placement);

  const recentsApplied = useApplied(setRecents, async () => (await refresh()).recents);
  const trayApplied = useApplied(setTray, async () => (await refresh()).tray);
  const placementApplied = useApplied(setPlacement, async () => (await refresh()).placement);

  return (
    <>
      <Group>
        <Row
          id="placement"
          label="Open the Palette on"
          applied={placementApplied.applied}
          error={placementApplied.error}
          description="Read on every summon, so unplugging a screen cannot strand the window off the desktop."
        >
          <Chips
            label="Open the Palette on"
            value={placement}
            options={PLACEMENTS}
            onChange={(next) => void placementApplied.apply(next, setPlacementState)}
          />
        </Row>
        <Row
          id="tray"
          label="Show the tray icon"
          applied={trayApplied.applied}
          error={trayApplied.error}
          description="The Palette has no taskbar button, so the tray is the other way in — and the only way to quit."
        >
          <Switch
            label="Show the tray icon"
            checked={tray}
            onChange={(on) => void trayApplied.apply(on, setTrayState)}
          />
        </Row>
      </Group>

      <Group title="Sources">
        <Row
          id="recents"
          label="Include recent files"
          applied={recentsApplied.applied}
          error={recentsApplied.error}
          description="Documents from Windows' own Recent folder, read on a timer rather than per keystroke. Off, nothing else about search changes."
        >
          <Switch
            label="Include recent files"
            checked={recents}
            onChange={(on) => void recentsApplied.apply(on, setRecentsState)}
          />
        </Row>
      </Group>
    </>
  );
}
