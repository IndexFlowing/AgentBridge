// web/src/pages/PlaceholderPage.tsx
import { Card, EmptyState } from '../components/ui';
import { theme } from '../theme';

export function PlaceholderPage({
  title,
  description,
}: {
  title: string;
  description: string;
}) {
  return (
    <div>
      <h1 style={{ fontSize: 22, margin: '0 0 4px' }}>{title}</h1>
      <p style={{ color: theme.muted, fontSize: 13, margin: '0 0 20px' }}>
        {description}
      </p>
      <Card>
        <EmptyState
          title="模块待开发"
          hint="该模块将在后续 P1 迭代中实现，当前导航仅作占位入口。"
        />
      </Card>
    </div>
  );
}
