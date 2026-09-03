/**
 * The last thing between a render error and a blank window.
 *
 * React unmounts the whole tree when a render throws, and a Tauri window with an
 * empty root is an opaque white rectangle with a title bar — indistinguishable
 * from a window that failed to load at all. That ambiguity cost a v0.6 debugging
 * session, so the failure now says what it was.
 *
 * Deliberately no styling beyond the inline rules: the boundary has to work when
 * the stylesheet is the thing that failed.
 */

import { Component, type ErrorInfo, type ReactNode } from "react";

interface State {
  error: Error | null;
  where: string;
}

export class ErrorBoundary extends Component<{ children: ReactNode }, State> {
  state: State = { error: null, where: "" };

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // Also to stderr, where the verify drivers already capture it.
    console.error("[takyon] render failed", error, info.componentStack);
    this.setState({ where: info.componentStack ?? "" });
  }

  render() {
    const { error, where } = this.state;
    if (!error) return this.props.children;

    return (
      <div
        role="alert"
        style={{
          padding: 24,
          font: "13px/1.5 Consolas, monospace",
          color: "#e6edf3",
          background: "#0b0e12",
          height: "100%",
          overflow: "auto",
          whiteSpace: "pre-wrap",
        }}
      >
        <strong>Takyon could not draw this window.</strong>
        {"\n\n"}
        {String(error.stack ?? error.message)}
        {"\n"}
        {where}
        {"\n"}
        {typeof window === "undefined" ? "" : window.location.href}
      </div>
    );
  }
}
