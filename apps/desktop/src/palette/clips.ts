/** Pure helpers for the Clipboard History surface, testable without a DOM. */

import type { ClipRow } from "@takyon/shared";

/** "1.4 MB". Binary units, one decimal past KB, as Explorer shows them. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  if (bytes < 1024 ** 3) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  return `${(bytes / 1024 ** 3).toFixed(1)} GB`;
}

/**
 * Which row to select once `id` is deleted: the one below, else the one above.
 * Jumping back to the newest would lose the user's place mid-cleanup.
 */
export function neighbourAfterDelete(clips: ClipRow[], id: number): number | null {
  const at = clips.findIndex((c) => c.id === id);
  if (at < 0) return null;
  return clips[at + 1]?.id ?? clips[at - 1]?.id ?? null;
}
