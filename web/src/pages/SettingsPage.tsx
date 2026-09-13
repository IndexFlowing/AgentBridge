// web/src/pages/SettingsPage.tsx
import { useState } from 'react';
import { proxyApi, systemApi } from '../api';
import { useAsync } from '../hooks/useAsync';
import { StateView } from '../components/StateView';
import {
  Badge,
  Card,
  CardHeader,
  InlineError,
  Input,
  Label,
  Spinner,
} from '../components/ui';
import { theme } from '../theme';

export function SettingsPage() {
  const connection = useAsync(() => systemApi.getConnection());
  const proxy = useAsync(() => proxyApi.get());
  const [proxyError, setProxyError] = useState<string | null>(null);
  const [toggling, setToggling] = useState(false);

  const toggleProxy = async (enabled: boolean) => {
    if (!proxy.data) return;
    setToggling(true);
    setProxyError(null);
    try {
      const updated = await proxyApi.save({
        enabled,
        kind: proxy.data.kind,
        host: proxy.data.host,
        port: proxy.data.port,
      });
      proxy.setData(updated);
    } catch (err) {
      setProxyError(err instanceof Error ? err.message : String(err));
    } finally {
      setToggling(false);
    }
  };

  return (
    <div>
      <h1 style={{ fontSize: 22, margin: '0 0 4px' }}>设置</h1>
      <p style={{ color: theme.muted, fontSize: 13, margin: '0 0 20px' }}>
        网关与网络配置
      </p>

      <Card style={{ marginBottom: '1rem' }}>
        <CardHeader title="网关监听配置" subtitle="修改后需重启 Rust 守护进程生效" />
        <StateView
          loading={connection.loading}
          error={connection.error}
          loadingLabel="读取配置..."
          onRetry={connection.reload}
        >
          {connection.data && (
            <div>
              <div
                style={{
                  color: theme.muted,
                  fontSize: 12,
                  marginBottom: '1rem',
                  wordBreak: 'break-all',
                }}
              >
                配置文件路径: {connection.data.config_path}
              </div>
              <div
                style={{
                  display: 'grid',
                  gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))',
                  gap: '1rem',
                }}
              >
                <div>
                  <Label>监听地址 (Host)</Label>
                  <Input disabled value={connection.data.host} />
                </div>
                <div>
                  <Label>监听端口 (Port)</Label>
                  <Input disabled value={String(connection.data.port)} />
                </div>
              </div>
              <div style={{ marginTop: '1rem', display: 'flex', gap: 8 }}>
                <Badge tone={connection.data.no_auth ? 'warning' : 'success'}>
                  {connection.data.no_auth ? 'No Auth' : 'Auth Enabled'}
                </Badge>
                {connection.data.oauth_enabled && (
                  <Badge tone="info">OAuth</Badge>
                )}
              </div>
              <div
                style={{
                  marginTop: '1rem',
                  color: theme.warning,
                  fontSize: 12,
                }}
              >
                * 修改监听地址或端口需要重启 Rust 守护进程。请直接编辑上方路径中的配置文件。
              </div>
            </div>
          )}
        </StateView>
      </Card>

      <Card>
        <CardHeader title="全局网络代理 (Proxy)" subtitle="仅编辑启用状态，凭据保持脱敏" />
        <StateView
          loading={proxy.loading}
          error={proxy.error}
          loadingLabel="读取代理配置..."
          onRetry={proxy.reload}
        >
          {proxy.data && (
            <div>
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '1rem',
                  marginBottom: '1rem',
                }}
              >
                <Label>启用代理</Label>
                <input
                  type="checkbox"
                  checked={proxy.data.enabled}
                  disabled={toggling}
                  onChange={(e) => void toggleProxy(e.target.checked)}
                />
                {toggling && <Spinner label="" />}
              </div>
              <div
                style={{
                  display: 'grid',
                  gridTemplateColumns: 'repeat(auto-fit, minmax(160px, 1fr))',
                  gap: '1rem',
                }}
              >
                <div>
                  <Label>协议类型</Label>
                  <Input disabled value={proxy.data.kind.toUpperCase()} />
                </div>
                <div>
                  <Label>代理主机</Label>
                  <Input disabled value={proxy.data.host} />
                </div>
                <div>
                  <Label>代理端口</Label>
                  <Input disabled value={String(proxy.data.port)} />
                </div>
              </div>
              {proxyError && <InlineError message={proxyError} />}
            </div>
          )}
        </StateView>
      </Card>
    </div>
  );
}
