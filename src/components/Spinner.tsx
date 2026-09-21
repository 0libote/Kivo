import * as stylex from "@stylexjs/stylex";

const spin = stylex.keyframes({
  to: { transform: "rotate(360deg)" },
});

const styles = stylex.create({
  spinner: {
    width: "15px",
    height: "15px",
    borderWidth: "1.8px",
    borderStyle: "solid",
    borderColor: "color-mix(in srgb, currentColor 25%, transparent)",
    borderTopColor: "currentColor",
    borderRadius: "50%",
    animationName: spin,
    animationDuration: ".75s",
    animationTimingFunction: "linear",
    animationIterationCount: "infinite",
  },
});

export function Spinner({ label = "Working" }: { readonly label?: string }) {
  const sx = stylex.props(styles.spinner);
  return (
    <output aria-label={label} role="status" {...sx}>
      <span className="sr-only">{label}</span>
    </output>
  );
}
