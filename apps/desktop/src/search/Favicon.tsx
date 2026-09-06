/**
 * A source's favicon, or its letter (ADR-0022).
 *
 * Bytes come from Rust over `takyon-favicon://`, never from the network: the
 * webview has no business reaching a host, and the CSP does not let it. A miss
 * is ordinary — plenty of sites serve nothing — so the tile is the normal state
 * rather than an error state.
 */

import { useState } from "react";

import { faviconUrl } from "@/api";

export function Favicon({
  host,
  size = 16,
  epoch = 0,
  className = "",
}: {
  host: string;
  size?: number;
  /**
   * Bumped when a search finishes caching icons.
   *
   * A row is drawn before its host's icon has been fetched, so the first ask
   * misses and `failed` sticks. This clears it and changes the URL, which is
   * what makes WebView2 ask again rather than reuse the miss.
   */
  epoch?: number;
  className?: string;
}) {
  // The URL that missed, not a flag: a new epoch is a new URL, so the miss
  // stops applying on its own and no effect has to reset anything.
  const [missed, setMissed] = useState("");
  const base = faviconUrl(host);
  const url = base && epoch > 0 ? `${base}?v=${epoch}` : base;

  if (!url || url === missed) {
    return (
      <span
        aria-hidden
        style={{ width: size, height: size, fontSize: size * 0.6 }}
        className={`grid shrink-0 place-items-center rounded-[0.25rem] bg-control font-semibold uppercase text-fg/60 ${className}`}
      >
        {host.replace(/^www\./, "").charAt(0)}
      </span>
    );
  }

  return (
    <img
      src={url}
      alt=""
      width={size}
      height={size}
      loading="lazy"
      onError={() => setMissed(url)}
      style={{ width: size, height: size }}
      className={`shrink-0 rounded-[0.25rem] object-contain ${className}`}
    />
  );
}
