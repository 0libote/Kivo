import { useRef } from "react";

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
  function move(current: number, delta: number) {
    const next = (current + delta + options.length) % options.length;
    onChange(options[next].value);
    group.current?.querySelectorAll<HTMLButtonElement>("[role=radio]")[next]?.focus();
  }
  return (
    <div aria-label={ariaLabel} className="segmented" ref={group} role="radiogroup">
      {options.map((option, index) => (
        <button
          aria-checked={value === option.value}
          className="segmented__item"
          data-selected={value === option.value}
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
          tabIndex={value === option.value ? 0 : -1}
          type="button"
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}
