/**
 * Advanced: the crash-log folder, and the sentence that matters next to it.
 *
 * ADR-0010 — logs are written locally and **nothing is ever sent**. The button
 * opens a folder. There is no upload path in Takyon for it to use.
 */

import { useCallback, useState } from "react";
import * as api from "@/api";
import { Group, Row } from "../controls";

export function Advanced() {
  const [error, setError] = useState<string | null>(null);

  const open = useCallback(async () => {
    setError(null);
    try {
      await api.openCrashLogs();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  return (
    <Group title="Diagnostics">
      <Row
        id="crash-logs"
        label="Crash logs"
        error={error}
        description="A panic in a release build is otherwise silent — no console, nothing on screen. Written here, and never sent anywhere."
      >
        <button
          type="button"
          onClick={() => void open()}
          className="rounded-control bg-control px-2.5 py-1 text-[12.5px] text-fg/80 transition-colors hover:text-fg"
        >
          Open folder
        </button>
      </Row>
    </Group>
  );
}
