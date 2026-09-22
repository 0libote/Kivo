import {
  SegmentedControl as AstryxSegmentedControl,
  SegmentedControlItem,
} from "@astryxdesign/core/SegmentedControl";

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

/**
 * Typed segmented control over Astryx's string-based SegmentedControl, so
 * call sites keep their string-literal unions without casts.
 */
export function SegmentedControl<T extends string>({
  ariaLabel,
  options,
  value,
  onChange,
}: SegmentedControlProps<T>) {
  return (
    <AstryxSegmentedControl
      label={ariaLabel}
      onChange={(next) => onChange(next as T)}
      size="sm"
      value={value}
    >
      {options.map((option) => (
        <SegmentedControlItem key={option.value} label={option.label} value={option.value} />
      ))}
    </AstryxSegmentedControl>
  );
}
