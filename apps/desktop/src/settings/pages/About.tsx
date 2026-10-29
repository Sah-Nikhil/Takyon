/**
 * About: what this copy of Takyon is, whether its hotkey is live, and updates.
 *
 * The hotkey line is here rather than on Keyboard because Keyboard is where you
 * *change* it and this is where you find out it never registered — which is the
 * state someone is in when they come looking, having pressed Alt+Space and had
 * nothing happen.
 *
 * Mounting this page makes no request (ADR-0040). The feed is reached from the
 * button below, the Palette's update row, and the daily check the switch governs.
 * The licence list is a sub-view and reads its file only when opened.
 */

import { useCallback, useContext, useEffect, useState } from "react";
import { Answer } from "@/components/Answer";
import { Lockup } from "@/components/Lockup";
import * as api from "@/api";
import type { HotkeyStatus, UpdateCheck } from "@takyon/shared";
import { Group, Row, Switch, useApplied } from "../controls";
import { CrumbContext } from "../crumb";
import { Licences } from "./Licences";

/** ADR-0011: the slug is what Windows keys off, and it is not the display name. */
const IDENTITY = "com.v3sper.takyon";

/** Who publishes official builds. The installer and exe carry the same name. */
const PUBLISHER = "V3SPER";

type Busy = null | "checking" | { downloaded: number; total?: number } | "installing";

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

export function About() {
  const [hotkey, setHotkey] = useState<HotkeyStatus | null>(null);
  const [check, setCheck] = useState<UpdateCheck | null>(null);
  const [busy, setBusy] = useState<Busy>(null);
  const [error, setError] = useState<string | null>(null);
  const [autoCheck, setAutoCheck] = useState<boolean | null>(null);
  const [licences, setLicences] = useState(false);
  const setCrumb = useContext(CrumbContext);
  const daily = useApplied(api.setUpdateAutoCheck, api.updateAutoCheck);
  const toggleDaily = useCallback(
    (on: boolean) => void daily.apply(on, (next) => setAutoCheck(next)),
    [daily],
  );

  useEffect(() => {
    void api.hotkeyStatus().then(setHotkey);
    void api.updateAutoCheck().then(setAutoCheck);
  }, []);

  const runCheck = async () => {
    setBusy("checking");
    setError(null);
    try {
      setCheck(await api.updateCheck());
    } catch (e) {
      setCheck(null);
      setError(message(e));
    } finally {
      setBusy(null);
    }
  };

  // No `finally`: on Windows a successful install never returns, the app exits.
  const install = async () => {
    setError(null);
    setBusy({ downloaded: 0 });
    try {
      await api.updateInstall((p) =>
        setBusy(p.phase === "installing" ? "installing" : { downloaded: p.downloaded, total: p.total }),
      );
    } catch (e) {
      setError(message(e));
      setBusy(null);
    }
  };

  const available = check?.state === "available" ? check : null;

  const openLicences = () => {
    setLicences(true);
    setCrumb({ label: "Third-party licences", back: () => (setLicences(false), setCrumb(null)) });
  };

  if (licences) return <Licences />;

  return (
    <>
      <div className="mb-8 flex flex-col items-center gap-3 pt-4">
        <Lockup size={30} />
        <p className="text-[12.5px] text-fg/60">Version {__APP_VERSION__}</p>
        <p className="text-[12px] text-fg/50">Published by {PUBLISHER}</p>
      </div>

      <Group title="Updates">
        <Row id="updates" label="Software update" description={describe(check, busy)} error={error}>
          <button
            type="button"
            disabled={busy !== null}
            onClick={() => void (available ? install() : runCheck())}
            className="rounded-control bg-control px-2.5 py-1 text-[12.5px] text-fg/86 transition-colors hover:text-fg disabled:opacity-50 disabled:hover:text-fg/86"
          >
            {available ? "Install and restart" : "Check for updates"}
          </button>
        </Row>
        {available?.notes && (
          <div className="px-3.5 py-3">
            <p className="mb-1.5 text-[12.5px] font-medium text-fg/70">What's new in {available.version}</p>
            <Answer
              text={available.notes}
              className="max-h-40 overflow-y-auto text-[12.5px] leading-snug text-fg/70"
            />
          </div>
        )}
        <Row
          id="update-auto"
          label="Check for updates automatically"
          description="Once a day, in the background. The request carries no searches and no device ID; GitHub sees this computer's address. A new version shows in the Palette."
          applied={daily.applied}
          error={daily.error}
        >
          <Switch
            checked={autoCheck ?? true}
            disabled={autoCheck === null}
            label="Check for updates automatically"
            onChange={toggleDaily}
          />
        </Row>
      </Group>

      <Group title="Diagnostics">
        <Row
          id="hotkey-status"
          label="Global hotkey"
          description={
            hotkey && !hotkey.registered
              ? hotkey.error
              : "Press it anywhere in Windows to open the Palette."
          }
        >
          <span
            className={`font-mono text-[12.5px] ${
              hotkey && !hotkey.registered ? "text-warning" : "text-fg/80"
            }`}
          >
            {hotkey?.accelerator ?? "…"}
          </span>
        </Row>
        <Row
          id="identity"
          label="Package identity"
          description="What the startup entry, the single-instance lock and the data folder are named after. Independent of the display name (ADR-0011)."
        >
          <span className="font-mono text-[12.5px] text-fg/80">{IDENTITY}</span>
        </Row>
      </Group>

      <Group title="Legal">
        <Row
          id="licences"
          label="Third-party licences"
          description="Search dependencies listed in Takyon's bundled notices. Read their licence texts offline."
        >
          <button
            type="button"
            onClick={openLicences}
            className="rounded-control bg-control px-2.5 py-1 text-[12.5px] text-fg/86 transition-colors hover:text-fg"
          >
            View
          </button>
        </Row>
      </Group>
    </>
  );
}

/** The line under "Software update", for every state the row can be in. */
function describe(check: UpdateCheck | null, busy: Busy): string {
  if (busy === "checking") return "Checking…";
  if (busy === "installing") return "Installing. Takyon closes and opens again by itself.";
  if (busy) {
    const percent = busy.total ? ` ${Math.floor((busy.downloaded / busy.total) * 100)}%` : "";
    return `Downloading…${percent}`;
  }
  switch (check?.state) {
    case "unconfigured":
      return "This build can't update itself. Installers from the Takyon releases page can.";
    case "current":
      return `Takyon ${check.version} is the latest version.`;
    case "available":
      return `Takyon ${check.version} is available. The download is signature-checked before it installs.`;
    default:
      return "Look for a new version now.";
  }
}
