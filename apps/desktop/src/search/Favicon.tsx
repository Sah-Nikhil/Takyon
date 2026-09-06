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
  className = "",
}: {
  host: string;
  size?: number;
  className?: string;
}) {
  const [failed, setFailed] = useState(false);
  const url = faviconUrl(host);

  if (!url || failed) {
    return (
      <span
        aria-hidden
        style={{ width: size, height: size, fontSize: size * 0.6 }}
        className={`grid shrink-0 place-items-center rounded-[0.25rem] bg-control font-semibold uppercase text-fg/45 ${className}`}
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
      onError={() => setFailed(true)}
      style={{ width: size, height: size }}
      className={`shrink-0 rounded-[0.25rem] object-contain ${className}`}
    />
  );
}
