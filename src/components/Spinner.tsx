import stylex from "@stylexjs/stylex";

const spin = stylex.keyframes({
  to: { transform: "rotate(360deg)" },
});

const styles = stylex.create({
  spinner: {
    borderColor: "color-mix(in srgb, currentColor 25%, transparent)",
    borderRadius: "50%",
    borderStyle: "solid",
    borderWidth: "1.8px",
    animationDuration: ".75s",
    animationIterationCount: "infinite",
    animationName: spin,
    animationTimingFunction: "linear",
    borderTopColor: "currentColor",
    height: 15,
    width: 15,
  },
});

export function Spinner({ label = "Working" }: { readonly label?: string }) {
  return (
    <output {...stylex.props(styles.spinner)} aria-label={label} role="status">
      <span className="sr-only">{label}</span>
    </output>
  );
}
