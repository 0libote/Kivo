import * as stylex from "@stylexjs/stylex";

const styles = stylex.create({
  track: {
    position: "relative",
    width: "38px",
    height: "22px",
    padding: 0,
    borderWidth: 0,
    borderRadius: "11px",
    backgroundColor: "color-mix(in srgb, var(--text-tertiary) 45%, transparent)",
    boxShadow: "inset 0 0 0 1px #1113170f",
    cursor: "pointer",
    transitionProperty: "background-color",
    transitionDuration: ".15s",
  },
  trackOn: {
    backgroundColor: "var(--accent)",
  },
  disabled: {
    opacity: ".45",
    cursor: "default",
  },
  thumb: {
    position: "absolute",
    top: "2px",
    left: "2px",
    width: "18px",
    height: "18px",
    borderRadius: "50%",
    backgroundColor: "#fff",
    boxShadow: "0 1px 4px #00000040",
    transitionProperty: "transform",
    transitionDuration: ".16s",
    transitionTimingFunction: "cubic-bezier(.2,.8,.2,1)",
  },
  thumbOn: {
    transform: "translateX(16px)",
  },
});

interface SwitchProps {
  readonly checked: boolean;
  readonly disabled?: boolean;
  readonly label: string;
  readonly onChange: (checked: boolean) => void;
}

export function Switch({ checked, disabled, label, onChange }: SwitchProps) {
  const track = stylex.props(styles.track, checked && styles.trackOn, disabled && styles.disabled);
  const thumb = stylex.props(styles.thumb, checked && styles.thumbOn);
  return (
    <button
      {...track}
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      role="switch"
      type="button"
    >
      <span {...thumb} />
    </button>
  );
}
