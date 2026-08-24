/**
 * The Settings window — a placeholder until v0.6, which is when settings becomes
 * real work (a settings.db, two-tier navigation, a search box).
 *
 * Two live controls: autostart, and the motion switch. Both are here for the same
 * reason — v0.1 ships the behaviour they govern, and a behaviour the user cannot
 * turn off is not a setting, it is a decision made on their behalf.
 *
 * It is a genuine second Tauri window with its own label and capability file
 * rather than a disabled tray item, because the multi-window seam and the second
 * capability file are a ten-minute job now and a source of surprises at v0.6.
 *
 * The one live control is autostart, and it is here rather than deferred because
 * v0.1 has to register autostart anyway — a switch that writes to the OS with no
 * way to read it back is how you end up with an orphan `Run` key.
 */

import { useCallback, useEffect, useState } from "react";
import { Lockup } from "@/components/Lockup";
import * as api from "@/api";
import { reduceMotion, setReduceMotion, systemReducesMotion } from "@/prefs";
import type { HotkeyStatus } from "@takyon/shared";

/**
 * Dev builds must never register autostart. A debug registration writes a `Run`
 * key pointing at `target\debug\` which survives uninstalling the real app, and
 * then launches a dev build every login. The Rust side is gated with
 * `#[cfg(not(debug_assertions))]`; this is the other half.
 */
const DEV = import.meta.env.DEV;

export function Settings() {
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [hotkey, setHotkey] = useState<HotkeyStatus | null>(null);
  const [stillMotion, setStillMotion] = useState(reduceMotion);
  // Read once. Windows can change it mid-session, but this line is copy, not
  // behaviour — the media query in styles.css enforces the OS setting either way.
  const [osStill] = useState(systemReducesMotion);

  // Read the OS on mount, every mount. Autostart state lives in the OS and is
  // never mirrored into our own storage: Task Manager → Startup apps flips it
  // behind the app's back with no event to observe, so a cached copy would be
  // confidently wrong (tesseract ADR-0026).
  useEffect(() => {
    void api.autostartIsEnabled().then(setAutostart);
    void api.hotkeyStatus().then(setHotkey);
  }, []);

  const toggle = useCallback(async () => {
    if (DEV || autostart === null) return;
    const next = !autostart;
    await api.autostartSetEnabled(next);
    // Re-read rather than trusting the write. If the registry write was refused,
    // the switch should snap back rather than lie.
    setAutostart(await api.autostartIsEnabled());
  }, [autostart]);

  const toggleMotion = useCallback((on: boolean) => {
    setReduceMotion(on);
    setStillMotion(on);
  }, []);

  return (
    <div className="h-full w-full overflow-y-auto bg-plate p-8 text-fg">
      <header className="mb-8">
        <Lockup size={26} />
      </header>

      <section className="max-w-xl space-y-6">
        <label className="flex items-start justify-between gap-6">
          <span>
            <span className="block text-[15px]">Start Takyon when I log in</span>
            <span className="mt-1 block text-[13px] text-fg/50">
              {DEV
                ? "Unavailable in a development build. Registering here would write a startup entry pointing at target\\debug\\, which would survive uninstalling the real app."
                : "The launcher needs to already be running for the hotkey to answer."}
            </span>
          </span>
          <input
            type="checkbox"
            disabled={DEV || autostart === null}
            checked={autostart ?? false}
            onChange={() => void toggle()}
            className="mt-1 size-4 shrink-0 accent-accent disabled:opacity-40"
          />
        </label>

        <label className="flex items-start justify-between gap-6">
          <span>
            <span className="block text-[15px]">Turn off animations</span>
            <span className="mt-1 block text-[13px] text-fg/50">
              {osStill
                ? "Windows is already set to reduce motion, so the mark is holding still regardless of this switch."
                : "The mark breathes while the Palette is open and waiting for a query. Nothing else in Takyon moves."}
            </span>
          </span>
          <input
            type="checkbox"
            checked={stillMotion}
            onChange={(e) => toggleMotion(e.target.checked)}
            className="mt-1 size-4 shrink-0 accent-accent"
          />
        </label>

        <div className="border-t border-white/10 pt-6 text-[13px] text-fg/50">
          <p>
            Hotkey:{" "}
            <span className="font-mono text-fg/80">{hotkey?.accelerator ?? "…"}</span>
            {hotkey && !hotkey.registered && (
              <span className="text-amber-300"> — not registered. {hotkey.error}</span>
            )}
          </p>
          <p className="mt-3">
            Everything else arrives in v0.6: hotkey rebinding, appearance, index roots,
            clipboard retention and the rest.
          </p>
        </div>
      </section>
    </div>
  );
}
