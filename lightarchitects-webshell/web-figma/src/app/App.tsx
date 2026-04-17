import { GlobalErrorBoundary } from './components/ErrorBoundary';
import { AppLayout } from './components/AppLayout';
import { HelixProvider } from '../imports/HelixContext';

export default function App() {
  return (
    <GlobalErrorBoundary>
      <HelixProvider>
        <AppLayout />
      </HelixProvider>
    </GlobalErrorBoundary>
  );
}
