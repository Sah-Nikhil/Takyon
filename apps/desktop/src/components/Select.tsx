/**
 * The app's dropdown. One component, both windows.
 *
 * A native `<select>` draws its popup with the OS renderer: a light grey list
 * over a near-black window, in a typeface and metric nothing else here uses, and
 * unstylable by design. `color-scheme: dark` would darken it and is the wrong
 * fix in the Palette, whose window is `transparent: true` (see `styles.css`).
 *
 * So the list is ours: a button and a listbox drawn from the same tokens as
 * every other surface. Keyboard behaviour follows the native control — arrows,
 * Home, End, typeahead, Enter, Escape — because that is what the control it
 * replaces does, and a launcher's audience navigates by keyboard first.
 */

import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";

export interface SelectOption<T extends string> {
  value: T;
  label: string;
}

/** How much room a popup needs below the button before it flips above it. */
const FLIP_MARGIN = 8;

/** Gap between button and popup, both directions. */
const OFFSET = 6;

/** Tallest a popup gets before it scrolls. Matches the old `max-h-60`. */
const MAX_HEIGHT = 240;

/** Where a popup sits on screen, in viewport coordinates. */
interface Anchor {
  left: number;
  width: number;
  /** Distance from the viewport top, or from its bottom when flipped above. */
  top?: number;
  bottom?: number;
  maxHeight: number;
}

/**
 * Measure the button and decide which side the popup opens on.
 *
 * Viewport coordinates, because the popup is portalled to `<body>` and
 * positioned `fixed` — see the render for why it cannot stay in the card.
 */
function anchorFor(box: DOMRect, rows: number): Anchor {
  const wanted = Math.min(rows * 30 + 12, MAX_HEIGHT);
  const below = window.innerHeight - box.bottom - OFFSET - FLIP_MARGIN;
  const above = box.top - OFFSET - FLIP_MARGIN;
  const flip = below < wanted && above > below;
  return {
    left: box.left,
    width: box.width,
    top: flip ? undefined : box.bottom + OFFSET,
    bottom: flip ? window.innerHeight - box.top + OFFSET : undefined,
    maxHeight: Math.max(80, Math.min(wanted, flip ? above : below)),
  };
}

export function Select<T extends string>({
  value,
  options,
  label,
  disabled = false,
  placeholder,
  className = "",
  onChange,
}: {
  value: T | "";
  options: ReadonlyArray<SelectOption<T>>;
  label: string;
  disabled?: boolean;
  /** Shown when nothing is chosen. Also the first, empty-valued row. */
  placeholder?: string;
  className?: string;
  onChange: (value: T | "") => void;
}) {
  const rows = useMemo(
    () =>
      placeholder === undefined
        ? options
        : [{ value: "" as T | "", label: placeholder }, ...options],
    [options, placeholder],
  );

  const [open, setOpen] = useState(false);
  const [anchor, setAnchor] = useState<Anchor | null>(null);
  /** Which row the keyboard is on. Separate from `value`: moving is not choosing. */
  const [active, setActive] = useState(0);
  const root = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const typed = useRef({ text: "", at: 0 });
  const id = useId();

  const current = rows.find((row) => row.value === value);
  const shown = current?.label ?? placeholder ?? "";

  const close = useCallback(
    (focusButton = true) => {
      setOpen(false);
      if (focusButton) button.current?.focus();
    },
    [],
  );

  const openList = useCallback(() => {
    if (disabled) return;
    // Measure before painting: a popup that opens off the bottom of a 620px
    // window and corrects itself a frame later reads as a glitch.
    const box = button.current?.getBoundingClientRect();
    if (box) setAnchor(anchorFor(box, rows.length));
    setActive(Math.max(0, rows.findIndex((row) => row.value === value)));
    setOpen(true);
  }, [disabled, rows, value]);

  const choose = useCallback(
    (index: number) => {
      const row = rows[index];
      if (row) onChange(row.value);
      close();
    },
    [rows, onChange, close],
  );

  // Pointer down rather than click: a click that lands on another control would
  // otherwise act on it and leave this list open behind it.
  useEffect(() => {
    if (!open) return;
    // The list is portalled out of `root`, so it has to be asked separately, or
    // pointer-down on a row closes the popup before its click can choose one.
    const onDown = (e: PointerEvent) => {
      const node = e.target as Node;
      if (!root.current?.contains(node) && !list.current?.contains(node)) {
        setOpen(false);
      }
    };
    document.addEventListener("pointerdown", onDown, true);
    return () => document.removeEventListener("pointerdown", onDown, true);
  }, [open]);

  /*
    The popup is `fixed`, so it does not move with the page under it. Re-measure
    on any scroll or resize rather than closing: the settings pane scrolls under
    a dropdown whenever the keyboard reaches one near the fold.
   */
  useEffect(() => {
    if (!open) return;
    const track = () => {
      const box = button.current?.getBoundingClientRect();
      if (box) setAnchor(anchorFor(box, rows.length));
    };
    window.addEventListener("scroll", track, true);
    window.addEventListener("resize", track);
    return () => {
      window.removeEventListener("scroll", track, true);
      window.removeEventListener("resize", track);
    };
  }, [open, rows.length]);

  // `preventScroll`, always: focusing an element that overhangs the fold makes
  // the browser scroll it into view, which drags the section under it and reads
  // as the page jumping when a dropdown opens.
  useEffect(() => {
    if (open) list.current?.focus({ preventScroll: true });
  }, [open]);

  /*
    Keeps the active row on screen when the arrows walk past the fold, by
    scrolling the list itself. `scrollIntoView` would do it in one line and also
    scroll every scrollable ancestor, which moves the page behind the list.
   */
  useEffect(() => {
    if (!open) return;
    const box = list.current;
    const row = box?.querySelector<HTMLElement>(`[data-index="${active}"]`);
    if (!box || !row) return;
    const top = row.offsetTop;
    const bottom = top + row.offsetHeight;
    if (top < box.scrollTop) box.scrollTop = top;
    else if (bottom > box.scrollTop + box.clientHeight) {
      box.scrollTop = bottom - box.clientHeight;
    }
  }, [open, active]);

  const onListKeyDown = (e: React.KeyboardEvent) => {
    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        setActive((i) => Math.min(i + 1, rows.length - 1));
        return;
      case "ArrowUp":
        e.preventDefault();
        setActive((i) => Math.max(i - 1, 0));
        return;
      case "Home":
        e.preventDefault();
        setActive(0);
        return;
      case "End":
        e.preventDefault();
        setActive(rows.length - 1);
        return;
      case "Enter":
      case " ":
        e.preventDefault();
        choose(active);
        return;
      case "Escape":
      case "Tab":
        // Stopped, or Escape over a dropdown inside the Palette would dismiss
        // the window underneath it rather than the list.
        e.preventDefault();
        e.stopPropagation();
        close();
        return;
    }
    if (e.key.length === 1 && !e.ctrlKey && !e.altKey && !e.metaKey) {
      // Typeahead, the one native behaviour people notice only when it is gone.
      const now = Date.now();
      typed.current.text = now - typed.current.at > 800 ? e.key : typed.current.text + e.key;
      typed.current.at = now;
      const needle = typed.current.text.toLowerCase();
      const found = rows.findIndex((row) => row.label.toLowerCase().startsWith(needle));
      if (found >= 0) setActive(found);
    }
  };

  return (
    <div ref={root} className={`relative ${className}`}>
      <button
        ref={button}
        type="button"
        role="combobox"
        aria-label={label}
        aria-expanded={open}
        aria-controls={open ? `${id}-list` : undefined}
        aria-haspopup="listbox"
        disabled={disabled}
        onClick={() => (open ? close() : openList())}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown" || e.key === "ArrowUp" || e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            openList();
          }
        }}
        className={`flex h-8 w-full items-center gap-2 rounded-control border border-hairline bg-control px-2.5 text-[13px] transition-colors ${
          disabled
            ? "cursor-default text-fg/46"
            : "text-fg hover:border-fg/20 focus-visible:border-accent/60 focus-visible:outline-none"
        }`}
      >
        <span className={`min-w-0 flex-1 truncate text-start ${current ? "" : "text-fg/60"}`}>
          {shown}
        </span>
        <Chevron open={open} />
      </button>

      {/*
        Portalled to `<body>` and positioned `fixed`. Absolute inside the wrapper
        was clipped by the settings card, whose `overflow-hidden` rounds its rows
        — the hotkey list lost its last two options and nothing said so.
      */}
      {open && anchor && createPortal(
        <div
          ref={list}
          id={`${id}-list`}
          role="listbox"
          aria-label={label}
          aria-activedescendant={`${id}-row-${active}`}
          tabIndex={-1}
          onKeyDown={onListKeyDown}
          style={{
            left: anchor.left,
            width: anchor.width,
            top: anchor.top,
            bottom: anchor.bottom,
            maxHeight: anchor.maxHeight,
          }}
          className="fixed z-50 overflow-y-auto rounded-control border border-hairline bg-card p-1 shadow-panel outline-none"
        >
          {rows.map((row, i) => {
            const chosen = row.value === value;
            return (
              <div
                key={row.value || "__empty"}
                id={`${id}-row-${i}`}
                data-index={i}
                role="option"
                aria-selected={chosen}
                onPointerEnter={() => setActive(i)}
                onClick={() => choose(i)}
                className={`flex cursor-default items-center gap-2 rounded-[0.375rem] px-2 py-1 text-[13px] ${
                  i === active ? "bg-row-selected" : ""
                } ${chosen ? "text-fg" : "text-fg/80"} ${row.value === "" ? "text-fg/60" : ""}`}
              >
                <span className="min-w-0 flex-1 truncate">{row.label}</span>
                {/* The chosen row is marked, not merely highlighted: the
                    highlight belongs to the keyboard, and the two are different
                    facts that land on the same row often enough to confuse. */}
                {chosen && <Tick />}
              </div>
            );
          })}
        </div>,
        document.body,
      )}
    </div>
  );
}

function Chevron({ open }: { open: boolean }) {
  return (
    <svg
      aria-hidden
      viewBox="0 0 12 12"
      className={`size-3 shrink-0 text-fg/56 transition-transform ${open ? "rotate-180" : ""}`}
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M2.5 4.5 6 8l3.5-3.5" />
    </svg>
  );
}

function Tick() {
  return (
    <svg
      aria-hidden
      viewBox="0 0 12 12"
      className="size-3 shrink-0 text-accent"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.75"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M2.5 6.5 5 9l4.5-6" />
    </svg>
  );
}
