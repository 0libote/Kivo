/**
 * Privacy-preserving crash counter for the window ErrorBoundary. Rendered
 * surfaces can contain private user text, so failures are only counted and
 * reported by error name/message — never with props, children, ErrorInfo
 * stacks, or anything else from the render tree. Kept free of React/StyleX
 * so unit tests can import it without the StyleX compiler.
 */
let boundaryFailures = 0;

export function getErrorBoundaryFailures(): number {
  return boundaryFailures;
}

export function resetErrorBoundaryFailuresForTests(): void {
  boundaryFailures = 0;
}

export function reportErrorBoundary(error: Error): void {
  boundaryFailures += 1;
  // Name/message only: an Error carries no render tree, and callers must not
  // forward ErrorInfo (its component stack is skipped on purpose).
  console.error(`Kivo window failed (${error.name}: ${error.message})`);
}
