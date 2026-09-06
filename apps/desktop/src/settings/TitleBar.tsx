/**
 * The Settings window's own title bar.
 *
 * Windows' native bar is a light-grey strip with square buttons, sitting on top
 * of a window whose whole design is near-black surfaces and hairline borders. It
 * is the one part of the app that never matched the rest, and it is the first
 * thing you see.
 *
 * So the window is undecorated and this draws it: the mark and the title on the
 * left, the three controls on the right, in the app's own tokens. Close only
 * closes this window — the Palette stays warm and the process stays alive
 * (ADR-0003).
 */

import { useCallback, useEffect, useState } from "react";
import * as api from "@/api";
import { Mark } from "@/components/Mark";

/** Height in logical pixels. Matches Windows' own so the reflex is unchanged. */
export const TITLEBAR_HEIGHT = 32;

export function TitleBar({ title }: { title: string }) {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    void api.windowIsMaximized().then(setMaximized);
    return api.onWindowResized(() => {
      void api.windowIsMaximized().then(setMaximized);
    });
  }, []);

  const toggle = useCallback(async () => {
    await api.windowToggleMaximize();
    setMaximized(await api.windowIsMaximized());
  }, []);

  return (
    <header
      // The drag region is the bar itself; the buttons opt out below, or a click
      // on Close would be swallowed as the start of a drag.
      data-tauri-drag-region
      style={{ height: TITLEBAR_HEIGHT }}
      className="flex shrink-0 select-none items-center justify-between border-b border-hairline bg-sidebar pl-3 pr-0"
    >
      <span data-tauri-drag-region className="flex items-center gap-2">
        <Mark size={13} />
        <span className="text-[12px] text-fg/68">{title}</span>
      </span>

      <span className="flex h-full items-stretch">
        <Control label="Minimize" onClick={() => void api.windowMinimize()}>
          {/* A 10px rule, which is what Windows draws. */}
          <rect x="3" y="7.5" width="10" height="1" />
        </Control>
        <Control label={maximized ? "Restore" : "Maximize"} onClick={() => void toggle()}>
          {maximized ? (
            <>
              <rect x="3" y="5" width="7" height="7" fill="none" strokeWidth="1" stroke="currentColor" />
              <path d="M5.5 5V3.5h7.5V11h-1.5" fill="none" strokeWidth="1" stroke="currentColor" />
            </>
          ) : (
            <rect x="3.5" y="3.5" width="9" height="9" fill="none" strokeWidth="1" stroke="currentColor" />
          )}
        </Control>
        {/* Red on hover, as every Windows window does. Breaking that convention
            would make the one destructive control the least obvious one. */}
        <Control label="Close" danger onClick={() => void api.windowClose()}>
          <path
            d="M3.5 3.5l9 9M12.5 3.5l-9 9"
            fill="none"
            strokeWidth="1"
            stroke="currentColor"
          />
        </Control>
      </span>
    </header>
  );
}

function Control({
  label,
  danger,
  onClick,
  children,
}: {
  label: string;
  danger?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onClick}
      /*
        `#c42b1c` is Windows' own close-button red and the one literal colour
        left here on purpose: it belongs to the platform, and a close button that
        changed hue per theme would stop reading as one. White over it because
        the red is dark in every appearance.
       */
      className={`flex w-[46px] items-center justify-center text-fg/72 transition-colors ${
        danger ? "hover:bg-[#c42b1c] hover:text-white" : "hover:bg-row-hover hover:text-fg"
      }`}
    >
      <svg width="16" height="16" viewBox="0 0 16 16" fill="currentColor" aria-hidden>
        {children}
      </svg>
    </button>
  );
}
