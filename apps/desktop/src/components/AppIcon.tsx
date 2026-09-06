/**
 * An application's icon, or its initial while there isn't one.
 *
 * Shared by the Palette's rows and the Settings alias list, because both draw the
 * same thing from the same `takyon-icon://` key (§6) and both have to survive the
 * icon arriving after the row has painted.
 */

import * as api from "@/api";

/**
 * What is drawn while an icon is missing.
 *
 * Three cases the row cannot tell apart: not extracted yet, the shell had none,
 * or no protocol handler. §6 requires it never block a row. The initial, not a
 * generic glyph, which at 24px makes every unresolved row identical.
 */
function Placeholder({ title }: { title: string }) {
  return (
    <div
      aria-hidden
      className="grid size-6 shrink-0 place-items-center rounded-[5px] bg-fg/10 text-[11px] font-medium text-fg/64"
    >
      {title.trim().charAt(0).toUpperCase() || "?"}
    </div>
  );
}

export function AppIcon({ icon, title }: { icon?: string; title: string }) {
  const src = api.iconUrl(icon);
  if (!src) return <Placeholder title={title} />;

  return (
    <img
      src={src}
      alt=""
      width={24}
      height={24}
      className="size-6 shrink-0"
      /*
        Fixed width and height, because the fetch resolves after this row has
        painted. Without them the text shifts sideways when an icon arrives
        late — for a list being arrowed through, the difference between
        "loading" and "flickering".
       */
      loading="eager"
      decoding="async"
    />
  );
}
