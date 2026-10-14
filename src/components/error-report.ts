/**
 * Privacy-preserving crash counter for the window ErrorBoundary. Rendered
 * surfaces can contain private user text, so failures are only counted and
 * reported with a fixed message — never with error names/messages, props,
 * children, ErrorInfo stacks, or anything else from the render tree. Kept free of React/StyleX
 * so unit tests can import it without the StyleX compiler.
 */
let boundaryFailures = 0;

export function getErrorBoundaryFailures(): number {
  return boundaryFailures;
}

export function resetErrorBoundaryFailuresForTests(): void {
  boundaryFailures = 0;
}

export function reportErrorBoundary(_error: Error): void {
  boundaryFailures += 1;
  // Error names and messages can contain private input too.
  console.error("Kivo window failed.");
}
