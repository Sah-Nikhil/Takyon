/**
 * Focus stats (v0.16), laid out like t3code's usage page: a headline total,
 * the split by Focus category, a daily chart, a totals strip, and a table that
 * switches between most-blocked and by-day. Numbers come from `stats.rs`.
 *
 * One series in one hue (the theme's accent), so no legend: the title names
 * it. Hand-built SVG, no chart library (plan § Surfaces).
 */

import { useEffect, useMemo, useRef, useState } from "react";
import type { FocusCategory, FocusStats as Stats } from "@takyon/shared";
import * as api from "@/api";
import { Chips } from "../controls";
import { FocusIcon } from "@/focus/icons";
import { keyLabel } from "@/focus/format";

const RANGES = [
  { value: "1", label: "Today" },
  { value: "7", label: "7 days" },
  { value: "30", label: "30 days" },
  { value: "90", label: "90 days" },
] as const;
type Range = (typeof RANGES)[number]["value"];

/** "3 h 25 min", "45 min", "0 min". */
function hours(ms: number): string {
  const m = Math.round(ms / 60_000);
  const h = Math.floor(m / 60);
  return h ? `${h} h ${m % 60} min` : `${m} min`;
}

const dayLabel = (start: number, days: number) =>
  new Date(start).toLocaleDateString(undefined, days > 7 ? { month: "short", day: "numeric" } : { weekday: "short" });

/** Width of an element, kept current: the SVG is drawn at 1:1 so text stays 10px. */
function useWidth() {
  const ref = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(600);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => entry && setWidth(Math.round(entry.contentRect.width)));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  return [ref, width] as const;
}

function DailyChart({ stats, days }: { stats: Stats; days: number }) {
  const [hover, setHover] = useState<number | null>(null);
  const [box, W] = useWidth();
  const H = 168;
  const pad = { l: 36, r: 8, t: 8, b: 22 };
  const max = Math.max(60 * 60_000, ...stats.days.map((d) => d.focusedMs));
  // Round the axis top up to whole hours so gridlines read as hours.
  const top = Math.ceil(max / 3_600_000) * 3_600_000;
  const plotW = W - pad.l - pad.r;
  const plotH = H - pad.t - pad.b;
  const step = plotW / Math.max(stats.days.length, 1);
  const bar = Math.max(2, Math.min(28, step - 2));
  const y = (ms: number) => pad.t + plotH - (ms / top) * plotH;
  const ticks = [0, top / 2, top];
  const labelEvery = Math.ceil(stats.days.length / 8);
  const shown = hover === null ? null : stats.days[hover];

  return (
    <div ref={box} className="relative">
      <svg viewBox={`0 0 ${W} ${H}`} width={W} height={H} className="block" role="img" aria-label="Focused time per day">
        {ticks.map((t) => (
          <g key={t}>
            <line x1={pad.l} x2={W - pad.r} y1={y(t)} y2={y(t)} className="stroke-fg/8" strokeWidth={1} />
            <text x={pad.l - 6} y={y(t) + 3} textAnchor="end" className="fill-fg/45 text-[10px]">
              {Math.round(t / 3_600_000)}h
            </text>
          </g>
        ))}
        {stats.days.map((d, i) => {
          const x = pad.l + i * step + (step - bar) / 2;
          const h = Math.max(0, plotH - (y(d.focusedMs) - pad.t));
          // 4px rounded data-end, flat on the baseline.
          const r = Math.min(4, bar / 2, h);
          return (
            <g key={d.start} onMouseEnter={() => setHover(i)} onMouseLeave={() => setHover(null)}>
              {/* Hit target taller and wider than the mark. */}
              <rect x={pad.l + i * step} y={pad.t} width={step} height={plotH} className="fill-transparent" />
              {h > 0 && (
                <path
                  d={`M${x},${pad.t + plotH} v${-(h - r)} q0,${-r} ${r},${-r} h${bar - 2 * r} q${r},0 ${r},${r} v${h - r} z`}
                  className={hover === i ? "fill-accent" : "fill-accent/70"}
                />
              )}
              {i % labelEvery === 0 && (
                <text x={pad.l + i * step + step / 2} y={H - 6} textAnchor="middle" className="fill-fg/45 text-[10px]">
                  {dayLabel(d.start, days)}
                </text>
              )}
            </g>
          );
        })}
      </svg>
      {shown && hover !== null && (
        <div
          className="pointer-events-none absolute top-0 rounded-md border border-hairline bg-card px-2.5 py-1.5 text-[12px] text-fg shadow-panel"
          style={{ left: `${((pad.l + hover * step + step / 2) / W) * 100}%`, transform: "translateX(-50%)" }}
        >
          <div className="text-fg/60">
            {new Date(shown.start).toLocaleDateString(undefined, { weekday: "long", month: "short", day: "numeric" })}
          </div>
          <div className="font-medium">{hours(shown.focusedMs)}</div>
          <div className="text-fg/60">
            {shown.sessions} sessions · {shown.blocks} blocked
          </div>
        </div>
      )}
    </div>
  );
}

export function FocusStats() {
  const [range, setRange] = useState<Range>("7");
  const [stats, setStats] = useState<Stats | null>(null);
  const [categories, setCategories] = useState<FocusCategory[]>([]);
  const [table, setTable] = useState<"blocked" | "days">("blocked");

  useEffect(() => {
    void api.focusStats(Number(range)).then(setStats);
  }, [range]);
  useEffect(() => {
    void api.focusCategories().then((c) => setCategories([...c.builtins, ...c.custom]));
  }, []);

  const name = useMemo(() => (id: string) => categories.find((c) => c.id === id), [categories]);

  if (!stats) return null;
  const empty = stats.sessions === 0;

  const totals: [string, number][] = [
    ["Sessions", stats.sessions],
    ["Full length", stats.fullLength],
    ["Ended early", stats.endedEarly],
    ["Strict", stats.strict],
    ["Blocked", stats.blocks],
    ["Snoozed", stats.snoozes],
  ];

  return (
    <div className="space-y-6">
      <div className="flex justify-end">
        <Chips label="Range" value={range} options={RANGES} onChange={setRange} />
      </div>

      {empty ? (
        <p className="rounded-card border border-hairline bg-card px-4 py-6 text-[13px] text-fg/60">
          No Focus sessions finished in this range. Start one with Start Focus Session in the Palette.
        </p>
      ) : (
        <>
          <div className="flex flex-wrap items-start gap-x-10 gap-y-4">
            <div className="min-w-56">
              <div className="font-mono text-[34px] font-medium tabular-nums leading-none text-fg">{hours(stats.focusedMs)}</div>
              <div className="mt-1.5 text-[13px] text-fg/60">focused · {stats.sessions} sessions</div>
            </div>
            <ul className="min-w-64 flex-1 space-y-2.5 pt-1">
                {stats.categories.slice(0, 5).map((c) => (
                  <li key={c.id} className="flex items-center gap-2 text-[13px]">
                    <FocusIcon name={name(c.id)?.icon} className="size-4 text-accent" />
                    <span className="min-w-0 flex-1 truncate text-fg">{name(c.id)?.name ?? c.id}</span>
                    <span className="tabular-nums text-fg/60">{hours(c.focusedMs)}</span>
                  </li>
                ))}
            </ul>
          </div>

          <div>
            <div className="mb-2 text-[13px] font-medium text-fg/80">Focused time per day</div>
            <DailyChart stats={stats} days={Number(range)} />
          </div>

          <dl className="grid grid-cols-3 gap-x-6 gap-y-4 border-y border-hairline py-4 sm:grid-cols-6">
            {totals.map(([label, value]) => (
              <div key={label}>
                <dt className="text-[12px] text-fg/55">{label}</dt>
                <dd className="mt-0.5 text-[16px] font-medium tabular-nums text-fg">{value}</dd>
              </div>
            ))}
          </dl>

          <div>
            <div className="mb-2 flex items-center justify-between">
              <h3 className="text-[13px] font-medium text-fg/80">Breakdown</h3>
              <Chips
                label="Breakdown"
                value={table}
                options={[
                  { value: "blocked", label: "Most blocked" },
                  { value: "days", label: "By day" },
                ]}
                onChange={setTable}
              />
            </div>
            <table className="w-full text-[13px]">
              <thead>
                <tr className="text-left text-[12px] text-fg/55">
                  <th className="py-2 font-normal">{table === "blocked" ? "App or website" : "Day"}</th>
                  <th className="py-2 text-right font-normal">{table === "blocked" ? "Times blocked" : "Focused"}</th>
                  {table === "days" && <th className="py-2 text-right font-normal">Sessions</th>}
                  {table === "days" && <th className="py-2 text-right font-normal">Blocked</th>}
                </tr>
              </thead>
              <tbody className="divide-y divide-hairline border-t border-hairline">
                {table === "blocked"
                  ? stats.blocked.map((b) => (
                      <tr key={b.key}>
                        <td className="py-2 text-fg">{keyLabel(b.key, categories)}</td>
                        <td className="py-2 text-right tabular-nums text-fg/80">{b.count}</td>
                      </tr>
                    ))
                  : [...stats.days].reverse().map((d) => (
                      <tr key={d.start}>
                        <td className="py-2 text-fg">
                          {new Date(d.start).toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric" })}
                        </td>
                        <td className="py-2 text-right tabular-nums text-fg/80">{hours(d.focusedMs)}</td>
                        <td className="py-2 text-right tabular-nums text-fg/80">{d.sessions}</td>
                        <td className="py-2 text-right tabular-nums text-fg/80">{d.blocks}</td>
                      </tr>
                    ))}
              </tbody>
            </table>
          </div>
        </>
      )}
    </div>
  );
}
