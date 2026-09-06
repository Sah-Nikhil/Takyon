/**
 * Appearance: the themes, the window's shape, its size, and motion.
 *
 * Its own page since v0.10. Through v0.9 these lived in a group on General,
 * which was right when the group was three controls; it is now the largest thing
 * in the window and the only one with a picker rather than a switch.
 *
 * The structure is Raycast's — Follow system appearance over a Dark theme and a
 * Light theme picker — and the picker itself is t3code's: a grid of cards, each
 * one a lit sphere of that family's own colours, because five near-black
 * rectangles are five identical rectangles.
 */

import { useCallback, useState } from "react";
import type { AppearanceMode, UiSize, WindowMode } from "@takyon/shared";
import {
  preferences,
  reduceMotion,
  refresh,
  setAppearance,
  setReduceMotion,
  setThemeFamily,
  setUiSize,
  setWindowMode,
  systemReducesMotion,
} from "@/prefs";
import { liveAppearance } from "@/theme/apply";
import { THEMES, family, type Appearance } from "@/theme/themes";
import { ThemeOrb } from "@/theme/ThemeOrb";
import { Chips, Group, Row, Switch, useApplied } from "../controls";

const SIZES: ReadonlyArray<{ value: UiSize; label: string }> = [
  { value: "small", label: "Small" },
  { value: "default", label: "Default" },
  { value: "large", label: "Large" },
];

/** The two explicit overrides. `system` is the switch above them, not a chip. */
const FIXED: ReadonlyArray<{ value: AppearanceMode; label: string }> = [
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
];

export function Appearance() {
  const [mode, setMode] = useState<AppearanceMode>(() => preferences().appearance);
  const [dark, setDark] = useState(() => preferences().themeDark);
  const [light, setLight] = useState(() => preferences().themeLight);
  const [windowMode, setMode2] = useState<WindowMode>(() => preferences().windowMode);
  const [size, setSize] = useState(() => preferences().uiSize);
  const [still, setStill] = useState(reduceMotion);
  // Read once. Windows can change it mid-session, but this line is copy rather
  // than behaviour — the media query in styles.css enforces the OS setting
  // whatever this switch says.
  const [osStill] = useState(systemReducesMotion);

  const modeApplied = useApplied(setAppearance, async () => (await refresh()).appearance);
  const windowApplied = useApplied(setWindowMode, async () => (await refresh()).windowMode);
  const sizeApplied = useApplied(setUiSize, async () => (await refresh()).uiSize);
  const motion = useApplied(setReduceMotion, async () => (await refresh()).reduceMotion);

  /*
    No `useApplied` here. It settles a control on storage after a write that
    might be refused, and says "Applied" because a flipped switch is otherwise
    the only feedback. Neither applies: Rust stores the id without interpreting
    it, and the confirmation is the whole window changing colour.
   */
  const chooseTheme = useCallback(async (appearance: Appearance, id: string) => {
    if (appearance === "dark") setDark(id);
    else setLight(id);
    await setThemeFamily(appearance, id);
  }, []);

  return (
    <>
      <Group>
        <Row
          id="appearance"
          label="Follow system appearance"
          applied={modeApplied.applied}
          error={modeApplied.error}
          description="Windows decides which of the two themes below is live. Turn this off to pin one."
        >
          <Switch
            label="Follow system appearance"
            checked={mode === "system"}
            onChange={(on) =>
              void modeApplied.apply(on ? "system" : "dark", (next) => setMode(next))
            }
          />
        </Row>
        {/*
          Only while the override is on. A disabled control here would be
          answering a question nobody asked: with the switch above set to follow,
          there is no third choice to make.
        */}
        {mode !== "system" && (
          <Row
            id="appearance-fixed"
            label="Use"
            description="Pinned, in both directions — this wins even when Windows disagrees."
          >
            <Chips
              label="Use"
              value={mode}
              options={FIXED}
              onChange={(next) => void modeApplied.apply(next, (v) => setMode(v))}
            />
          </Row>
        )}
      </Group>

      {/*
        `liveAppearance` rather than a test on `mode`: under `system` neither
        half is ruled out by the preference, so asking the preference marks both
        pickers as the live one. Only Windows knows, and that is what it answers.
      */}
      <ThemePicker
        id="theme-dark"
        title="Dark theme"
        appearance="dark"
        chosen={dark}
        live={liveAppearance(mode) === "dark"}
        onChoose={(id) => void chooseTheme("dark", id)}
      />

      <ThemePicker
        id="theme-light"
        title="Light theme"
        appearance="light"
        chosen={light}
        live={liveAppearance(mode) === "light"}
        onChoose={(id) => void chooseTheme("light", id)}
      />

      <Group title="Window">
        <Row
          id="window-mode"
          label="Window mode"
          applied={windowApplied.applied}
          error={windowApplied.error}
          description={
            windowMode === "compact"
              ? "The Palette is one line and grows a row at a time as results arrive."
              : "The Palette opens at a fixed height, groups results by kind, and suggests what you open most."
          }
        >
          <div role="radiogroup" aria-label="Window mode" className="flex gap-2">
            {(["compact", "expanded"] as const).map((value) => (
              <WindowModeCard
                key={value}
                value={value}
                chosen={value === windowMode}
                onChoose={() => void windowApplied.apply(value, setMode2)}
              />
            ))}
          </div>
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
            onChange={(next) => void sizeApplied.apply(next, setSize)}
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

/**
 * One half's picker: five spheres, and a line naming what the chosen one is for.
 *
 * The note sits under the grid rather than on each card. At the width five cards
 * need, a sentence per card is a sentence per card wrapped to four lines — and
 * the only note anyone wants is the one for what they just picked.
 */
function ThemePicker({
  id,
  title,
  appearance,
  chosen,
  live,
  onChoose,
}: {
  id: string;
  title: string;
  appearance: Appearance;
  chosen: string;
  /** Whether this half is the one currently painted. Said, never enforced. */
  live: boolean;
  onChoose: (id: string) => void;
}) {
  return (
    <section className="mb-6" id={`setting-${id}`}>
      <h2 className="mb-2 flex items-center gap-2 px-1 text-[13px] font-medium text-fg/60">
        {title}
        {/*
          Both pickers stay usable: choosing a dark theme in daylight has to
          work. This says which one you are looking at rather than disabling
          the other.
        */}
        {live && (
          <span className="rounded-full bg-control px-1.5 py-px text-[10.5px] text-fg/68">
            on screen now
          </span>
        )}
      </h2>
      <div className="overflow-hidden rounded-card border border-hairline bg-card p-3">
        <div
          role="radiogroup"
          aria-label={title}
          className="grid grid-cols-[repeat(auto-fill,minmax(96px,1fr))] gap-2"
        >
          {THEMES.map((theme) => {
            const selected = theme.id === chosen;
            return (
              <button
                key={theme.id}
                type="button"
                role="radio"
                aria-checked={selected}
                onClick={() => onChoose(theme.id)}
                className={`flex flex-col items-center gap-2 rounded-control border px-2 py-3 transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent/60 ${
                  selected
                    ? "border-accent/55 bg-row-selected"
                    : "border-transparent hover:bg-row-hover"
                }`}
              >
                <ThemeOrb family={theme} appearance={appearance} />
                <span
                  className={`text-[12px] leading-none ${selected ? "text-fg" : "text-fg/72"}`}
                >
                  {theme.label}
                </span>
              </button>
            );
          })}
        </div>
        <p className="mt-3 px-1 text-[12.5px] leading-snug text-fg/60">
          {family(chosen).note}
        </p>
      </div>
    </section>
  );
}

/**
 * Compact and Expanded, drawn rather than described.
 *
 * Wireframes in the live theme's own tokens rather than the reference's
 * photographic gradients: the difference between these modes *is* a shape, so a
 * picture of it is the honest control — and it previews the theme for free.
 */
function WindowModeCard({
  value,
  chosen,
  onChoose,
}: {
  value: WindowMode;
  chosen: boolean;
  onChoose: () => void;
}) {
  const compact = value === "compact";
  return (
    <button
      type="button"
      role="radio"
      aria-checked={chosen}
      aria-label={compact ? "Compact" : "Expanded"}
      onClick={onChoose}
      className={`flex w-[104px] shrink-0 flex-col items-center gap-2 rounded-control border p-2 transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-accent/60 ${
        chosen ? "border-accent/55 bg-row-selected" : "border-hairline hover:bg-row-hover"
      }`}
    >
      {/* The desktop the Palette floats over, so the panel below has something
          to be separated *from* — which is the whole job of `--color-edge`. */}
      <span
        aria-hidden
        className="flex h-[58px] w-full items-center justify-center rounded-[5px] bg-plate p-1.5"
      >
        <span
          className={`flex w-full flex-col gap-[3px] rounded-[3px] border border-edge bg-card px-1.5 ${
            compact ? "py-1.5" : "h-full py-1"
          }`}
        >
          {/* The input line, in both. */}
          <span className="block h-[3px] w-2/3 rounded-full bg-fg/30" />
          {!compact && (
            <>
              <span className="mt-[3px] block h-[2px] w-1/4 rounded-full bg-fg/15" />
              <span className="block h-[3px] w-full rounded-full bg-fg/20" />
              <span className="block h-[3px] w-5/6 rounded-full bg-fg/20" />
              <span className="mt-[2px] block h-[2px] w-1/3 rounded-full bg-fg/15" />
              <span className="block h-[3px] w-3/4 rounded-full bg-fg/20" />
            </>
          )}
        </span>
      </span>
      <span className={`text-[12px] leading-none ${chosen ? "text-fg" : "text-fg/72"}`}>
        {compact ? "Compact" : "Expanded"}
      </span>
    </button>
  );
}
