import { Component, type ErrorInfo, type ReactNode } from "react";
import { useNavigate } from "react-router-dom";

type Props = { children: ReactNode };

type State = {
  error: Error | null;
  resetKey: number;
};

function ErrorFallback({
  error,
  onRetry,
  onReload,
}: {
  error: Error;
  onRetry: () => void;
  onReload: () => void;
}) {
  const navigate = useNavigate();

  const goWorkspace = (): void => {
    onRetry();
    navigate("/", { replace: true });
  };

  return (
    <div
      role="alert"
      style={{
        minHeight: "100vh",
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        padding: "2rem",
        background: "var(--bg)",
        color: "var(--text)",
      }}
    >
      <div
        style={{
          maxWidth: 520,
          width: "100%",
          padding: "1.5rem",
          borderRadius: 8,
          border: "1px solid var(--border)",
          background: "var(--surface)",
        }}
      >
        <h1
          style={{
            margin: "0 0 0.5rem",
            fontSize: "1.25rem",
            color: "var(--error)",
          }}
        >
          Something went wrong
        </h1>
        <p
          style={{
            margin: "0 0 1rem",
            color: "var(--text-muted)",
            fontSize: "0.95rem",
          }}
        >
          The UI hit an unexpected error. You can try again, reload the page, or
          return to the workspace.
        </p>
        <p
          style={{
            margin: "0 0 1rem",
            padding: "0.75rem",
            borderRadius: 4,
            background: "var(--bg)",
            border: "1px solid var(--border)",
            fontSize: "0.85rem",
            wordBreak: "break-word",
          }}
        >
          {error.message}
        </p>
        {import.meta.env.DEV && error.stack ? (
          <details
            style={{
              marginBottom: "1rem",
              fontSize: "0.75rem",
              color: "var(--text-muted)",
            }}
          >
            <summary style={{ cursor: "pointer", marginBottom: 8 }}>
              Stack trace
            </summary>
            <pre
              style={{
                margin: 0,
                overflow: "auto",
                maxHeight: 200,
                whiteSpace: "pre-wrap",
              }}
            >
              {error.stack}
            </pre>
          </details>
        ) : null}
        <div style={{ display: "flex", flexWrap: "wrap", gap: 8 }}>
          <button
            type="button"
            onClick={onRetry}
            style={{
              padding: "0.5rem 1rem",
              borderRadius: 4,
              border: "1px solid var(--accent)",
              background: "var(--accent)",
              color: "var(--bg)",
              fontWeight: 600,
            }}
          >
            Try again
          </button>
          <button
            type="button"
            onClick={onReload}
            style={{
              padding: "0.5rem 1rem",
              borderRadius: 4,
              border: "1px solid var(--border)",
              background: "var(--bg)",
              color: "var(--text)",
            }}
          >
            Reload page
          </button>
          <button
            type="button"
            onClick={goWorkspace}
            style={{
              padding: "0.5rem 1rem",
              borderRadius: 4,
              border: "1px solid var(--border)",
              background: "transparent",
              color: "var(--accent)",
            }}
          >
            Workspace
          </button>
        </div>
      </div>
    </div>
  );
}

export default class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null, resetKey: 0 };

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error("Kobayashi UI error:", error, info.componentStack);
  }

  private handleRetry = (): void => {
    this.setState((prev) => ({
      error: null,
      resetKey: prev.resetKey + 1,
    }));
  };

  private handleReload = (): void => {
    window.location.reload();
  };

  render(): ReactNode {
    const { error } = this.state;
    if (error) {
      return (
        <ErrorFallback
          error={error}
          onRetry={this.handleRetry}
          onReload={this.handleReload}
        />
      );
    }

    return <div key={this.state.resetKey}>{this.props.children}</div>;
  }
}
