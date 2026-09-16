// web/src/pages/ProxyPage.tsx
import { useState } from 'react';
import type { FormEvent } from 'react';
import { CheckCircle2, XCircle } from 'lucide-react';
import type { ProxyKind, ProxyRecord } from '../api';
import { proxyApi } from '../api';
import { useAsync } from '../hooks/useAsync';
import { StateView } from '../components/StateView';
import {
  Badge,
  Button,
  Card,
  CardHeader,
  InlineError,
  Input,
  Label,
  Select,
} from '../components/ui';
import { formatRelative } from '../lib/format';
import { theme } from '../theme';

const PROXY_KINDS: ProxyKind[] = ['http', 'https', 'socks5'];

interface FormState {
  name: string;
  kind: ProxyKind;
  host: string;
  port: string;
  username: string;
  password: string;
  testUrl: string;
  enabled: boolean;
  isDefault: boolean;
}

const EMPTY_FORM: FormState = {
  name: '',
  kind: 'http',
  host: '',
  port: '',
  username: '',
  password: '',
  testUrl: '',
  enabled: true,
  isDefault: false,
};

export function ProxyPage() {
  const { data, loading, error, reload } = useAsync(() => proxyApi.list());
  const [form, setForm] = useState<FormState>(EMPTY_FORM);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [verifyingId, setVerifyingId] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const update = <K extends keyof FormState>(key: K, value: FormState[K]) =>
    setForm((prev) => ({ ...prev, [key]: value }));

  const resetForm = () => {
    setForm(EMPTY_FORM);
    setEditingId(null);
  };

  const startEdit = (proxy: ProxyRecord) => {
    setActionError(null);
    setNotice(null);
    setEditingId(proxy.id);
    setForm({
      name: proxy.name,
      kind: proxy.kind,
      host: proxy.host,
      port: String(proxy.port),
      username: '',
      password: '',
      testUrl: proxy.test_url,
      enabled: proxy.enabled,
      isDefault: proxy.is_default,
    });
  };

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    const port = Number(form.port);
    if (!Number.isInteger(port) || port <= 0 || port > 65535) {
      setActionError('端口必须是 1-65535 之间的整数');
      return;
    }
    if (!form.name.trim()) {
      setActionError('代理名称不能为空');
      return;
    }
    if (!form.host.trim()) {
      setActionError('代理主机不能为空');
      return;
    }

    setSaving(true);
    setActionError(null);
    setNotice(null);
    try {
      const payload = {
        id: editingId ?? undefined,
        name: form.name.trim(),
        kind: form.kind,
        host: form.host.trim(),
        port,
        username: form.username.trim() || undefined,
        password: form.password || undefined,
        enabled: form.enabled,
        is_default: form.isDefault,
        test_url: form.testUrl.trim() || undefined,
      };
      if (editingId) {
        await proxyApi.update(editingId, payload);
      } else {
        await proxyApi.create(payload);
      }
      resetForm();
      reload();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  const runAction = async (id: string, action: () => Promise<unknown>) => {
    setActionError(null);
    setNotice(null);
    setBusyId(id);
    try {
      await action();
      reload();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setBusyId(null);
    }
  };

  const handleVerify = async (proxy: ProxyRecord) => {
    setActionError(null);
    setNotice(null);
    setVerifyingId(proxy.id);
    try {
      const result = await proxyApi.verify(proxy.id);
      setNotice(
        `「${proxy.name}」验证${result.success ? '成功' : '失败'}：${result.message}（${result.latency_ms}ms）`,
      );
      reload();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setVerifyingId(null);
    }
  };

  const handleDelete = (proxy: ProxyRecord) => {
    if (!window.confirm(`确定删除代理「${proxy.name}」吗？`)) return;
    void runAction(proxy.id, () => proxyApi.remove(proxy.id));
  };

  return (
    <div>
      <h1 style={{ fontSize: 22, margin: '0 0 4px' }}>代理</h1>
      <p style={{ color: theme.muted, fontSize: 13, margin: '0 0 20px' }}>
        管理执行器可绑定的网络代理，并验证真实连通性
      </p>

      <Card style={{ marginBottom: '1rem' }}>
        <CardHeader
          title={editingId ? '编辑代理' : '添加代理'}
          subtitle="密码留空则保持原有凭据；验证目标留空时使用内置健康检查地址"
        />
        <form onSubmit={handleSubmit}>
          <div
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fit, minmax(180px, 1fr))',
              gap: '1rem',
            }}
          >
            <div>
              <Label>名称</Label>
              <Input
                aria-label="代理名称"
                value={form.name}
                onChange={(e) => update('name', e.target.value)}
                placeholder="例如: 公司代理"
                required
              />
            </div>
            <div>
              <Label>协议</Label>
              <Select
                aria-label="代理协议"
                value={form.kind}
                onChange={(e) => update('kind', e.target.value as ProxyKind)}
              >
                {PROXY_KINDS.map((kind) => (
                  <option key={kind} value={kind}>
                    {kind.toUpperCase()}
                  </option>
                ))}
              </Select>
            </div>
            <div>
              <Label>主机</Label>
              <Input
                aria-label="代理主机"
                value={form.host}
                onChange={(e) => update('host', e.target.value)}
                placeholder="127.0.0.1"
                required
              />
            </div>
            <div>
              <Label>端口</Label>
              <Input
                aria-label="代理端口"
                value={form.port}
                onChange={(e) => update('port', e.target.value)}
                placeholder="7890"
                inputMode="numeric"
                required
              />
            </div>
            <div>
              <Label>用户名（可选）</Label>
              <Input
                aria-label="代理用户名"
                value={form.username}
                onChange={(e) => update('username', e.target.value)}
                placeholder="留空则保持原凭据"
              />
            </div>
            <div>
              <Label>密码（可选）</Label>
              <Input
                aria-label="代理密码"
                type="password"
                value={form.password}
                onChange={(e) => update('password', e.target.value)}
                placeholder="留空则保持原凭据"
              />
            </div>
            <div>
              <Label>验证目标（可选）</Label>
              <Input
                aria-label="验证目标"
                value={form.testUrl}
                onChange={(e) => update('testUrl', e.target.value)}
                placeholder="https://example.com"
              />
            </div>
          </div>

          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '1.5rem',
              marginTop: '1rem',
              flexWrap: 'wrap',
            }}
          >
            <label style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 13 }}>
              <input
                type="checkbox"
                aria-label="启用代理"
                checked={form.enabled}
                onChange={(e) => update('enabled', e.target.checked)}
              />
              启用
            </label>
            <label style={{ display: 'flex', alignItems: 'center', gap: 6, fontSize: 13 }}>
              <input
                type="checkbox"
                aria-label="设为默认代理"
                checked={form.isDefault}
                onChange={(e) => update('isDefault', e.target.checked)}
              />
              设为默认
            </label>
            <div style={{ display: 'flex', gap: 8, marginLeft: 'auto' }}>
              {editingId && (
                <Button type="button" variant="secondary" onClick={resetForm} disabled={saving}>
                  取消
                </Button>
              )}
              <Button type="submit" disabled={saving}>
                {saving ? '保存中...' : editingId ? '更新代理' : '保存代理'}
              </Button>
            </div>
          </div>
        </form>
        {actionError && <InlineError message={actionError} />}
        {notice && (
          <div style={{ color: theme.success, fontSize: 12, marginTop: 6 }}>{notice}</div>
        )}
      </Card>

      <StateView
        loading={loading}
        error={error}
        onRetry={reload}
        empty={!!data && data.length === 0}
        emptyTitle="暂无代理"
        emptyHint="添加一个代理后即可在执行器中选择绑定。"
      >
        <div style={{ display: 'flex', flexDirection: 'column', gap: '1rem' }}>
          {data?.map((proxy) => (
            <Card key={proxy.id} active={editingId === proxy.id}>
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
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 16, fontWeight: 700 }}>
                    {proxy.name}
                    {proxy.is_default && <Badge tone="accent">默认</Badge>}
                    <Badge tone={proxy.enabled ? 'success' : 'neutral'}>
                      {proxy.enabled ? '已启用' : '已禁用'}
                    </Badge>
                  </div>
                  <div
                    style={{
                      color: theme.muted,
                      fontFamily: 'monospace',
                      fontSize: 12,
                      marginTop: 6,
                      wordBreak: 'break-all',
                    }}
                  >
                    {proxy.kind.toUpperCase()} {proxy.host}:{proxy.port}
                  </div>
                  <div style={{ display: 'flex', gap: 6, marginTop: 10, flexWrap: 'wrap' }}>
                    <Badge tone={proxy.username_configured ? 'info' : 'neutral'}>
                      {proxy.username_configured ? '已配置凭据' : '无凭据'}
                    </Badge>
                    {proxy.test_url && <Badge tone="neutral">目标 {proxy.test_url}</Badge>}
                  </div>
                  <div
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: 6,
                      marginTop: 12,
                      fontSize: 13,
                      color:
                        proxy.last_verified_ok === null
                          ? theme.muted
                          : proxy.last_verified_ok
                            ? theme.success
                            : theme.danger,
                    }}
                  >
                    {proxy.last_verified_ok === null ? null : proxy.last_verified_ok ? (
                      <CheckCircle2 size={16} />
                    ) : (
                      <XCircle size={16} />
                    )}
                    {proxy.last_verified_ok === null
                      ? '尚未验证'
                      : proxy.last_verified_ok
                        ? `验证成功 (${proxy.last_verified_latency_ms ?? 0}ms)`
                        : '验证失败'}
                    {proxy.last_verified_at
                      ? ` · ${formatRelative(proxy.last_verified_at)}`
                      : ''}
                  </div>
                </div>
                <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
                  <Button
                    variant="secondary"
                    aria-label={`连接验证-${proxy.name}`}
                    disabled={verifyingId === proxy.id || busyId === proxy.id}
                    onClick={() => void handleVerify(proxy)}
                  >
                    {verifyingId === proxy.id ? '验证中...' : '连接验证'}
                  </Button>
                  <Button
                    variant="secondary"
                    aria-label={`切换-${proxy.name}`}
                    disabled={busyId === proxy.id}
                    onClick={() =>
                      void runAction(proxy.id, () =>
                        proxyApi.setEnabled(proxy.id, !proxy.enabled),
                      )
                    }
                  >
                    {proxy.enabled ? '禁用' : '启用'}
                  </Button>
                  <Button variant="secondary" aria-label={`编辑-${proxy.name}`} onClick={() => startEdit(proxy)}>
                    编辑
                  </Button>
                  <Button variant="danger" aria-label={`删除-${proxy.name}`} onClick={() => handleDelete(proxy)}>
                    删除
                  </Button>
                </div>
              </div>
            </Card>
          ))}
        </div>
      </StateView>
    </div>
  );
}
