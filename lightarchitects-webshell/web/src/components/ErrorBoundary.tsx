/**
 * ErrorBoundary — React error boundary that catches render errors and shows
 * a styled fallback instead of unmounting the entire tree.
 *
 * React class component (required — functional components cannot be error
 * boundaries). Wraps any subtree; on error, replaces it with a dark-themed
 * error display showing the message and a collapsible stack trace.
 *
 * Two usage patterns:
 *
 *   1. Full-page (top-level):
 *      <ErrorBoundary>
 *        <App />
 *      </ErrorBoundary>
 *
 *   2. Panel-level with custom fallback:
 *      <ErrorBoundary fallback={(error, reset) => <p>Failed: {error.message}</p>}>
 *        <HelixScene />
 *      </ErrorBoundary>
 */
import { Component, type ErrorInfo, type ReactNode } from 'react';

interface ErrorBoundaryProps {
  children: ReactNode;
  /** Optional custom fallback render function. */
  fallback?: (error: Error, reset: () => void) => ReactNode;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
  showStack: boolean;
}

export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null, showStack: false };
  }

  static getDerivedStateFromError(error: Error): Partial<ErrorBoundaryState> {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    console.error('[ErrorBoundary] caught render error:', error, info.componentStack);
  }

  reset = (): void => {
    this.setState({ hasError: false, error: null, showStack: false });
  };

  toggleStack = (): void => {
    this.setState((prev) => ({ showStack: !prev.showStack }));
  };

  render(): ReactNode {
    if (!this.state.hasError || !this.state.error) {
      return this.props.children;
    }

    const { error } = this.state;

    // Custom fallback render prop takes priority.
    if (this.props.fallback) {
      return this.props.fallback(error, this.reset);
    }

    // Default: full-page error display with dark theme.
    return (
      <div style={{
        height: '100%',
        width: '100%',
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        justifyContent: 'center',
        background: '#0a0a0f',
        color: '#e2e8f0',
        fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace",
        padding: '2rem',
        boxSizing: 'border-box',
      }}>
        <div style={{
          maxWidth: '600px',
          width: '100%',
          textAlign: 'center',
        }}>
          <div style={{ fontSize: '2rem', marginBottom: '1rem' }}>
            Something went wrong
          </div>
          <div style={{
            fontSize: '0.875rem',
            color: '#94a3b8',
            marginBottom: '0.5rem',
            wordBreak: 'break-word',
          }}>
            {error.message}
          </div>
          <button
            onClick={this.toggleStack}
            style={{
              background: 'none',
              border: '1px solid #334155',
              color: '#94a3b8',
              padding: '0.25rem 0.75rem',
              borderRadius: '4px',
              cursor: 'pointer',
              fontSize: '0.75rem',
              fontFamily: 'inherit',
              marginBottom: '1rem',
            }}
          >
            {this.state.showStack ? 'Hide stack trace' : 'Show stack trace'}
          </button>
          {this.state.showStack && error.stack && (
            <pre style={{
              fontSize: '0.7rem',
              color: '#64748b',
              background: '#1e293b',
              padding: '0.75rem',
              borderRadius: '4px',
              overflow: 'auto',
              textAlign: 'left',
              maxHeight: '200px',
              maxWidth: '100%',
              margin: '0 0 1rem',
            }}>
              {error.stack}
            </pre>
          )}
          <div>
            <button
              onClick={this.reset}
              style={{
                background: '#1e293b',
                border: '1px solid #334155',
                color: '#e2e8f0',
                padding: '0.5rem 1.5rem',
                borderRadius: '4px',
                cursor: 'pointer',
                fontSize: '0.875rem',
                fontFamily: 'inherit',
              }}
            >
              Reload
            </button>
          </div>
        </div>
      </div>
    );
  }
}