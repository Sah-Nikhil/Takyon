/**
 * About: what this copy of Takyon is, and whether its hotkey is live.
 *
 * The hotkey line is here rather than on Keyboard because Keyboard is where you
 * *change* it and this is where you find out it never registered — which is the
 * state someone is in when they come looking, having pressed Alt+Space and had
 * nothing happen.
 */

import { useEffect, useState } from "react";
import { Lockup } from "@/components/Lockup";
import * as api from "@/api";
import type { HotkeyStatus } from "@takyon/shared";
import { Group, Row } from "../controls";

/** ADR-0011: the slug is what Windows keys off, and it is not the display name. */
const IDENTITY = "com.v3sper.launcher";

export function About() {
  const [hotkey, setHotkey] = useState<HotkeyStatus | null>(null);

  useEffect(() => {
    void api.hotkeyStatus().then(setHotkey);
  }, []);

  return (
    <>
      <div className="mb-8 flex flex-col items-center gap-3 pt-4">
        <Lockup size={30} />
        <p className="text-[12.5px] text-fg/45">Version {__APP_VERSION__}</p>
      </div>

      <Group title="Diagnostics">
        <Row
          id="hotkey-status"
          label="Global hotkey"
          description={
            hotkey && !hotkey.registered
              ? hotkey.error
              : "Press it anywhere in Windows to open the Palette."
          }
        >
          <span
            className={`font-mono text-[12.5px] ${
              hotkey && !hotkey.registered ? "text-amber-300" : "text-fg/70"
            }`}
          >
            {hotkey?.accelerator ?? "…"}
          </span>
        </Row>
        <Row
          id="identity"
          label="Package identity"
          description="What the startup entry, the single-instance lock and the data folder are named after. Independent of the display name (ADR-0011)."
        >
          <span className="font-mono text-[12.5px] text-fg/70">{IDENTITY}</span>
        </Row>
      </Group>
    </>
  );
}
