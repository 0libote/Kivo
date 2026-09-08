interface SwitchProps {
  checked: boolean;
  disabled?: boolean;
  label: string;
  onChange: (checked: boolean) => void;
}

export function Switch({ checked, disabled, label, onChange }: SwitchProps) {
  return (
    <button
      aria-checked={checked}
      aria-label={label}
      className="switch"
      data-checked={checked}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      role="switch"
      type="button"
    >
      <span className="switch__thumb" />
    </button>
  );
}
