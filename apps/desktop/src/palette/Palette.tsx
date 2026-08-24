/**
 * The Palette. In v0.1 it is an input row and nothing else — no Entries, no
 * Sources, no ranking. The phase exists to test the warm-window bet, not to ship
 * features (docs/plans/v0.1-warm-shell.md).
 *
 * It is built on `cmdk`'s `Command` from the first commit even though there is
 * nothing to list yet, so v0.2 is a fill-in rather than a rewrite of the input's
 * keyboard handling.
 */

import { useEffect, useRef, useState } from "react";
import { Command } from "cmdk";
import { InputMark } from "@/components/Mark";
import * as api from "@/api";
import { applyMotionPreference, watchMotionPreference } from "@/prefs";
import type { HotkeyStatus } from "@takyon/shared";

export function Palette() {
  const [value, setValue] = useState("");
  const [hotkey, setHotkey] = useState<HotkeyStatus | null>(null);
  /*
    Whether the window is on screen. In Tauri the Palette is created hidden and
    stays alive between summons (docs/tbc/0002), so this starts false and the
    show event flips it; in the browser there is no window to hide and the app is
    visible from first paint. It exists only to keep the idle pulse from
    animating against a hidden window, which would burn a frame budget forever
    for something nobody can see.
  */
  const [shown, setShown] = useState(!api.inTauri);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    void api.hotkeyStatus().then(setHotkey);
  }, []);

  useEffect(() => {
    return api.onShow((payload) => {
      // ROADMAP v0.1: the Palette always opens empty. Nothing is remembered
      // between invocations, deliberately (ADR-0001).
      setValue("");
      setShown(true);
      // The Settings window may have flipped the motion switch while the Palette
      // was hidden. Re-reading here is what makes the two windows agree without
      // any cross-window plumbing (see prefs.ts).
      applyMotionPreference();
      inputRef.current?.focus();

      // Two frames, not one. The first rAF callback runs *before* the browser
      // paints; reporting there would measure the moment we asked for a frame
      // rather than the moment one existed. The second fires after that paint has
      // been committed, which is the closest honest proxy for "first pixel" the
      // renderer can give us.
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          // Focused twice, here and above. The synchronous call is the one that
          // usually wins; this one covers the case where the webview had not yet
          // been given keyboard focus by the host when the event arrived, which
          // otherwise leaves the caret invisible and the first keystroke lost.
          inputRef.current?.focus();
          void api.reportFirstPixel(payload.showId);
        });
      });
    });
  }, []);

  useEffect(() => {
    return api.onHide(() => setShown(false));
  }, []);

  useEffect(watchMotionPreference, []);

  // Bound to the document, not to the <Command> element.
  //
  // A React `onKeyDown` on the container only fires for events that bubble from
  // inside it, so Escape did nothing whenever focus sat on `body` — which happens
  // after a hide/show cycle, before the input has been refocused. Escape is one of
  // only three ways out of the Palette; it cannot be conditional on which element
  // inside the webview happens to hold focus.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        void api.dismiss();
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, []);

  return (
    <div className="flex h-full w-full flex-col p-2">
      <Command
        shouldFilter={false}
        className="overflow-hidden rounded-xl border border-white/10 bg-plate/95 shadow-2xl backdrop-blur-xl"
      >
        <div className="flex items-center gap-3 px-4">
          {/*
            The particle breathes only while the Palette is up and the query is
            still empty. Both halves matter: pulsing while hidden animates
            nothing anyone can see, and pulsing while the user types would read
            as "working" when the shell is in fact idle.
          */}
          <InputMark pulse={shown && value === ""} className="shrink-0 text-fg/45" />
          <Command.Input
            ref={inputRef}
            value={value}
            onValueChange={setValue}
            autoFocus
            placeholder="Search"
            className="h-12 w-full bg-transparent text-[15px] text-fg outline-none placeholder:text-fg/35"
          />
        </div>

        {/*
          Empty in v0.1. `Command.List` is mounted anyway so that the window's
          content height is already driven by the list, which is what TBC-0006's
          content-sized window will grow against in v0.2.
        */}
        <Command.List />
      </Command>

      {/*
        IMPLEMENTATION_PLAN §7: a taken hotkey must be *reported*, never silently
        swallowed. A dialog fires at startup as well, but that one is dismissable
        and this one is not — if the binding is dead, the surface it was supposed
        to open should say so every time it is reached by any other route.
      */}
      {hotkey && !hotkey.registered && (
        <div
          role="alert"
          className="mt-2 rounded-lg border border-amber-400/30 bg-amber-400/10 px-4 py-2 text-[13px] text-amber-200"
        >
          <span className="font-medium">{hotkey.accelerator} could not be registered.</span>{" "}
          {hotkey.error ?? "Another application is holding it."} Takyon can still be opened
          from the tray icon.
        </div>
      )}
    </div>
  );
}
