export function Spinner({ label = "Working" }: { readonly label?: string }) {
  return <output aria-label={label} className="spinner" />;
}
