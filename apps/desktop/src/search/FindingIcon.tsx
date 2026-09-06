/**
 * The icon that names a finding's kind (ADR-0022).
 *
 * Phosphor at `duotone` weight, always. Never `fill`: duotone carries a solid
 * body behind its stroke, so it survives 15px on a near-black plate where a bare
 * 1.5px stroke antialiases to mud, without going to a flat silhouette.
 *
 * The vocabulary is closed and mirrored in three places: this map, `ICONS` in
 * `findings.ts`, and the list `synth.rs` puts in the prompt. A test asserts the
 * first two agree; the third is prose an Agent reads, so it can only drift into
 * asking for a token that falls back to neutral.
 */

import {
  BookOpen,
  CalendarBlank,
  Car,
  ChartBar,
  CheckCircle,
  CircleDashed,
  Clock,
  Code,
  Coins,
  Egg,
  Fire,
  ForkKnife,
  Globe,
  Heartbeat,
  type Icon,
  Info,
  ListBullets,
  MapPin,
  MusicNotes,
  Question,
  Quotes,
  Scales,
  ShieldCheck,
  Star,
  TrendDown,
  TrendUp,
  Trophy,
  User,
  UsersThree,
  Warning,
  Wrench,
  XCircle,
} from "@phosphor-icons/react";

/** Token to glyph. Keys must match `ICONS` in `findings.ts` exactly. */
export const FINDING_ICONS: Readonly<Record<string, Icon>> = {
  score: Trophy,
  money: Coins,
  time: Clock,
  date: CalendarBlank,
  place: MapPin,
  person: User,
  group: UsersThree,
  car: Car,
  chart: ChartBar,
  up: TrendUp,
  down: TrendDown,
  warning: Warning,
  check: CheckCircle,
  cross: XCircle,
  question: Question,
  quote: Quotes,
  list: ListBullets,
  star: Star,
  globe: Globe,
  book: BookOpen,
  tool: Wrench,
  fire: Fire,
  shield: ShieldCheck,
  disagree: Scales,
  unknown: CircleDashed,
  egg: Egg,
  food: ForkKnife,
  health: Heartbeat,
  code: Code,
  music: MusicNotes,
};

/**
 * One finding's icon, or the neutral one.
 *
 * Never absent: a gutter that is sometimes empty makes the column of findings
 * ragged, which is worse than a generic glyph.
 */
export function FindingIcon({ name, className = "" }: { name?: string; className?: string }) {
  const Glyph = (name && FINDING_ICONS[name]) || Info;
  return <Glyph weight="duotone" className={className} aria-hidden />;
}
