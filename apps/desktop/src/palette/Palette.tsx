/**
 * The Palette. v0.1 was an input row; v0.2 fills in the list underneath it.
 *
 * Two rules are load-bearing and easy to lose in a later edit. **Nothing here
 * calls `invoke()`** — everything goes through `api.ts`, which is what keeps
 * TBC-0007's visual layer working. And **the Palette hides before anything
 * launches**, with Rust doing the hiding inside `activate`; a second hide here
 * would race it.
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { Command } from "cmdk";
import {
  CALC_CAPTION_HEIGHT,
  CALC_CARD_HEIGHT,
  LIST_CHROME,
  MAX_VISIBLE_ROWS,
  ROW_HEIGHT,
  type Action,
  type Entry,
  type EntryKind,
} from "@takyon/shared";
import { InputMark } from "@/components/Mark";
import * as api from "@/api";
import { applyMotionPreference, calcPolicy, watchMotionPreference } from "@/prefs";
import type { HotkeyStatus } from "@takyon/shared";
import { CalcCard } from "./CalcCard";
import { EntryRow } from "./EntryRow";
import { ActionMenu } from "./ActionMenu";

export function Palette() {
  const [value, setValue] = useState("");
  const [entries, setEntries] = useState<Entry[]>([]);
  const [indexing, setIndexing] = useState(false);
  const [selected, setSelected] = useState("");
  const [menu, setMenu] = useState<Action[] | null>(null);
  const [hotkey, setHotkey] = useState<HotkeyStatus | null>(null);
  /*
    Whether the window is on screen. Created hidden in Tauri and alive between
    summons (docs/tbc/0002), so this starts false and the show event flips it;
    in the browser there is no window and the app is visible from first paint.
    It exists to stop the idle pulse animating against a hidden window.
   */
  const [shown, setShown] = useState(!api.inTauri);
  const inputRef = useRef<HTMLInputElement>(null);
  const bannerRef = useRef<HTMLDivElement>(null);

  /*
    Sequence numbers (IMPLEMENTATION_PLAN §3).
    
    Refs, not state: both are read and written inside the same async callback,
    and a state update would not be visible to a response arriving before React
    re-renders — exactly the fast-keystroke case this exists to handle.
   */
  const nextSeq = useRef(1);
  const newestSeen = useRef(0);

  const runQuery = useCallback((q: string) => {
    const seq = nextSeq.current++;
    void api.query(q, seq).then((result) => {
      if (result.seq < newestSeen.current) return;
      newestSeen.current = result.seq;
      setEntries(result.entries);
      setIndexing(result.indexing);
      // Selection follows the top Entry on every new result set. From v0.3 the
      // Stability rule freezes it ~100 ms after the last keystroke so that a late
      // Source cannot move what Enter is about to launch; until then, "the top
      // one" is the whole rule.
      setSelected(result.entries[0]?.id ?? "");

      // §10's "hotkey to first Entry" budget. Reported only when an Entry is
      // actually on screen — the empty query on every show would otherwise
      // report a paint of nothing and flatter the number badly.
      if (result.entries.length > 0) {
        requestAnimationFrame(() => {
          requestAnimationFrame(() => void api.reportFirstEntry(result.seq));
        });
      }
    });
  }, []);

  useEffect(() => {
    void api.hotkeyStatus().then(setHotkey);
  }, []);

  // Rust holds the calculator's Mode because the rule is enforced inside the
  // Source, on the keystroke path. Pushing it once on mount is what restores the
  // remembered choice after a restart; Settings pushes again when it changes.
  useEffect(() => {
    void api.setCalcPolicy(calcPolicy());
  }, []);

  useEffect(() => {
    runQuery(value);
  }, [value, runQuery]);

  useEffect(() => {
    return api.onShow((payload) => {
      // ROADMAP v0.1: the Palette always opens empty. Nothing is remembered
      // between invocations, deliberately (ADR-0001).
      setValue("");
      setEntries([]);
      setMenu(null);
      setShown(true);
      // The Settings window may have flipped the motion switch while the Palette
      // was hidden. Re-reading here is what makes the two windows agree without
      // any cross-window plumbing (see prefs.ts).
      applyMotionPreference();
      // Same sync point, same reason: Settings may have changed the calculator's
      // Mode while the Palette was hidden, and the next keystroke is about to ask
      // Rust a question that depends on it.
      void api.setCalcPolicy(calcPolicy());
      inputRef.current?.focus();

      // Two frames, not one. The first rAF callback runs *before* the browser
      // paints, so reporting there measures asking for a frame rather than
      // having one. The second fires after the paint was committed.
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
    return api.onHide(() => {
      setShown(false);
      setMenu(null);
      // Cleared on hide, not just on the next show. `window::hide` resets the
      // shape and shrinks the window to one row, so eight rows left mounted
      // behind it disagree with the window for as long as it stays hidden.
      // No `setActionMenu(null)`: `reset_shape` already did that.
      setValue("");
      setEntries([]);
    });
  }, []);

  useEffect(watchMotionPreference, []);

  /*
    Measure the hotkey-failure banner and tell the window how tall it is.
    
    Wrapping text below the list in a flex column, so a too-short window takes the
    difference out of the list and clips its last Entry. A ResizeObserver, not one
    measurement on mount: a DPI change re-wraps without remounting.
   */
  useEffect(() => {
    const el = bannerRef.current;
    if (!el) {
      // No banner: report zero so a window still holding space for a previous one
      // shrinks back rather than showing an empty strip.
      void api.setBannerHeight(0);
      return;
    }
    const report = () => void api.setBannerHeight(el.getBoundingClientRect().height);
    report();
    const observer = new ResizeObserver(report);
    observer.observe(el);
    return () => observer.disconnect();
  }, [hotkey]);

  const closeMenu = useCallback(() => {
    setMenu(null);
    void api.setActionMenu(null);
  }, []);

  const run = useCallback(
    (entryId: string, actionId: string) => {
      if (!entryId) return;
      closeMenu();
      // No hide here. Rust hides the Palette inside `activate`, before it asks the
      // shell for anything, so the window is gone before the application starts
      // painting (v0.2 task 7).
      void api.activate(entryId, actionId);
    },
    [closeMenu],
  );

  const openMenu = useCallback(() => {
    if (!selected) return;
    void api.actionsFor(selected).then((actions) => {
      // An Entry with no actions gets no menu, rather than an empty box. An empty
      // popover reads as a bug; nothing happening reads as "not applicable here".
      if (actions.length === 0) return;
      setMenu(actions);
      // The window has to grow before the menu is drawn into it, or its last
      // rows fall outside the native window entirely. Rust owns that, because it
      // is the window that is too short and nothing in the webview can see it.
      void api.setActionMenu(actions.length);
    });
  }, [selected]);

  // Bound to the document, not the <Command> element.
  //
  // A React `onKeyDown` only fires for events bubbling from inside it, so Escape
  // did nothing whenever focus sat on `body` — which happens after a hide/show
  // cycle. Escape is one of three ways out; it cannot depend on that.
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        // The menu closes first. Escape means "back one step", and dismissing the
        // whole Palette from an open menu loses the query as well as the menu.
        if (menu) return; // ActionMenu stops propagation and handles its own.
        e.preventDefault();
        void api.dismiss();
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [menu]);

  /*
    Modifier accelerators, matching Rust's `actions::for_modifiers`.
    
    The chord table lives in Rust so that it is one definition rather than two —
    this reads the state and names the action, it does not decide what the action
    means. Rebinding at v0.6 changes the Rust table and this keeps working.
   */
  const actionForEvent = (e: React.KeyboardEvent, kind: EntryKind | undefined) => {
    // Kind-aware since v0.4, matching `actions::for_modifiers`. A calculation has
    // nothing to open, elevate or reveal, so every chord copies its answer rather
    // than reaching an action it does not have.
    if (kind === "calc") return "copy_answer";
    if (e.ctrlKey && e.shiftKey) return "reveal";
    if (e.ctrlKey) return "run_as_admin";
    return "open";
  };

  /** The Kind of the row Enter would act on. */
  const selectedKind = entries.find((e) => e.id === selected)?.kind;

  /*
    The height Rust reserved for the list, chrome included.
    
    `LIST_CHROME` is not decoration: border-box with an explicit height puts the
    `py-1` padding and the 1px top border *inside* it, so `rows * ROW_HEIGHT`
    alone clips the last row and grows a scrollbar on a list that fits.
   */
  /*
    A calculation is drawn as a card, not a row, so it is subtracted before the
    cap: eight rows *plus* a card is taller than the shape TBC-0006 chose. Rust
    computes the same number in `window::content_height`, and a test asserts the
    constants agree — this side only decides how tall the list box is drawn.
   */
  const calcCard = entries[0]?.kind === "calc";
  const listRows = Math.min(
    Math.max(entries.length - (calcCard ? 1 : 0), 0),
    MAX_VISIBLE_ROWS,
  );
  const listHeight =
    (calcCard ? CALC_CAPTION_HEIGHT + CALC_CARD_HEIGHT : 0) +
    listRows * ROW_HEIGHT +
    LIST_CHROME;
  const showList = entries.length > 0 || (indexing && value.trim().length > 0);

  return (
    <div className="relative flex h-full w-full flex-col p-2">
      <Command
        shouldFilter={false}
        value={selected}
        onValueChange={setSelected}
        className="overflow-hidden rounded-xl border border-white/10 bg-plate/95 shadow-2xl backdrop-blur-xl"
        onKeyDown={(e) => {
          if (menu) return;
          if (e.key === "k" && e.ctrlKey) {
            e.preventDefault();
            openMenu();
            return;
          }
          // Every non-Enter accelerator in `actions.rs`'s table needs a branch
          // here, or the menu advertises a shortcut that does nothing. A Rust
          // test asserts the two sides still agree.
          if (e.key.toLowerCase() === "c" && e.ctrlKey && e.shiftKey) {
            e.preventDefault();
            // A calculation has no path, so this chord is not offered on one and
            // must not be sent: Rust refuses it, which would surface as an error
            // toast for a keystroke that should have done nothing.
            if (selectedKind !== "calc") run(selected, "copy_path");
            return;
          }
          if (e.key === "Enter") {
            e.preventDefault();
            run(selected, actionForEvent(e, selectedKind));
          }
        }}
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
          {entries.length > 0 && (
            <kbd className="shrink-0 rounded border border-white/10 px-1.5 py-0.5 text-[10px] text-fg/35">
              Ctrl K
            </kbd>
          )}
        </div>

        {/*
          The list is capped at twelve Entries by the ranker (§3) and shows eight
          at a time, so there is nothing here worth virtualising: a windowing
          library would add a dependency and a measurement pass to avoid rendering
          four rows. The ROADMAP asks for a virtualised list, and this is the
          honest reading of that requirement at this cap — revisit it the day a
          Source returns an unbounded set, which by §3 is never on this path.
        */}
        {showList && (
          <Command.List
            style={{ height: entries.length > 0 ? listHeight : ROW_HEIGHT + LIST_CHROME }}
            /*
              `px-2` so a selected row's rounded background insets from the panel
              edge. Without it the highlight ran full width, its corners were
              never visible, and it collided with the border. The 8px here plus
              the row's own 8px is the input row's `px-4`, which is what puts an
              icon directly under the mark.
             */
            className="overflow-y-auto border-t border-white/5 px-2 py-1"
          >
            {indexing && entries.length === 0 && (
              <div
                className="flex items-center px-2 text-[13px] text-fg/40"
                style={{ height: ROW_HEIGHT }}
              >
                Indexing applications…
              </div>
            )}
            {entries.map((entry) => (
              <Command.Item
                key={entry.id}
                value={entry.id}
                // Not a hardcoded "open": a calculation has nothing to open, and
                // Rust refuses that action, so a click would silently do nothing.
                onSelect={() => run(entry.id, entry.kind === "calc" ? "copy_answer" : "open")}
                // A calculation carries its own selected state, on the card
                // rather than on this wrapper, which also holds the caption.
                className={
                  entry.kind === "calc"
                    ? "cursor-default"
                    : "cursor-default rounded-md data-[selected=true]:bg-white/10"
                }
              >
                {entry.kind === "calc" ? (
                  <CalcCard entry={entry} selected={entry.id === selected} />
                ) : (
                  <EntryRow entry={entry} selected={entry.id === selected} />
                )}
              </Command.Item>
            ))}
          </Command.List>
        )}
      </Command>

      {menu && (
        <ActionMenu
          actions={menu}
          onRun={(actionId) => run(selected, actionId)}
          onClose={() => {
            closeMenu();
            inputRef.current?.focus();
          }}
        />
      )}

      {/*
        IMPLEMENTATION_PLAN §7: a taken hotkey must be *reported*, never silently
        swallowed. A dialog fires at startup as well, but that one is dismissable
        and this one is not — if the binding is dead, the surface it was supposed
        to open should say so every time it is reached by any other route.
      */}
      {hotkey && !hotkey.registered && (
        <div
          ref={bannerRef}
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
