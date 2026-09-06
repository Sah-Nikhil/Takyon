/**
 * Web search: the key `!s` needs, and nothing else (v0.9 task 2).
 *
 * The key never comes back from Rust — it is DPAPI-wrapped on disk and a hint,
 * its last four characters, is all this page can show. That is deliberate: a
 * bearer token for someone else's paid account should not sit in a webview's
 * memory to fill a text box with.
 */

import { useCallback, useEffect, useState } from "react";
import type { WebSettings } from "@takyon/shared";

import * as api from "@/api";
import { Group, Row } from "../controls";

export function WebSearch() {
  const [settings, setSettings] = useState<WebSettings | null>(null);
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  // `then` rather than `await` in the effect body, the shape `Agents.tsx` uses:
  // a synchronous `setState` inside an effect is a cascading render.
  const load = useCallback(() => {
    void api.webSettings().then(setSettings);
  }, []);

  useEffect(load, [load]);

  const save = async (value: string) => {
    setError(null);
    try {
      await api.setWebKey(value);
      setDraft("");
      setSaved(value.trim() !== "");
      load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const provider = settings?.provider ?? "Exa";
  const keyless = settings?.keylessProvider ?? "DuckDuckGo";

  return (
    <div className="space-y-6">
      <Group title="Key">
        <Row
          id="exa-key"
          label={`${provider} key`}
          description={
            <>
              {settings?.hasKey
                ? `A key is stored (${settings.hint}). It is wrapped for this Windows account and never leaves it, except to ${provider}.`
                : `Optional. Without one, !s searches with ${keyless}, which needs no key and no account.`}
            </>
          }
          error={error}
          applied={saved}
        >
          <form
            className="flex items-center gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              void save(draft);
            }}
          >
            <input
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              // A password field, because this is one in every way that matters
              // and a screen share is the ordinary case for a launcher.
              type="password"
              autoComplete="off"
              spellCheck={false}
              placeholder={settings?.hasKey ? "Replace the stored key" : "Paste your key"}
              aria-label={`${provider} key`}
              className="w-64 rounded-control bg-control px-2.5 py-1 text-[12.5px] text-fg outline-none placeholder:text-fg/46"
            />
            <button
              type="submit"
              disabled={!draft.trim()}
              className="rounded-control px-2 py-1 text-[12.5px] text-fg/72 transition-colors hover:bg-row-hover hover:text-fg disabled:opacity-30"
            >
              Save
            </button>
          </form>
        </Row>

        {settings?.hasKey && (
          <Row
            id="brave-key-clear"
            label="Remove the key"
            description="Deletes it from this machine. !s stops searching until another is added."
          >
            <button
              type="button"
              onClick={() => void save("")}
              className="rounded-control px-2 py-1 text-[12.5px] text-fg/72 transition-colors hover:bg-row-hover hover:text-fg"
            >
              Remove
            </button>
          </Row>
        )}

        <Row
          id="exa-signup"
          label="Where to get one"
          description={`${provider}'s own console issues the key. Takyon never sees the account it belongs to.`}
        >
          <button
            type="button"
            onClick={() => void api.openUrl(settings?.signupUrl ?? "https://dashboard.exa.ai/api-keys")}
            className="rounded-control px-2 py-1 text-[12.5px] text-fg/72 transition-colors hover:bg-row-hover hover:text-fg"
          >
            Open
          </button>
        </Row>
      </Group>

      <Group title="Fallback">
        <Row
          id="web-fallback"
          label={`${keyless} answers when ${provider} cannot`}
          description={`${keyless} needs no key, so !s always works. When a key is stored ${provider} is asked first, and anything that stops it — a wrong key, a spent quota, an outage — falls through to ${keyless} rather than failing. The answer header names whichever one replied.`}
        >
          <span className="text-[12.5px] text-fg/56">ADR-0021</span>
        </Row>
      </Group>

      <Group title="What leaves the machine">
        <Row
          id="web-outbound"
          label="Only the question, and only on Enter"
          description={`Typing !s sends nothing. On Enter the question goes to ${provider} when a key is stored and ${keyless} otherwise, the pages it names are read, and their text goes to whichever Agent is first in Settings → Agents. A line without a Bang never leaves this machine.`}
        >
          <span className="text-[12.5px] text-fg/56">ADR-0002</span>
        </Row>
      </Group>
    </div>
  );
}
