/**
 * Calculator: when the calculator is allowed to answer a Bangless query.
 *
 * A tier-two page, and the one that proves tier two is real rather than a
 * structure waiting for v0.7 — v0.4 shipped the feature, so the control here
 * changes something today.
 */

import { useState } from "react";
import type { CalcPolicy } from "@takyon/shared";
import { calcPolicy, refresh, setCalcPolicy } from "@/prefs";
import { Chips, Group, Row, useApplied } from "../controls";

const MODES: ReadonlyArray<{ value: CalcPolicy; label: string }> = [
  { value: "automatic", label: "As I type" },
  { value: "explicit", label: "After =" },
];

export function Calculator() {
  const [mode, setMode] = useState(calcPolicy);
  // A genuine re-read, not the in-process cache, so a failed write settles the
  // chips on what Rust stored rather than on what was clicked.
  const applied = useApplied(setCalcPolicy, async () => (await refresh()).calcPolicy);

  return (
    <Group>
      <Row
        id="calc-policy"
        label="Answer arithmetic"
        applied={applied.applied}
        error={applied.error}
        description={
          mode === "explicit"
            ? "Type = first to calculate, as in =12*1.18. Nothing else is ever read as arithmetic, so a search can never lose its top row to a number."
            : "12*1.18 answers as you type. The cost is that a plain number does too, so 2022 shows a result above Adobe Photoshop 2022 and Enter copies it."
        }
      >
        <Chips
          label="Answer arithmetic"
          value={mode}
          options={MODES}
          onChange={(next) => void applied.apply(next, setMode)}
        />
      </Row>
    </Group>
  );
}
