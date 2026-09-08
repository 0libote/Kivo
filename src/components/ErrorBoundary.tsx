import { Component, type ErrorInfo, type ReactNode } from "react";

interface Props { children: ReactNode }
interface State { failed: boolean }

export class ErrorBoundary extends Component<Props, State> {
  state: State = { failed: false };

  static getDerivedStateFromError(): State {
    return { failed: true };
  }

  componentDidCatch(_error: Error, _info: ErrorInfo) {
    // Intentionally avoid logging: rendered surfaces can contain private user text.
  }

  render() {
    if (this.state.failed) {
      return (
        <main className="fatal-surface">
          <div><strong>Kivo couldn’t open this window.</strong><span>Close it and try again.</span></div>
        </main>
      );
    }
    return this.props.children;
  }
}
