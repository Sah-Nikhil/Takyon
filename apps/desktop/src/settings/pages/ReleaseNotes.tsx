/**
 * "About / Release notes": what changed in recent versions, t3code-style.
 *
 * Reads the bundled `CHANGELOG.md`, loaded only when this view mounts, so it is
 * offline and costs the Settings window nothing until opened. Each version links
 * to its GitHub release, which carries the full commit list.
 */

import { useEffect, useState } from "react";
import { ArrowUpRight } from "@phosphor-icons/react";
import * as api from "@/api";
import { InlineText } from "@/components/Answer";
import {
  parseChangelog,
  RELEASES_URL,
  releaseUrl,
  SHOWN_RELEASES,
  type ReleaseItem,
  type ReleaseNote,
} from "../releaseNotes";

export function ReleaseNotes() {
  const [notes, setNotes] = useState<ReleaseNote[] | null>(null);

  useEffect(() => {
    void import("../../../../../CHANGELOG.md?raw").then((m) => setNotes(parseChangelog(m.default)));
  }, []);

  if (notes === null) return <p className="text-[12.5px] text-fg/56">Reading release notes…</p>;
  const older = Math.max(0, notes.length - SHOWN_RELEASES);

  return (
    <div className="space-y-4">
      <div className="divide-y divide-hairline overflow-hidden rounded-card border border-hairline bg-card">
        {notes.slice(0, SHOWN_RELEASES).map((note) => (
          <Release key={note.version} note={note} />
        ))}
      </div>
      {older > 0 && (
        <GitHubLink url={RELEASES_URL}>
          {`${older} older release${older === 1 ? "" : "s"} on GitHub`}
        </GitHubLink>
      )}
    </div>
  );
}

function Release({ note }: { note: ReleaseNote }) {
  const installed = note.version === __APP_VERSION__;
  return (
    <section className="px-4 py-3.5" aria-label={`Version ${note.version}`}>
      <div className="mb-2 flex items-baseline gap-2">
        <h3 className="text-[13px] font-medium text-fg">v{note.version}</h3>
        {note.title && <span className="min-w-0 truncate text-[12.5px] text-fg/56">{note.title}</span>}
        {installed && (
          <span className="ms-auto shrink-0 rounded bg-control px-1.5 py-px text-[10.5px] text-fg/70">
            Installed
          </span>
        )}
      </div>
      <ReleaseItems items={note.items} notes={note.notes} />
      <GitHubLink url={releaseUrl(note.version)}>View release on GitHub</GitHubLink>
    </section>
  );
}

/** Labelled bullets, then any paragraphs. Shared with About's update preview. */
export function ReleaseItems({ items, notes }: { items: ReleaseItem[]; notes: string[] }) {
  return (
    <>
      {items.length > 0 && (
        <ul className="list-disc space-y-1.5 ps-4 text-[12.5px] leading-snug text-fg/80">
          {items.map((item, i) => (
            <li key={i} className="break-words">
              {item.label && <strong className="font-semibold text-fg">{item.label}. </strong>}
              <InlineText text={item.text} />
            </li>
          ))}
        </ul>
      )}
      {notes.map((note, i) => (
        <p key={i} className="mt-2 text-[12px] leading-snug text-fg/60">
          <InlineText text={note} />
        </p>
      ))}
    </>
  );
}

function GitHubLink({ url, children }: { url: string; children: string }) {
  return (
    <button
      type="button"
      onClick={() => void api.openUrl(url)}
      className="mt-2 inline-flex items-center gap-1 rounded-sm text-[12px] text-fg/56 underline-offset-2 transition-colors hover:text-fg hover:underline focus-visible:text-fg focus-visible:outline-none"
    >
      {children}
      <ArrowUpRight aria-hidden weight="duotone" className="size-3 shrink-0" />
    </button>
  );
}
