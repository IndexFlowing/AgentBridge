// web/src/pages/ExecutorsPage.tsx
import { useState } from 'react';
import type { FormEvent } from 'react';
import { CheckCircle2, XCircle } from 'lucide-react';
import type { Executor } from '../api';
import { executorsApi, proxyApi } from '../api';
import { useAsync } from '../hooks/useAsync';
import { StateView } from '../components/StateView';
import {
  Badge,
  Button,
  Card,
  InlineError,
  Input,
  Label,
  Select,
} from '../components/ui';
import { theme } from '../theme';

const EXECUTOR_KINDS = ['opencode', 'antigravity'];

export function ExecutorsPage() {
  const { data, loading, error, reload } = useAsync(() => executorsApi.list());
  const { data: proxies } = useAsync(() => proxyApi.list());
  const [name, setName] = useState('');
  const [kind, setKind] = useState(EXECUTOR_KINDS[0]);
  const [command, setCommand] = useState('');
  const [proxyId, setProxyId] = useState('');
  const [saving, setSaving] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const handleAdd = async (e: FormEvent) => {
    e.preventDefault();
    setSaving(true);
    setActionError(null);
    try {
      await executorsApi.save({
        name: name.trim(),
        kind,
        command: command.trim(),
        proxy_id: proxyId || undefined,
        enabled: true,
      });
      setName('');
      setCommand('');
      setProxyId('');
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

  const handleBindProxy = async (executor: Executor, value: string) => {
    setActionError(null);
    setBusyId(executor.id);
    try {
      await executorsApi.save({
        id: executor.id,
        name: executor.name,
        kind: executor.kind,
        command: executor.command,
        executable: executor.executable,
        working_directory: executor.working_directory,
        proxy_id: value || undefined,
        enabled: executor.enabled,
      });
      reload();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusyId(null);
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
        注册并检测本地任务执行器，可为每个执行器绑定网络代理
      </p>

      <Card style={{ marginBottom: '1rem' }}>
        <h3 style={{ margin: '0 0 1rem', color: theme.accent, fontSize: 15 }}>
          + 注册本地执行器
        </h3>
        <form
          onSubmit={handleAdd}
          style={{ display: 'flex', gap: '1rem', alignItems: 'flex-end', flexWrap: 'wrap' }}
        >
          <div style={{ flex: '1 1 180px' }}>
            <Label>显示名称</Label>
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="例如: 本地 OpenCode"
              required
            />
          </div>
          <div style={{ flex: '0 1 150px' }}>
            <Label>类型</Label>
            <Select
              aria-label="执行器类型"
              value={kind}
              onChange={(e) => setKind(e.target.value)}
            >
              {EXECUTOR_KINDS.map((k) => (
                <option key={k} value={k}>
                  {k}
                </option>
              ))}
            </Select>
          </div>
          <div style={{ flex: '2 1 260px' }}>
            <Label>终端命令或绝对路径</Label>
            <Input
              value={command}
              onChange={(e) => setCommand(e.target.value)}
              placeholder="例如: opencode"
              required
            />
          </div>
          <div style={{ flex: '1 1 180px' }}>
            <Label>代理</Label>
            <Select
              aria-label="代理绑定"
              value={proxyId}
              onChange={(e) => setProxyId(e.target.value)}
            >
              <option value="">不使用代理</option>
              {(proxies ?? []).map((proxy) => (
                <option key={proxy.id} value={proxy.id}>
                  {proxy.name}
                </option>
              ))}
            </Select>
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
                <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
                  <Select
                    aria-label={`代理绑定-${executor.name}`}
                    value={executor.proxy_id ?? ''}
                    disabled={busyId === executor.id}
                    onChange={(e) => void handleBindProxy(executor, e.target.value)}
                    style={{ maxWidth: 200 }}
                  >
                    <option value="">不使用代理</option>
                    {(proxies ?? []).map((proxy) => (
                      <option key={proxy.id} value={proxy.id}>
                        {proxy.name}
                      </option>
                    ))}
                  </Select>
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
