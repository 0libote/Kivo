export function Spinner({ label = "Working" }: { label?: string }) {
  return <span aria-label={label} className="spinner" role="status" />;
}
