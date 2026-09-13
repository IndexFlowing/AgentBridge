// web/src/components/StateView.tsx
import type { ReactNode } from 'react';
import { EmptyState, ErrorState, Spinner } from './ui';

export function StateView({
  loading,
  error,
  empty,
  emptyTitle,
  emptyHint,
  loadingLabel,
  onRetry,
  children,
}: {
  loading: boolean;
  error: string | null;
  empty?: boolean;
  emptyTitle?: string;
  emptyHint?: string;
  loadingLabel?: string;
  onRetry?: () => void;
  children: ReactNode;
}) {
  if (loading) return <Spinner label={loadingLabel} />;
  if (error) return <ErrorState message={error} onRetry={onRetry} />;
  if (empty) return <EmptyState title={emptyTitle} hint={emptyHint} />;
  return <>{children}</>;
}
