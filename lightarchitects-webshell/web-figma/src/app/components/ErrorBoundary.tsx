import { Component, ReactNode, ErrorInfo } from 'react';

const s = {
  page: {
    width: '100vw',
    height: '100vh',
    display: 'flex',
    flexDirection: 'column' as const,
    alignItems: 'center',
    justifyContent: 'center',
    background: '#0a0a0f',
    color: '#e2e8f0',
    fontFamily: "'JetBrains Mono', 'Fira Code', monospace",
    padding: '2rem',
  },
  title: {
    fontSize: '2rem',
    marginBottom: '1rem',
    color: '#e2e8f0',
  },
  detail: {
    fontSize: '0.875rem',
    color: '#94a3b8',
    marginBottom: '1rem',
    maxWidth: 600,
    textAlign: 'center' as const,
    wordBreak: 'break-word' as const,
  },
  stackBtn: {
    background: 'none',
    border: '1px solid #334155',
    color: '#94a3b8',
    padding: '0.25rem 0.75rem',
    borderRadius: 4,
    fontSize: '0.75rem',
    cursor: 'pointer',
    fontFamily: 'inherit',
    marginBottom: '0.75rem',
  },
  stack: {
    fontSize: '0.7rem',
    color: '#64748b',
    background: '#1e293b',
    padding: '0.75rem',
    borderRadius: 4,
    maxHeight: 200,
    overflow: 'auto' as const,
    maxWidth: 700,
    width: '100%',
    marginBottom: '1.5rem',
    whiteSpace: 'pre-wrap' as const,
  },
  reloadBtn: {
    background: '#1e293b',
    border: '1px solid #334155',
    color: '#e2e8f0',
    padding: '0.5rem 1.5rem',
    borderRadius: 4,
    fontSize: '0.875rem',
    cursor: 'pointer',
    fontFamily: 'inherit',
  },
};

interface GlobalState {
  hasError: boolean;
  error: Error | null;
  stack: string | null;
  showStack: boolean;
}

export class GlobalErrorBoundary extends Component<{ children: ReactNode }, GlobalState> {
  state: GlobalState = { hasError: false, error: null, stack: null, showStack: false };

  static getDerivedStateFromError(error: Error): Partial<GlobalState> {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error('[GlobalErrorBoundary]', error, info.componentStack);
    this.setState({ stack: info.componentStack ?? error.stack ?? null });
  }

  render() {
    if (!this.state.hasError) return this.props.children;

    const { error, stack, showStack } = this.state;
    return (
      <div style={s.page}>
        <h1 style={s.title}>Something went wrong</h1>
        <p style={s.detail}>{error?.message}</p>

        {stack && (
          <button
            style={s.stackBtn}
            onClick={() => this.setState(prev => ({ showStack: !prev.showStack }))}
          >
            {showStack ? 'hide stack' : 'show stack'}
          </button>
        )}

        {showStack && stack && <pre style={s.stack}>{stack}</pre>}

        <button style={s.reloadBtn} onClick={() => window.location.reload()}>
          reload
        </button>
      </div>
    );
  }
}
