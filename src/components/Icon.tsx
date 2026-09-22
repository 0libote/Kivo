import * as stylex from "@stylexjs/stylex";
import type { LucideIcon } from "lucide-react";
import {
  AlignLeft,
  ArrowLeft,
  AudioLines,
  Cable,
  Check,
  ChevronDown,
  ChevronUp,
  CircleAlert,
  Copy,
  House,
  Info,
  List,
  ListChecks,
  Mic,
  Pencil,
  RefreshCw,
  Repeat,
  Settings,
  Sparkles,
  X,
} from "lucide-react";
import type { SVGProps } from "react";

export type IconName =
  | "audio"
  | "arrow-left"
  | "home"
  | "info"
  | "connection"
  | "check"
  | "chevron-down"
  | "chevron-up"
  | "close"
  | "copy"
  | "error"
  | "key-points"
  | "microphone"
  | "pencil"
  | "proofread"
  | "refresh"
  | "rewrite"
  | "settings"
  | "spark"
  | "summarize";

interface IconProps extends SVGProps<SVGSVGElement> {
  readonly name: IconName;
  readonly size?: number;
}

/**
 * Kivo's product icon set, rendered from Lucide — the same icon library the
 * Astryx theme registry uses, so custom and semantic Astryx icons share one
 * stroke weight and geometry.
 */
const icons: Record<IconName, LucideIcon> = {
  audio: AudioLines,
  "arrow-left": ArrowLeft,
  home: House,
  info: Info,
  connection: Cable,
  check: Check,
  "chevron-down": ChevronDown,
  "chevron-up": ChevronUp,
  close: X,
  copy: Copy,
  error: CircleAlert,
  "key-points": List,
  microphone: Mic,
  pencil: Pencil,
  proofread: ListChecks,
  refresh: RefreshCw,
  rewrite: Repeat,
  settings: Settings,
  spark: Sparkles,
  summarize: AlignLeft,
};

const styles = stylex.create({
  icon: {
    display: "block",
    flexShrink: 0,
  },
});

export function Icon({ name, size = 18, ...props }: IconProps) {
  const Glyph = icons[name];
  return (
    <Glyph
      aria-hidden="true"
      size={size}
      strokeWidth={1.75}
      {...stylex.props(styles.icon)}
      {...props}
    />
  );
}
