import { Button } from "@astryxdesign/core/Button";
import { EmptyState } from "@astryxdesign/core/EmptyState";
import * as stylex from "@stylexjs/stylex";
import { Component, type ErrorInfo, type ReactNode } from "react";
import { reportErrorBoundary } from "./error-report";
import { Icon } from "./Icon";

interface Props {
  children: ReactNode;
}
interface State {
  failed: boolean;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { failed: false };

  static getDerivedStateFromError(): State {
    return { failed: true };
  }

  componentDidCatch(error: Error, _info: ErrorInfo) {
    // Count and report with a fixed message. Error details, props, children,
    // user text, and the ErrorInfo component stack are never logged: this
    // boundary wraps surfaces that render private user content.
    reportErrorBoundary(error);
  }

  render() {
    if (this.state.failed) {
      return (
        <main {...stylex.props(styles.fatal)}>
          <EmptyState
            actions={
              <Button
                label="Reload window"
                onClick={() => window.location.reload()}
                variant="primary"
              />
            }
            description="Close it and try again."
            icon={<Icon name="error" size={24} />}
            title="Kivo couldn’t open this window."
          />
        </main>
      );
    }
    return this.props.children;
  }
}

const styles = stylex.create({
  fatal: {
    display: "grid",
    placeItems: "center",
    width: "100%",
    height: "100%",
    backgroundColor: "var(--color-background-body)",
  },
});
