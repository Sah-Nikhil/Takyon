/**
 * Focus category icons: the name stored in `categories.json` to a Phosphor
 * glyph, always `duotone` (ADR-0022). Unknown names fall back to a neutral one.
 */

import {
  BookOpen,
  Briefcase,
  ChatsCircle,
  Code,
  GameController,
  Globe,
  Lightning,
  MusicNotes,
  Newspaper,
  PlayCircle,
  ShoppingBag,
  Star,
  Target,
  UsersThree,
  type Icon,
} from "@phosphor-icons/react";

export const FOCUS_ICONS: Readonly<Record<string, Icon>> = {
  UsersThree,
  ChatsCircle,
  PlayCircle,
  Newspaper,
  ShoppingBag,
  GameController,
  Star,
  Target,
  Globe,
  Code,
  BookOpen,
  Briefcase,
  MusicNotes,
  Lightning,
};

/** Icons a custom Focus category may pick, in picker order. */
export const PICKABLE_ICONS = Object.keys(FOCUS_ICONS);

export function FocusIcon({ name, className = "" }: { name?: string; className?: string }) {
  const Glyph = (name && FOCUS_ICONS[name]) || Target;
  return <Glyph weight="duotone" className={className} aria-hidden />;
}
