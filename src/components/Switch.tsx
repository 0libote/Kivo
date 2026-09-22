import { Switch as AstryxSwitch } from "@astryxdesign/core/Switch";

interface SwitchProps {
  readonly checked: boolean;
  readonly disabled?: boolean;
  readonly label: string;
  readonly onChange: (checked: boolean) => void;
}

/**
 * Compact settings switch. Astryx owns the control; Kivo keeps its own prop
 * shape (checked/onChange) because every call site renders the control inside
 * a settings row whose visible label lives in the row, not on the switch.
 */
export function Switch({ checked, disabled, label, onChange }: SwitchProps) {
  return (
    <AstryxSwitch
      isDisabled={disabled}
      isLabelHidden
      label={label}
      onChange={(next) => onChange(next)}
      size="sm"
      value={checked}
    />
  );
}
