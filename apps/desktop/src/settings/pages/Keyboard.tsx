/**
 * Keyboard: the two ways to open the Palette.
 *
 * They are two because they are two *mechanisms*, not two preferences. The chord
 * is an accelerator, registered with `RegisterHotKey` through
 * `tauri-plugin-global-shortcut`. The Windows key cannot be one — it is a
 * modifier, so there is no chord to register — and needs a `WH_KEYBOARD_LL`
 * hook instead, with its own failure modes and its own reasons to be off by
 * default. `superkey.rs` carries the argument.
 *
 * Both controls read the **response**, never the click: a refused chord leaves
 * the previous binding live and a refused hook leaves nothing installed, and in
 * each case the control has to settle where the truth is.
 */

import { useCallback, useEffect, useState } from "react";
import type { ContestedChord, HotkeyStatus } from "@takyon/shared";
import * as api from "@/api";
import { Select } from "@/components/Select";
import { preferences, setSuperHotkey } from "@/prefs";
import { Group, Row, Switch } from "../controls";

/** How often the takeover step re-attempts the contested chord. */
const CLAIM_POLL_MS = 700;

/** How long "Applied" stays up. Matches `controls.tsx`'s own. */
const APPLIED_MS = 1400;

export function Keyboard() {
  const [choices, setChoices] = useState<string[]>([]);
  const [status, setStatus] = useState<HotkeyStatus | null>(null);
  const [applied, setApplied] = useState(false);
  const [superKey, setSuperKey] = useState(() => preferences().superHotkey);
  const [superError, setSuperError] = useState<string | null>(null);
  const [contested, setContested] = useState<ContestedChord | null>(null);
  const [platform, setPlatform] = useState<string>("other");

  useEffect(() => {
    void api.hotkeyChoices().then(setChoices);
    void api.hotkeyStatus().then(setStatus);
    void api.contestedChord().then(setContested);
    void api.settingsSnapshot().then((snap) => setPlatform(snap.platform));
  }, []);

  const bind = useCallback(async (accelerator: string) => {
    setApplied(false);
    // The response is the truth: `registered` with an `error` set means the new
    // chord was refused and the old one was kept.
    const next = await api.setHotkey(accelerator);
    setStatus(next);
    if (!next.error) {
      setApplied(true);
      setTimeout(() => setApplied(false), APPLIED_MS);
    }
  }, []);

  /*
    The switch settles on the hook, not on the click. `SetWindowsHookExW` can
    refuse — policy, another process on the chain, a session with no desktop —
    and a switch reading on against nothing is the worst of the three states
    this control can be in.
   */
  const toggleSuper = useCallback(async (on: boolean) => {
    setSuperError(null);
    setSuperKey(on);
    const live = await setSuperHotkey(on);
    setSuperKey(live);
    if (live !== on) {
      setSuperError(
        "Windows refused the keyboard hook, so nothing was bound. The chord below still works.",
      );
    }
  }, []);

  // Rust orders `CHOICES` with the platform default first, so the reset button
  // offers the right chord without a second copy of it here — the previous
  // literal said Alt+Space, which is Option+Space on macOS.
  const fallback = choices[0] ?? "";
  const live = status?.accelerator ?? fallback;
  const held = contested !== null && live === contested.accelerator && !status?.registered;

  /*
    Poll rather than offer an "I've done it" button. Registration starts
    succeeding the instant Spotlight's shortcut is unchecked, so this advances
    on the real thing — nothing to lie to, and no dead end.
  */
  useEffect(() => {
    if (!held) return;
    let live = true;
    const timer = setInterval(() => {
      void api.claimContestedChord().then((next) => {
        if (live && next.registered) setStatus(next);
      });
    }, CLAIM_POLL_MS);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [held]);


  return (
    <Group>
      <Row
        id="hotkey"
        label="Open Takyon with"
        applied={applied}
        error={status?.error ?? null}
        description={
          status && !status.registered
            ? "Nothing is bound. Pick another chord — the launcher is running, but only the tray can reach it."
            : platform === "macos"
              ? "Pressed anywhere. Carbon hotkeys are known not to fire inside some self-drawn terminals, such as Zed and VS Code."
              : "Pressed anywhere in Windows. Alt+Space is contested: PowerToys Run and the classic window menu both want it."
        }
      >
        {/*
          A dropdown since v0.10, where six chips used to sit. The chips were the
          whole control once; now there is a switch above them and a list that
          reads as one choice belongs in a list. `Select` is already
          keyboard-complete — arrows, typeahead, Home, End.
        */}
        <Select
          label="Open Takyon with"
          value={live}
          options={choices.map((c) => ({ value: c, label: c.replace(/\+/g, " + ") }))}
          onChange={(next) => next && void bind(next)}
          className="w-48"
        />
        <button
          type="button"
          onClick={() => void bind(fallback)}
          disabled={!fallback || live === fallback}
          className="rounded-control px-2 py-1 text-[12.5px] text-fg/64 transition-colors hover:bg-row-hover hover:text-fg/86 disabled:opacity-30 disabled:hover:bg-transparent"
        >
          Reset
        </button>
      </Row>


      {held && contested && (
        <Row
          id="release-contested-chord"
          label={`${contested.accelerator.replace(/\+/g, " + ")} is held by the system`}
          description="Spotlight has it, and no application can take it programmatically. Uncheck “Show Spotlight search” in Keyboard Shortcuts and Takyon binds it the moment you do — there is no button to press here afterwards."
        >
          <button
            type="button"
            onClick={() => void api.openContestedChordPane()}
            className="rounded-control bg-control px-2.5 py-1 text-[12.5px] text-fg/86 transition-colors hover:text-fg"
          >
            Open Keyboard Shortcuts
          </button>
        </Row>
      )}

      {platform !== "macos" && (
      <Row
        id="super-hotkey"
        label="Open Takyon with the Windows key"
        error={superError}
        description="Tapping it opens the Palette instead of the Start menu. Holding it is untouched — Win+R, Win+E and Win+L all still work. Like the chord above, it cannot reach a window running as administrator."
      >
        <Switch
          label="Open Takyon with the Windows key"
          checked={superKey}
          onChange={(on) => void toggleSuper(on)}
        />
      </Row>
      )}
    </Group>
  );
}
