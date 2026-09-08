import type { SVGProps } from "react";

export type IconName =
  | "audio"
  | "check"
  | "close"
  | "copy"
  | "error"
  | "key-points"
  | "microphone"
  | "pencil"
  | "proofread"
  | "rewrite"
  | "settings"
  | "spark"
  | "summarize";

interface IconProps extends SVGProps<SVGSVGElement> {
  name: IconName;
  size?: number;
}

const paths: Record<IconName, React.ReactNode> = {
  audio: <><path d="M4 9v6M8 6v12M12 3v18M16 7v10M20 10v4" /></>,
  check: <path d="m5 12 4 4L19 6" />,
  close: <path d="m6 6 12 12M18 6 6 18" />,
  copy: <><rect x="8" y="8" width="11" height="11" rx="2" /><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2" /></>,
  error: <><circle cx="12" cy="12" r="9" /><path d="M12 7v6M12 17h.01" /></>,
  "key-points": <><path d="M9 6h11M9 12h11M9 18h11" /><circle cx="4" cy="6" r="1" /><circle cx="4" cy="12" r="1" /><circle cx="4" cy="18" r="1" /></>,
  microphone: <><rect x="8" y="3" width="8" height="12" rx="4" /><path d="M5 11a7 7 0 0 0 14 0M12 18v3M8.5 21h7" /></>,
  pencil: <><path d="m4 20 4.5-1 10-10a2.1 2.1 0 0 0-3-3l-10 10L4 20Z" /><path d="m14 7 3 3" /></>,
  proofread: <><path d="M4 5h10M4 10h8M4 15h5" /><path d="m13 17 2.5 2.5L21 13" /></>,
  rewrite: <><path d="M17 3l4 4-4 4" /><path d="M3 11V9a2 2 0 0 1 2-2h16M7 21l-4-4 4-4" /><path d="M21 13v2a2 2 0 0 1-2 2H3" /></>,
  settings: <><circle cx="12" cy="12" r="3" /><path d="M19 13.5v-3l-2-.7-.7-1.6.9-1.9-2.1-2.1-1.9.9-1.7-.7L10.8 2h-3l-.7 2.4-1.6.7-2-.9-2 2.1.9 1.9-.7 1.6-2 .7v3l2 .7.7 1.6-.9 1.9 2.1 2.1 1.9-.9 1.6.7.7 2.3h3l.7-2.3 1.7-.7 1.9.9 2.1-2.1-.9-1.9.7-1.6 2-.7Z" transform="translate(2) scale(.84)" /></>,
  spark: <><path d="m12 2 1.2 4.1L17 8l-3.8 1.9L12 14l-1.2-4.1L7 8l3.8-1.9L12 2Z" /><path d="m18.5 14 .7 2.3 2.3.7-2.3.7-.7 2.3-.7-2.3-2.3-.7 2.3-.7.7-2.3Z" /></>,
  summarize: <><path d="M5 5h14M5 9h14M5 13h9M5 17h7" /></>,
};

export function Icon({ name, size = 18, ...props }: IconProps) {
  return (
    <svg
      aria-hidden="true"
      className="icon"
      fill="none"
      height={size}
      viewBox="0 0 24 24"
      width={size}
      {...props}
    >
      <g stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.75">
        {paths[name]}
      </g>
    </svg>
  );
}
