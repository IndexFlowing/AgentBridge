// web/src/pages/ExecutorsPage.tsx
import { useState } from 'react';
import type { FormEvent } from 'react';
import { CheckCircle2, XCircle } from 'lucide-react';
import type { Executor } from '../api';
import { executorsApi } from '../api';
import { useAsync } from '../hooks/useAsync';
import { StateView } from '../components/StateView';
import {
  Badge,
  Button,
  Card,
  InlineError,
  Input,
  Label,
} from '../components/ui';
import { theme } from '../theme';

export function ExecutorsPage() {
  const { data, loading, error, reload } = useAsync(() => executorsApi.list());
  const [name, setName] = useState('');
  const [command, setCommand] = useState('');
  const [saving, setSaving] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  const handleAdd = async (e: FormEvent) => {
    e.preventDefault();
    setSaving(true);
    setActionError(null);
    try {
      await executorsApi.save({
        name: name.trim(),
        kind: 'opencode',
        command: command.trim(),
        enabled: true,
      });
      setName('');
      setCommand('');
      reload();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  const runAction = async (action: () => Promise<unknown>) => {
    setActionError(null);
    try {
      await action();
      reload();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleDelete = (executor: Executor) => {
    if (!window.confirm(`确定删除执行器「${executor.name}」吗？`)) return;
    void runAction(() => executorsApi.remove(executor.id));
  };

  return (
    <div>
      <h1 style={{ fontSize: 22, margin: '0 0 4px' }}>执行器</h1>
      <p style={{ color: theme.muted, fontSize: 13, margin: '0 0 20px' }}>
        注册并检测本地任务执行器
      </p>

      <Card style={{ marginBottom: '1rem' }}>
        <h3 style={{ margin: '0 0 1rem', color: theme.accent, fontSize: 15 }}>
          + 注册本地执行器
        </h3>
        <form
          onSubmit={handleAdd}
          style={{ display: 'flex', gap: '1rem', alignItems: 'flex-end', flexWrap: 'wrap' }}
        >
          <div style={{ flex: '1 1 200px' }}>
            <Label>显示名称</Label>
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="例如: 本地 OpenCode"
              required
            />
          </div>
          <div style={{ flex: '2 1 320px' }}>
            <Label>终端命令或绝对路径</Label>
            <Input
              value={command}
              onChange={(e) => setCommand(e.target.value)}
              placeholder="例如: opencode"
              required
            />
          </div>
          <Button type="submit" disabled={saving}>
            {saving ? '注册中...' : '注册执行器'}
          </Button>
        </form>
        {actionError && <InlineError message={actionError} />}
      </Card>

      <StateView
        loading={loading}
        error={error}
        onRetry={reload}
        empty={!!data && data.length === 0}
        emptyTitle="暂无执行器"
        emptyHint="注册一个本地命令后即可开始执行任务。"
      >
        <div style={{ display: 'flex', flexDirection: 'column', gap: '1rem' }}>
          {data?.map((executor) => (
            <Card key={executor.id || executor.name}>
              <div
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  gap: 12,
                  alignItems: 'flex-start',
                  flexWrap: 'wrap',
                }}
              >
                <div style={{ minWidth: 0 }}>
                  <div style={{ fontSize: 16, fontWeight: 700 }}>
                    {executor.name}
                  </div>
                  <div
                    style={{
                      color: theme.muted,
                      fontFamily: 'monospace',
                      fontSize: 12,
                      marginTop: 4,
                      wordBreak: 'break-all',
                    }}
                  >
                    $ {executor.command}
                  </div>
                  <div
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: 6,
                      marginTop: 12,
                      fontSize: 13,
                      color: executor.available ? theme.success : theme.danger,
                    }}
                  >
                    {executor.available ? (
                      <CheckCircle2 size={16} />
                    ) : (
                      <XCircle size={16} />
                    )}
                    {executor.status || (executor.available ? '就绪' : '不可用')}
                    {executor.version ? ` (${executor.version})` : ''}
                  </div>
                </div>
                <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
                  {executor.proxy_id && (
                    <Badge tone="info">Proxy: {executor.proxy_id}</Badge>
                  )}
                  <Button
                    variant="secondary"
                    onClick={() =>
                      void runAction(() => executorsApi.test(executor.id))
                    }
                  >
                    连通性测试
                  </Button>
                  {!executor.detected && (
                    <Button
                      variant="danger"
                      onClick={() => handleDelete(executor)}
                    >
                      删除
                    </Button>
                  )}
                </div>
              </div>
            </Card>
          ))}
        </div>
      </StateView>
    </div>
  );
}
