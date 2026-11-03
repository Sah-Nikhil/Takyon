/**
 * The update row on the Palette's empty line (ADR-0040).
 *
 * Compact draws it as a pill inside the input, Expanded as a row under the input
 * row, the shapes Raycast uses. Both read a notice Rust already holds in memory:
 * nothing here, or on the show that draws it, makes a request. Enter on
 * `available` installs; on `whatsNew` it opens the installed version's notes.
 */

import { ArrowCircleUp, Sparkle } from "@phosphor-icons/react";
import { ROW_HEIGHT, type UpdateNotice } from "@takyon/shared";

/** Install progress, or why it stopped. Null while nothing has been pressed. */
export type NoticeBusy =
  | null
  | { downloaded: number; total?: number }
  | "installing"
  | { error: string };

/** cmdk value for the row; no Entry id can collide with it. */
export const NOTICE_ID = "takyon:update-notice";

/** The words, for both shapes and the footer. */
export function noticeCopy(notice: UpdateNotice, busy: NoticeBusy) {
  const available = notice.kind === "available";
  const title = available ? "Update available" : "See what's new";
  const action = available ? "Install Update" : "Show Release Notes";
  let status: string | null = null;
  if (busy === "installing") status = "Installing. Takyon restarts by itself.";
  else if (busy && "error" in busy) status = busy.error;
  else if (busy) {
    const percent = busy.total ? ` ${Math.floor((busy.downloaded / busy.total) * 100)}%` : "";
    status = `Downloading…${percent}`;
  }
  return { title, version: `v${notice.version}`, action, status };
}

function NoticeIcon({ notice }: { notice: UpdateNotice }) {
  const Icon = notice.kind === "available" ? ArrowCircleUp : Sparkle;
  return (
    <span className="flex size-6 shrink-0 items-center justify-center rounded-md bg-accent/16 text-accent">
      <Icon size={15} weight="duotone" />
    </span>
  );
}

const failed = (busy: NoticeBusy) => busy !== null && typeof busy === "object" && "error" in busy;

/** Expanded: one row, shaped like an Entry. */
export function NoticeRow({ notice, busy }: { notice: UpdateNotice; busy: NoticeBusy }) {
  const copy = noticeCopy(notice, busy);
  return (
    <div className="flex items-center gap-3 px-2" style={{ height: ROW_HEIGHT }} data-testid="update-notice">
      <NoticeIcon notice={notice} />
      <div className="flex min-w-0 flex-1 items-baseline gap-1.5">
        <span className="truncate text-[14px] leading-tight text-fg">{copy.title}</span>
        <span className="shrink-0 text-[11px] leading-tight tabular-nums text-fg/56">{copy.version}</span>
      </div>
      <span
        className={`shrink-0 truncate text-[11px] leading-tight ${failed(busy) ? "text-warning" : "text-fg/46"}`}
        role={failed(busy) ? "alert" : undefined}
      >
        {copy.status ?? (notice.kind === "available" ? "Update" : "Changelog")}
      </span>
    </div>
  );
}

/** Compact: a pill at the input row's right edge. */
export function NoticePill({
  notice,
  busy,
  selected,
  onActivate,
}: {
  notice: UpdateNotice;
  busy: NoticeBusy;
  selected: boolean;
  onActivate: () => void;
}) {
  const copy = noticeCopy(notice, busy);
  return (
    <button
      type="button"
      tabIndex={-1}
      onMouseDown={(e) => e.preventDefault()}
      onClick={onActivate}
      data-testid="update-notice"
      data-selected={selected || undefined}
      className={`flex shrink-0 items-center gap-2.5 rounded-lg px-2.5 py-1.5 text-left transition-colors ${
        selected ? "bg-row-selected" : "bg-control hover:bg-row-selected"
      }`}
    >
      <NoticeIcon notice={notice} />
      <span className="flex flex-col">
        <span className="flex items-baseline gap-1.5">
          <span className="text-[12.5px] leading-tight text-fg">{copy.title}</span>
          <span className="text-[11px] leading-tight tabular-nums text-fg/56">{copy.version}</span>
        </span>
        <span
          className={`text-[11px] leading-tight ${failed(busy) ? "text-warning" : "text-fg/56"}`}
          role={failed(busy) ? "alert" : undefined}
        >
          {copy.status ?? `${copy.action} ↵`}
        </span>
      </span>
    </button>
  );
}
