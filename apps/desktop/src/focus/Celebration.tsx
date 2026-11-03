/**
 * The end of a Focus session, celebrated: confetti over the whole monitor, or
 * a toast where the bar sat (`?style=toast`, chosen in `celebrate.rs` when
 * `focus.confetti` is off or motion is reduced). The window is click-through,
 * never focused, and destroyed by Rust; this page only plays and fades.
 */

import { useEffect, useRef, useState } from "react";
import { Confetti } from "@phosphor-icons/react";
import { CONFETTI_MS, launch, opacity, step, type Piece } from "./confetti";

/** Toast on screen, fade-in to the start of its fade-out. */
export const TOAST_MS = 3_400;
const TOAST_FADE_MS = 220;
const COLOURS = 6;

export function Celebration() {
  const params = new URLSearchParams(window.location.search);
  const goal = params.get("goal") ?? "";
  // Rust always sends it; 70 mirrors `celebrate.rs` `AMOUNT` for a bare URL.
  const amount = Number(params.get("amount") ?? 70);
  return params.get("style") === "toast" ? <Toast goal={goal} /> : <Cannons amount={amount} />;
}

/** Live colour tokens from `styles.css`; canvas cannot read `var()` itself. */
function colours(): string[] {
  const css = getComputedStyle(document.documentElement);
  return Array.from({ length: COLOURS }, (_, i) => css.getPropertyValue(`--confetti-${i + 1}`).trim());
}

function draw(ctx: CanvasRenderingContext2D, pieces: Piece[], fill: string[], alpha: number) {
  ctx.globalAlpha = alpha;
  for (const p of pieces) {
    ctx.save();
    ctx.translate(p.x, p.y);
    ctx.rotate(p.angle);
    // Squash on one axis as it flips: the strip turns edge-on and back.
    ctx.scale(1, Math.cos(p.flip));
    ctx.fillStyle = fill[p.colour] ?? "transparent";
    if (p.round) {
      ctx.beginPath();
      ctx.arc(0, 0, p.size / 2.4, 0, Math.PI * 2);
      ctx.fill();
    } else {
      ctx.fillRect(-p.size / 2, -p.size / 4, p.size, p.size / 2);
    }
    ctx.restore();
  }
}

function Cannons({ amount }: { amount: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const el = canvas.current;
    const ctx = el?.getContext("2d");
    if (!el || !ctx) return;
    const { innerWidth: w, innerHeight: h, devicePixelRatio: dpr = 1 } = window;
    el.width = Math.round(w * dpr);
    el.height = Math.round(h * dpr);
    ctx.scale(dpr, dpr);
    const fill = colours();
    const pieces = launch(w, h, COLOURS, Math.random, amount);
    let start: number | null = null;
    let last = 0;
    let frame = 0;
    const tick = (now: number) => {
      start ??= now;
      const elapsed = now - start;
      // Capped: a frame stalled by a cold webview must not teleport the paper.
      step(pieces, Math.min((now - (last || now)) / 1000, 1 / 30), h);
      last = now;
      ctx.clearRect(0, 0, w, h);
      if (elapsed >= CONFETTI_MS) return;
      draw(ctx, pieces, fill, opacity(elapsed));
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [amount]);

  return <canvas ref={canvas} data-testid="confetti" aria-hidden className="pointer-events-none fixed inset-0 size-full" />;
}

function Toast({ goal }: { goal: string }) {
  const [shown, setShown] = useState(false);

  useEffect(() => {
    // Mounted hidden, then shown on the next frame, so the fade-in runs.
    const enter = requestAnimationFrame(() => setShown(true));
    const leave = window.setTimeout(() => setShown(false), TOAST_MS);
    return () => {
      cancelAnimationFrame(enter);
      window.clearTimeout(leave);
    };
  }, []);

  return (
    <div className="grid h-full w-full place-items-center p-2">
      <div
        role="status"
        data-testid="focus-toast"
        style={{ opacity: shown ? 1 : 0, transition: `opacity ${TOAST_FADE_MS}ms ease-out` }}
        className="flex h-12 w-full items-center gap-3 rounded-xl border border-edge bg-plate px-3.5"
      >
        <Confetti weight="duotone" className="size-5 shrink-0 text-accent" />
        <div className="min-w-0 flex-1">
          <p className="text-[13px] font-medium text-fg">Focus session complete</p>
          {goal && <p className="truncate text-[12px] text-fg/60">{goal}</p>}
        </div>
      </div>
    </div>
  );
}
