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
import { refresh } from "@/prefs";
import type {
  Ask,
  Web as WebMode,
  AgentKind,
  AgentSnapshot,
  FileIndexReport,
  HotkeyStatus,
  ViewKind,
} from "@takyon/shared";
import { AGENT_LABELS, agentSummary, blockedReason, pickAgent } from "@/agents/status";
import { AskView } from "./AskView";
import { SearchView } from "./SearchView";
import { CalcCard } from "./CalcCard";
import { ClipboardHistory } from "./ClipboardHistory";
import { Footer } from "./Footer";
import { EntryRow } from "./EntryRow";
import { ActionMenu } from "./ActionMenu";

/**
 * What a click does, per Kind. Mirrors `actions::for_modifiers` with no modifier.
 *
 * Not a hardcoded "open": a calculation has nothing to open and a Clip has no
 * file, and Rust refuses both, so a click would silently do nothing.
 */
const PRIMARY_ACTION: Partial<Record<EntryKind, string>> = {
  calc: "copy_answer",
  clip: "paste",
  command: "open_command",
};

export function Palette() {
  const [value, setValue] = useState("");
  const [entries, setEntries] = useState<Entry[]>([]);
  const [indexing, setIndexing] = useState(false);
  /*
    The file index's state, read only while `!e` is being typed (v0.7 task 7).
    Off the keystroke path deliberately: it changes on the walk's schedule, not
    the user's, so riding `query` would ship the same three words per keypress.
   */
  const [fileIndex, setFileIndex] = useState<FileIndexReport | null>(null);
  const [selected, setSelected] = useState("");
  const [menu, setMenu] = useState<Action[] | null>(null);
  const [hotkey, setHotkey] = useState<HotkeyStatus | null>(null);
  /*
    The full-window surface, when one is open (v0.5). Null is the root search.
    Rust holds the same answer because the *native window* has to resize, which
    nothing in here can do.
   */
  const [view, setView] = useState<ViewKind | null>(null);
  /** Action labels for the footer, by id. Fetched once; Rust owns the words. */
  const [labels, setLabels] = useState<Record<string, Action>>({});
  /*
    Whether the window is on screen. Created hidden in Tauri and alive between
    summons (docs/tbc/0002), so this starts false and the show event flips it;
    in the browser there is no window and the app is visible from first paint.
    It exists to stop the idle pulse animating against a hidden window.
   */
  const [shown, setShown] = useState(!api.inTauri);
  /*
    The `!c` Mode's state for this keystroke, or null. Rust decides, because
    `bang.rs` owns the grammar and which Agent answers is a stored preference.
   */
  const [ask, setAsk] = useState<Ask | null>(null);
  /*
    Every Agent's Sign-in state, read only while `!c` is being typed. Off the
    keystroke path for the same reason `fileIndex` is, and more so: a probe is
    three process spawns (v0.8 Traps).
   */
  const [agents, setAgents] = useState<AgentSnapshot[] | null>(null);
  /**
   * The question the Ask view is answering, and who is answering it. Null when
   * the view is closed. Resolved at Enter, so a later probe cannot move it.
   */
  const [asking, setAsking] = useState<{ agent: AgentKind; query: string } | null>(null);
  /*
    The `!s` Mode's state for this keystroke, or null. Rust decides, exactly as
    it does for `!c`, and no request is made while it is being typed (ADR-0002).
   */
  const [web, setWeb] = useState<WebMode | null>(null);
  /** The query the search view is answering. Null when the view is closed. */
  const [searching, setSearching] = useState<string | null>(null);
  const filesBang = value.trimStart().toLowerCase().startsWith("!e");
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

  /*
    Asked when `!e` is typed, and re-asked while it is not Ready. A Stale index
    must say so rather than serve what it happens to remember (ADR-0007), and
    Building must not read as "no such file".
   */
  useEffect(() => {
    // Left as it was when `!e` was last typed rather than cleared here: the note
    // below is gated on `filesBang` anyway, and clearing synchronously in an
    // effect is a render-phase write.
    if (!filesBang) return;
    let live = true;
    const read = () => {
      void api.fileIndexStatus().then((r) => {
        if (live) setFileIndex(r);
      });
    };
    read();
    const timer = setInterval(read, 1500);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [filesBang]);

  const runQuery = useCallback((q: string) => {
    const seq = nextSeq.current++;
    void api.query(q, seq).then((result) => {
      if (result.seq < newestSeen.current) return;
      newestSeen.current = result.seq;
      setEntries(result.entries);
      setIndexing(result.indexing);
      setAsk(result.ask ?? null);
      setWeb(result.web ?? null);
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

  /*
    Probed on the first `!c` of a summon, then reused. Once rather than on an
    interval, unlike the file index: an Agent's Sign-in state changes when the
    user runs a CLI command, not while they type.
   */
  useEffect(() => {
    if (ask === null || agents !== null) return;
    let live = true;
    void api.agentSnapshots().then((all) => {
      if (live) setAgents(all);
    });
    return () => {
      live = false;
    };
  }, [ask, agents]);

  useEffect(() => {
    void api
      .actionLabels()
      .then((all) => setLabels(Object.fromEntries(all.map((a) => [a.id, a]))));
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
      setView(null);
      setAsk(null);
      setAsking(null);
      setWeb(null);
      setSearching(null);
      // Dropped rather than kept: an Agent signed in or out through its CLI
      // between two summons, and a stale card is worse than a second probe.
      setAgents(null);
      setShown(true);
      // The Settings window may have written a preference while the Palette was
      // hidden. Re-reading here is what makes the two windows agree without any
      // cross-window plumbing (see prefs.ts). The calculator's Mode is no longer
      // pushed back: since v0.6 Rust stores it and read it at startup.
      void refresh();
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
      // `window::reset_shape` already cleared the View on the Rust side, so this
      // only catches the React half up.
      setView(null);
      // Cleared on hide, not just on the next show. `window::hide` resets the
      // shape and shrinks the window to one row, so eight rows left mounted
      // behind it disagree with the window for as long as it stays hidden.
      // No `setActionMenu(null)`: `reset_shape` already did that.
      setValue("");
      setEntries([]);
      setAsk(null);
      // The Ask view goes with the window. A Turn it started does not — only
      // `agentCancel` stops one, and `useTurn` fires that on unmount.
      setAsking(null);
      setWeb(null);
      setSearching(null);
    });
  }, []);


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

  /*
    Rust is told too, and is not optional: the native window has to grow to the
    surface's height, which nothing in the webview can do (TBC-0006).
   */
  const openView = useCallback((next: ViewKind) => {
    setView(next);
    void api.setView(next);
  }, []);

  const closeView = useCallback(() => {
    setView(null);
    void api.setView(null);
    setAsking(null);
    // Back to an empty root, which is where every summon starts (ADR-0001).
    setValue("");
    setEntries([]);
  }, []);

  /*
    Enter on the `!c` row, never a keystroke: a Turn is a process, and asking as
    the user types would spawn one per character.
   */
  const startAsk = useCallback(() => {
    if (!ask || !ask.query) return;
    // The resolved Agent, not the first preference: the row already names the
    // one that will answer, and the view must ask that same one.
    const agent = pickAgent(ask.order, agents);
    if (!agent) return;
    // Refuses only on a probe that came back and said no. Before it lands the
    // Turn goes ahead and the Agent's own error is the answer.
    if (blockedReason(agents?.find((a) => a.kind === agent))) return;
    setAsking({ agent, query: ask.query });
    setView("ask");
    void api.setView("ask");
  }, [ask, agents]);

  /*
    Enter on the `!s` row. A search is a paid request and an Agent Turn, so it
    happens once, on Enter, never per keystroke (v0.9 Traps).
   */
  const startSearch = useCallback(() => {
    if (!web || !web.query || !web.hasKey) return;
    setSearching(web.query);
    setView("web");
    void api.setView("web");
  }, [web]);

  const closeMenu = useCallback(() => {
    setMenu(null);
    void api.setActionMenu(null);
  }, []);

  const run = useCallback(
    (entryId: string, actionId: string) => {
      if (!entryId) return;
      closeMenu();
      // A command navigates into a surface instead of launching, so it never
      // reaches the shell and the window stays up.
      if (actionId === "open_command") {
        void api.activate(entryId, actionId);
        openView("clipboard-history");
        return;
      }
      // No hide here either: Rust hides inside `activate`, before it asks the
      // shell for anything, so the window is gone before the app paints.
      const done = api.activate(entryId, actionId);
      // Deleting a clip is the one action Rust leaves the Palette open for, so
      // the list has to be asked again or the deleted row stays on screen.
      if (actionId === "delete_clip") void done.then(() => runQuery(value));
      else void done;
    },
    [closeMenu, openView, runQuery, value],
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
        // Same rule one level up: a surface goes back to the search that opened
        // it rather than dismissing the window.
        if (view) return; // ClipboardHistory stops propagation and handles its own.
        e.preventDefault();
        void api.dismiss();
      }
      // Ctrl+, opens Settings (v0.6 task 1), the same window the tray item opens.
      // Bound here rather than as a global shortcut: it is only meaningful while
      // the Palette has focus, and a second system-wide chord is a second thing
      // to collide with.
      if (e.key === "," && e.ctrlKey) {
        e.preventDefault();
        void api.openSettings();
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [menu, view]);

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
    // A Clip has nothing to launch either. Enter pastes it back where you were;
    // Ctrl+Enter only loads the clipboard, for pasting somewhere Takyon's
    // synthesised keystroke cannot reach.
    if (kind === "clip") return e.ctrlKey ? "copy_clip" : "paste";
    // A command opens its surface whatever the modifiers, for the same reason a
    // calculation copies: the other chords reach actions it does not have.
    if (kind === "command") return "open_command";
    if (e.ctrlKey && e.shiftKey) return "reveal";
    if (e.ctrlKey) return "run_as_admin";
    return "open";
  };

  /** The row Enter would act on, and its Kind. */
  const selectedEntry = entries.find((e) => e.id === selected);
  const selectedKind = selectedEntry?.kind;

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
  /*
    What `!e` says about the index, or null when there is nothing to say. Ready
    is silent: a working index needs no announcement, and a row that is always
    there is one nobody reads when it finally matters.
   */
  const fileIndexNote = !filesBang
    ? null
    : fileIndex?.state === "stale"
      ? "Some changes were missed — rescanning, results may be incomplete"
      : // Rust reserved the row via `indexing`, so falling back to the general
        // wording keeps a reserved row from rendering empty while the first
        // status read is still in flight.
        fileIndex?.state === "building" || indexing
        ? "Building the file index…"
        : null;

  /*
    The one row `!c` shows before Enter: which Agent would answer, and whether it
    can. A signed-out Agent gets the sentence and no row to press (ADR-0017).
   */
  const askKind = ask ? pickAgent(ask.order, agents) : null;
  const askAgent = askKind ? agents?.find((a) => a.kind === askKind) : undefined;
  // Amber and unpressable, in the only two states that earn it: nothing is
  // switched on, or the probe came back and the Agent it named cannot answer.
  // An unfinished probe is neither — `!c` asks anyway.
  const askBlocked = !ask
    ? null
    : askKind === null
      ? "No agent is switched on. Turn one on in Settings."
      : blockedReason(askAgent);
  const askLabel = askAgent?.label ?? (askKind ? AGENT_LABELS[askKind] : "");
  // Named when it is not the first preference, or the row silently answers as
  // someone else. The order is a preference, so falling through is normal.
  const skipped = ask && askKind && askKind !== ask.order[0] ? ask.order[0] : null;
  const askFallback = skipped
    ? ` · ${agents?.find((a) => a.kind === skipped)?.label ?? AGENT_LABELS[skipped]} unavailable`
    : "";
  const askNote = !ask
    ? null
    : (askBlocked ??
      (ask.query
        ? `Ask ${askLabel} — press Enter${askFallback}`
        : `${askLabel}${askAgent ? ` · ${agentSummary(askAgent).headline}` : ""}${askFallback}`));

  /*
    The one row `!s` shows before Enter: what will happen, and where the key
    comes from when there is none. Amber for the no-key state, which is not an
    error but does need an action.
   */
  const webNote = !web
    ? null
    : !web.hasKey
      ? `No ${web.provider} key. Add one in Settings → Web search.`
      : web.query
        ? `Search the web with ${web.provider} — press Enter`
        : `${web.provider} · your question leaves this machine`;

  const showList =
    entries.length > 0 ||
    webNote !== null ||
    fileIndexNote !== null ||
    askNote !== null ||
    (indexing && value.trim().length > 0);

  /*
    Replaces the root rather than overlaying it: Rust has already resized to
    `VIEW_HEIGHT`, so a list left mounted under it would be a second scrollable
    thing in a window sized for one.
   */
  if (view === "clipboard-history") {
    return (
      <div className="relative flex h-full w-full flex-col p-2">
        <ClipboardHistory onClose={closeView} />
      </div>
    );
  }

  if (view === "web" && searching && web) {
    return (
      <div className="relative flex h-full w-full flex-col p-2">
        <SearchView query={searching} provider={web.provider} onClose={closeView} />
      </div>
    );
  }

  if (view === "ask" && asking) {
    return (
      <div className="relative flex h-full w-full flex-col p-2">
        <AskView
          agent={asking.agent}
          question={asking.query}
          snapshot={agents?.find((a) => a.kind === asking.agent)}
          onClose={closeView}
        />
      </div>
    );
  }

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
          // Destroys the clip, and only a clip: the chord is not offered on any
          // other Kind, and sending it would surface an error toast for a
          // keystroke that should have done nothing.
          if (e.key === "Backspace" && e.ctrlKey) {
            if (selectedKind === "clip") {
              e.preventDefault();
              run(selected, "delete_clip");
            }
            return;
          }
          if (e.key === "Enter") {
            e.preventDefault();
            // `!c` has no Entry to activate: the answer streams into a surface
            // rather than being launched.
            if (ask) startAsk();
            else if (web) startSearch();
            else run(selected, actionForEvent(e, selectedKind));
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
            {/*
              `!c` sets the same flag to reserve its row, so it is excluded here
              alongside `!e` — "Indexing applications…" under a question would be
              a status row about the wrong thing entirely.
             */}
            {indexing && entries.length === 0 && !filesBang && !ask && (
              <div
                className="flex items-center px-2 text-[13px] text-fg/40"
                style={{ height: ROW_HEIGHT }}
              >
                Indexing applications…
              </div>
            )}
            {/*
              Above the results, not below: Stale means the list may be missing
              rows, and a caveat under the answer is one nobody reads (ADR-0007).
             */}
            {fileIndexNote && (
              <div
                className="flex items-center px-2 text-[13px] text-fg/40"
                style={{ height: ROW_HEIGHT }}
              >
                {fileIndexNote}
              </div>
            )}
            {/*
              One row, and never a pressable one when the Agent cannot answer:
              the sentence is the whole response in that case (ADR-0017).
             */}
            {/*
              Warm rather than neutral, in both its states: this is the row that
              says a keystroke will leave the machine (`docs/brand.md`).
             */}
            {webNote && (
              <div
                className={`flex items-center px-2 text-[13px] ${
                  web?.hasKey ? "text-amber-200/90" : "text-amber-300"
                }`}
                style={{ height: ROW_HEIGHT }}
                role={web?.hasKey ? undefined : "alert"}
                data-testid="web-note"
              >
                {webNote}
              </div>
            )}
            {askNote && (
              <div
                className={`flex items-center px-2 text-[13px] ${
                  askBlocked ? "text-amber-300" : "text-fg/70"
                }`}
                style={{ height: ROW_HEIGHT }}
                role={askBlocked ? "alert" : undefined}
              >
                {askNote}
              </div>
            )}
            {entries.map((entry) => (
              <Command.Item
                key={entry.id}
                value={entry.id}
                // Not a hardcoded "open": a calculation has nothing to open, and
                // Rust refuses that action, so a click would silently do nothing.
                onSelect={() =>
                  run(entry.id, PRIMARY_ACTION[entry.kind] ?? "open")
                }
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

        {/*
          Only with the list, matching Raycast: an empty Palette has no selected
          row to describe. `FOOTER_HEIGHT` is added to the window in the same
          branch, on both sides of the seam.
         */}
        {showList && (
          <Footer
            entry={selectedEntry}
            labels={labels}
            // `!c` has no Entry, so the footer names the Bang's own verb instead
            // of a menu that has nothing to open. `null` where there is nothing
            // for Enter to do either — an unanswerable Agent, or no question yet.
            hint={
              ask
                ? ask.query && !askBlocked
                  ? "Ask"
                  : null
                : web
                  ? web.query && web.hasKey
                    ? "Search"
                    : null
                  : undefined
            }
          />
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
