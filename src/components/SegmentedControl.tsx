import { useRef } from "react";
import * as stylex from "@stylexjs/stylex";

const styles = stylex.create({
  group: {
    display: "inline-flex",
    gap: "1px",
    padding: "3px",
    borderRadius: "9px",
    backgroundColor: "var(--surface-muted)",
  },
  item: {
    minHeight: "27px",
    paddingInline: "11px",
    borderWidth: 0,
    borderRadius: "6px",
    backgroundColor: "transparent",
    color: "var(--text-secondary)",
    fontSize: "12px",
    cursor: "pointer",
  },
  itemSelected: {
    color: "var(--text)",
    backgroundColor: "var(--surface-strong)",
    boxShadow: "0 1px 4px #0000001a",
  },
});

interface Segment<T extends string> {
  readonly label: string;
  readonly value: T;
}

interface SegmentedControlProps<T extends string> {
  readonly ariaLabel: string;
  readonly options: Segment<T>[];
  readonly value: T;
  readonly onChange: (value: T) => void;
}

export function SegmentedControl<T extends string>({ ariaLabel, options, value, onChange }: SegmentedControlProps<T>) {
  const group = useRef<HTMLDivElement>(null);
  const groupSx = stylex.props(styles.group);
  function move(current: number, delta: number) {
    const next = (current + delta + options.length) % options.length;
    onChange(options[next].value);
    group.current?.querySelectorAll<HTMLButtonElement>("[role=radio]")[next]?.focus();
  }
  return (
    <div {...groupSx} aria-label={ariaLabel} ref={group} role="radiogroup">
      {options.map((option, index) => {
        const selected = value === option.value;
        const sx = stylex.props(styles.item, selected && styles.itemSelected);
        return (
          <button
            {...sx}
            aria-checked={selected}
            key={option.value}
            onClick={() => onChange(option.value)}
            onKeyDown={(event) => {
              if (event.key === "ArrowRight" || event.key === "ArrowDown") {
                event.preventDefault();
                move(index, 1);
              } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
                event.preventDefault();
                move(index, -1);
              }
            }}
            role="radio"
            tabIndex={selected ? 0 : -1}
            type="button"
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
