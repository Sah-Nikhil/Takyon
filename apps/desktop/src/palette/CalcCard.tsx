/**
 * A calculation, drawn as a card rather than a row (v0.4.5 task 1).
 *
 * Still a `Command.Item` in the same list — see `Palette.tsx`. The card is a
 * *rendering* of the selected Entry, so arrow keys, Enter, `Ctrl+K` and the
 * Stability rule keep working and nothing in `sources/calc` knows about it.
 *
 * Its height is not `ROW_HEIGHT`, and the native window is sized from Rust
 * (TBC-0006), so `CALC_CARD_HEIGHT` and `CALC_CAPTION_HEIGHT` are mirrored in
 * `window.rs`. A wrong number here clips the list against a transparent window,
 * which the mocked visual layer cannot see.
 */

import { CALC_CARD_HEIGHT, CALC_CAPTION_HEIGHT, type Entry } from "@takyon/shared";

/** Operator characters, coloured apart from the operands as Raycast does. */
const OPERATORS = /([+\-*/^%()×÷−]+)/;

/**
 * Split an expression so operators can be tinted.
 *
 * Display only. The parser has its own tokenizer and this must never grow into a
 * second one — it exists to make `12+30%` readable at a glance, nothing more.
 */
function tint(expression: string) {
  return expression.split(OPERATORS).map((part, i) =>
    OPERATORS.test(part) ? (
      <span key={i} className="text-accent/80">
        {part}
      </span>
    ) : (
      <span key={i}>{part}</span>
    ),
  );
}

function Half({ value, label, tinted = false }: { value: string; label: string; tinted?: boolean }) {
  return (
    <div className="flex min-w-0 flex-col items-center justify-center gap-2 px-4">
      <div className="w-full truncate text-center text-[26px] font-semibold leading-none text-fg">
        {tinted ? tint(value) : value}
      </div>
      <div className="rounded bg-fg/10 px-1.5 py-0.5 text-[10px] leading-none text-fg/60">
        {label}
      </div>
    </div>
  );
}

export function CalcCard({ entry, selected }: { entry: Entry; selected: boolean }) {
  return (
    <div className="px-2">
      <div
        className="flex items-end text-[11px] leading-none text-fg/56"
        style={{ height: CALC_CAPTION_HEIGHT, paddingBottom: 6 }}
      >
        Calculator
      </div>

      {/*
        Selection lands on the card, not on the wrapper. The wrapper also holds
        the caption, and highlighting a section label reads as a bug rather than
        as a selection.
       */}
      <div
        className={`relative grid grid-cols-2 rounded-lg ring-1 transition-colors ${
          selected ? "bg-fg/[0.09] ring-fg/15" : "bg-fg/[0.05] ring-transparent"
        }`}
        style={{ height: CALC_CARD_HEIGHT - 8 }}
      >
        {/* The expression, so the answer can be checked without retyping it. */}
        <Half value={entry.subtitle ?? ""} label="Expression" tinted />
        <Half value={entry.title} label="Result" />

        {/*
          Two segments with a gap, not one rule behind an opaque arrow. Masking
          would mean hardcoding the card's composited colour, which is a hex
          nobody would think to update when a token moves.
         */}
        <div
          aria-hidden
          className="absolute left-1/2 top-5 w-px bg-fg/10"
          style={{ bottom: "calc(50% + 11px)" }}
        />
        <div
          aria-hidden
          className="absolute left-1/2 bottom-5 w-px bg-fg/10"
          style={{ top: "calc(50% + 11px)" }}
        />
        <div
          aria-hidden
          className="absolute left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 text-[17px] leading-none text-fg/76"
        >
          →
        </div>
      </div>
    </div>
  );
}
