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
  return (
    <div aria-label={ariaLabel} className="segmented" role="radiogroup">
      {options.map((option) => (
        <button
          aria-checked={value === option.value}
          className="segmented__item"
          data-selected={value === option.value}
          key={option.value}
          onClick={() => onChange(option.value)}
          role="radio"
          type="button"
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}
