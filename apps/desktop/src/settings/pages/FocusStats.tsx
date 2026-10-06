/**
 * Focus stats (v0.16), the "Focus / Stats" sub-view of Settings → Focus.
 * t3code's usage page: headline total, split by Focus category, a daily line
 * over a gradient, a totals strip, a breakdown table. Numbers from `stats.rs`.
 *
 * One series in the theme's accent, so no legend: the title names it.
 */

import { useEffect, useId, useMemo, useRef, useState } from "react";
import type { FocusCategory, FocusStats as Stats } from "@takyon/shared";
import * as api from "@/api";
import { Chips, Confirm } from "../controls";
import { FocusIcon } from "@/focus/icons";
import { keyLabel } from "@/focus/format";

/** A line needs two days at least; t3's ranges, less its hourly 24 h. */
const RANGES = [
  { value: "7", label: "7 days" },
  { value: "30", label: "30 days" },
  { value: "90", label: "90 days" },
] as const;
type Range = (typeof RANGES)[number]["value"];

/** "3 h 25 min", "45 min", "0 min". */
export function hours(ms: number): string {
  const m = Math.round(ms / 60_000);
  const h = Math.floor(m / 60);
  return h ? `${h} h ${m % 60} min` : `${m} min`;
}

const axisDate = (start: number) =>
  new Date(start).toLocaleDateString(undefined, { month: "short", day: "numeric" }).toUpperCase();

/** Width of an element, kept current: the SVG is drawn 1:1 so text stays 10px. */
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

/**
 * Monotone cubic (Fritsch-Carlson) through the points: smooth like t3's line,
 * and never dipping below a zero day between two points.
 */
function smoothPath(points: { x: number; y: number }[]): string {
  const n = points.length;
  if (n === 0) return "";
  if (n === 1) return `M${points[0]!.x},${points[0]!.y}`;
  const dx = (i: number) => points[i + 1]!.x - points[i]!.x;
  const slope = (i: number) => (points[i + 1]!.y - points[i]!.y) / dx(i);
  const m = points.map((_, i) => {
    if (i === 0) return slope(0);
    if (i === n - 1) return slope(n - 2);
    const a = slope(i - 1);
    const b = slope(i);
    return a * b <= 0 ? 0 : (3 * (dx(i - 1) + dx(i))) / ((2 * dx(i) + dx(i - 1)) / a + (dx(i) + 2 * dx(i - 1)) / b);
  });
  let d = `M${points[0]!.x},${points[0]!.y}`;
  for (let i = 0; i < n - 1; i++) {
    const p = points[i]!;
    const q = points[i + 1]!;
    const h = dx(i) / 3;
    d += ` C${p.x + h},${p.y + m[i]! * h} ${q.x - h},${q.y - m[i + 1]! * h} ${q.x},${q.y}`;
  }
  return d;
}

function DailyChart({ stats }: { stats: Stats }) {
  const [hover, setHover] = useState<number | null>(null);
  const [box, W] = useWidth();
  const gradient = useId();
  const H = 200;
  const pad = { l: 40, r: 8, t: 10, b: 24 };
  const max = Math.max(60 * 60_000, ...stats.days.map((d) => d.focusedMs));
  const top = Math.ceil(max / 3_600_000) * 3_600_000;
  const plotW = W - pad.l - pad.r;
  const plotH = H - pad.t - pad.b;
  const n = stats.days.length;
  const x = (i: number) => pad.l + (n <= 1 ? plotW / 2 : (i / (n - 1)) * plotW);
  const y = (ms: number) => pad.t + plotH - (ms / top) * plotH;
  const points = stats.days.map((d, i) => ({ x: x(i), y: y(d.focusedMs) }));
  const line = smoothPath(points);
  const base = pad.t + plotH;
  const area = n ? `${line} L${x(n - 1)},${base} L${x(0)},${base} Z` : "";
  const ticks = [0, top / 2, top];
  const labels = n > 2 ? [0, Math.floor((n - 1) / 2), n - 1] : [0, n - 1];
  const shown = hover === null ? null : stats.days[hover];

  return (
    <div ref={box} className="relative">
      <svg
        width={W}
        height={H}
        viewBox={`0 0 ${W} ${H}`}
        className="block"
        role="img"
        aria-label="Focused time per day"
        onMouseMove={(e) => {
          const left = e.currentTarget.getBoundingClientRect().left;
          const i = Math.round(((e.clientX - left - pad.l) / plotW) * (n - 1));
          setHover(Math.min(n - 1, Math.max(0, i)));
        }}
        onMouseLeave={() => setHover(null)}
      >
        <defs>
          <linearGradient id={gradient} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" style={{ stopColor: "var(--color-accent)", stopOpacity: 0.32 }} />
            <stop offset="100%" style={{ stopColor: "var(--color-accent)", stopOpacity: 0 }} />
          </linearGradient>
        </defs>
        {ticks.map((t) => (
          <g key={t}>
            <line x1={pad.l} x2={W - pad.r} y1={y(t)} y2={y(t)} className="stroke-fg/8" strokeWidth={1} />
            <text x={pad.l - 8} y={y(t) + 3} textAnchor="end" className="fill-fg/45 text-[10px]">
              {Math.round(t / 3_600_000)}h
            </text>
          </g>
        ))}
        <path d={area} fill={`url(#${gradient})`} />
        <path d={line} fill="none" className="stroke-accent" strokeWidth={2} strokeLinejoin="round" strokeLinecap="round" />
        {labels.map((i) => (
          <text
            key={i}
            x={x(i)}
            y={H - 6}
            textAnchor={i === 0 ? "start" : i === n - 1 ? "end" : "middle"}
            className="fill-fg/45 text-[10px] tracking-[0.04em]"
          >
            {stats.days[i] ? axisDate(stats.days[i].start) : ""}
          </text>
        ))}
        {hover !== null && shown && (
          <g>
            <line x1={x(hover)} x2={x(hover)} y1={pad.t} y2={base} className="stroke-fg/20" strokeWidth={1} />
            {/* 2px plate ring so the dot reads on top of the line. */}
            <circle cx={x(hover)} cy={y(shown.focusedMs)} r={5} className="fill-accent stroke-plate" strokeWidth={2} />
          </g>
        )}
      </svg>
      {shown && hover !== null && (
        <div
          className="pointer-events-none absolute top-0 rounded-md border border-hairline bg-card px-2.5 py-1.5 text-[12px] text-fg shadow-panel"
          style={{ left: Math.min(Math.max(x(hover), 80), W - 80), transform: "translateX(-50%)" }}
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

export function FocusStatsPanel() {
  const [range, setRange] = useState<Range>("30");
  const [stats, setStats] = useState<Stats | null>(null);
  const [categories, setCategories] = useState<FocusCategory[]>([]);
  const [table, setTable] = useState<"blocked" | "days">("blocked");
  const [clearing, setClearing] = useState(false);

  useEffect(() => {
    void api.focusStats(Number(range)).then(setStats);
  }, [range]);
  useEffect(() => {
    void api.focusCategories().then((c) => setCategories([...c.builtins, ...c.custom]));
  }, []);

  const find = useMemo(() => (id: string) => categories.find((c) => c.id === id), [categories]);

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
              <div className="font-mono text-[34px] font-medium tabular-nums leading-none text-fg">
                {hours(stats.focusedMs)}
              </div>
              <div className="mt-1.5 text-[13px] text-fg/60">focused · {stats.sessions} sessions</div>
            </div>
            <ul className="min-w-64 flex-1 space-y-2.5 pt-1">
              {stats.categories.slice(0, 5).map((c) => (
                <li key={c.id} className="flex items-center gap-2 text-[13px]">
                  <FocusIcon name={find(c.id)?.icon} className="size-4 text-accent" />
                  <span className="min-w-0 flex-1 truncate text-fg">{find(c.id)?.name ?? c.id}</span>
                  <span className="tabular-nums text-fg/60">{hours(c.focusedMs)}</span>
                </li>
              ))}
            </ul>
          </div>

          <div>
            <div className="mb-2 text-[13px] font-medium text-fg/80">Focused time per day</div>
            <DailyChart stats={stats} />
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
                          {new Date(d.start).toLocaleDateString(undefined, {
                            weekday: "short",
                            month: "short",
                            day: "numeric",
                          })}
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

      <div className="flex justify-end">
        <button
          type="button"
          onClick={() => setClearing(true)}
          className="rounded-control px-2.5 py-1 text-[12.5px] text-fg/60 transition-colors hover:bg-row-hover hover:text-fg"
        >
          Clear Focus history…
        </button>
      </div>
      {clearing && (
        <Confirm
          title="Clear Focus history?"
          consequence={`Every finished Focus session is deleted with what it blocked, ${stats.sessions} of them in this range. A running session stays.`}
          confirmLabel="Clear history"
          onConfirm={() => {
            setClearing(false);
            void api.focusClearHistory().then(() => api.focusStats(Number(range)).then(setStats));
          }}
          onCancel={() => setClearing(false)}
        />
      )}
    </div>
  );
}
