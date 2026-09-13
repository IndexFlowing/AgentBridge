// web/src/pages/DashboardPage.tsx
import type { ReactNode } from 'react';
import { Link } from 'react-router-dom';
import { RefreshCw } from 'lucide-react';
import type {
  DashboardData,
  Executor,
  ProxyData,
  TaskData,
} from '../api';
import { dashboardApi, executorsApi, proxyApi } from '../api';
import type { AsyncState } from '../hooks/useAsync';
import { useAsync } from '../hooks/useAsync';
import { StateView } from '../components/StateView';
import {
  Badge,
  Button,
  Card,
  CardHeader,
  EmptyState,
  ErrorState,
  Spinner,
  StatCard,
  type Tone,
} from '../components/ui';
import { theme } from '../theme';
import {
  formatDuration,
  formatRelative,
  orUnknown,
} from '../lib/format';

function statusTone(status: string | null | undefined): Tone {
  switch ((status ?? '').toLowerCase()) {
    case 'running':
      return 'info';
    case 'success':
    case 'executed':
    case 'review':
    case 'done':
      return 'success';
    case 'failed':
    case 'blocked':
      return 'danger';
    case 'cancelled':
      return 'warning';
    default:
      return 'neutral';
  }
}

function KeyValue({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 3 }}>
      <span style={{ fontSize: 11, color: theme.faint }}>{label}</span>
      <span style={{ fontSize: 13, wordBreak: 'break-word' }}>{children}</span>
    </div>
  );
}

function pickCurrentTask(tasks: TaskData[]): TaskData | null {
  const withTask = tasks.filter((t) => t.task_id);
  if (withTask.length === 0) return null;
  const running = withTask.find(
    (t) => (t.lifecycle ?? '').toLowerCase() === 'running',
  );
  if (running) return running;
  return [...withTask].sort(
    (a, b) =>
      new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime(),
  )[0];
}

function GatewayCard({ data }: { data: DashboardData }) {
  const gateway = data.gateway;
  return (
    <Card>
      <CardHeader
        title="Service / Gateway"
        subtitle="MCP 控制平面接入点"
        right={
          gateway.online ? (
            <Badge tone="success">● 在线</Badge>
          ) : (
            <Badge tone="danger">○ 离线</Badge>
          )
        }
      />
      <div style={{ display: 'grid', gap: 12 }}>
        <KeyValue label="Endpoint">
          <code style={{ color: theme.accent }}>{orUnknown(gateway.endpoint)}</code>
        </KeyValue>
        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
          <KeyValue label="Host">{orUnknown(gateway.host)}</KeyValue>
          <KeyValue label="Port">{gateway.port || 'Unknown'}</KeyValue>
        </div>
        <KeyValue label="Managed">{gateway.managed ? '是' : '否'}</KeyValue>
        <KeyValue label={`已连接客户端 (${gateway.clients.length})`}>
          {gateway.clients.length === 0 ? (
            <span style={{ color: theme.faint }}>无已连接客户端</span>
          ) : (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 3 }}>
              {gateway.clients.slice(0, 5).map((c) => (
                <span key={c.client_id}>
                  {c.client_name || c.client_id}
                  {c.has_active_token ? '' : ' (无活动令牌)'}
                </span>
              ))}
            </div>
          )}
        </KeyValue>
      </div>
    </Card>
  );
}

function ExecutorCard({
  defaultKind,
  executors,
  loading,
  error,
  onRetry,
}: {
  defaultKind: string;
  executors: Executor[] | null;
  loading: boolean;
  error: string | null;
  onRetry: () => void;
}) {
  return (
    <Card>
      <CardHeader
        title="Executor"
        subtitle="任务执行器"
        right={<Badge tone="accent">{orUnknown(defaultKind)}</Badge>}
      />
      {loading ? (
        <Spinner label="读取执行器..." />
      ) : error ? (
        <ErrorState message={error} onRetry={onRetry} />
      ) : !executors || executors.length === 0 ? (
        <EmptyState title="未检测到执行器" />
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          {executors.map((ex) => (
            <div
              key={ex.id || ex.name}
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'center',
                gap: 8,
                fontSize: 13,
              }}
            >
              <span>
                {ex.name}
                {ex.version ? (
                  <span style={{ color: theme.faint }}> {ex.version}</span>
                ) : null}
              </span>
              <Badge tone={ex.available ? 'success' : 'danger'}>
                {ex.available ? '就绪' : ex.status || '不可用'}
              </Badge>
            </div>
          ))}
        </div>
      )}
    </Card>
  );
}

function ProxyStatusCard({
  proxy,
  loading,
  error,
  onRetry,
}: {
  proxy: ProxyData | null;
  loading: boolean;
  error: string | null;
  onRetry: () => void;
}) {
  return (
    <Card>
      <CardHeader
        title="Proxy"
        subtitle="仅展示脱敏状态"
        right={
          proxy ? (
            proxy.enabled ? (
              <Badge tone="success">已启用</Badge>
            ) : (
              <Badge tone="neutral">未启用</Badge>
            )
          ) : undefined
        }
      />
      {loading ? (
        <Spinner label="读取代理状态..." />
      ) : error ? (
        <ErrorState message={error} onRetry={onRetry} />
      ) : !proxy ? (
        <EmptyState title="无代理配置" />
      ) : (
        <div style={{ display: 'grid', gap: 12 }}>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
            <KeyValue label="类型">{proxy.kind.toUpperCase()}</KeyValue>
            <KeyValue label="端口">{proxy.port || 'Unknown'}</KeyValue>
          </div>
          <KeyValue label="主机">{orUnknown(proxy.host)}</KeyValue>
          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}>
            <KeyValue label="用户名">
              <Badge tone={proxy.username_configured ? 'success' : 'neutral'}>
                {proxy.username_configured ? '已配置' : '未配置'}
              </Badge>
            </KeyValue>
            <KeyValue label="密码">
              <Badge tone={proxy.password_configured ? 'success' : 'neutral'}>
                {proxy.password_configured ? '已配置' : '未配置'}
              </Badge>
            </KeyValue>
          </div>
        </div>
      )}
    </Card>
  );
}

export function DashboardPage() {
  const dashboard = useAsync(() => dashboardApi.get());
  const proxy = useAsync(() => proxyApi.get());
  const executors = useAsync(() => executorsApi.list());

  return (
    <div>
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'flex-start',
          marginBottom: 20,
          gap: 12,
        }}
      >
        <div>
          <h1 style={{ fontSize: 22, margin: 0 }}>仪表盘</h1>
          <p style={{ color: theme.muted, fontSize: 13, margin: '4px 0 0' }}>
            AgentBridge 控制平面实时状态
          </p>
        </div>
        <Button
          variant="secondary"
          onClick={() => {
            dashboard.reload();
            proxy.reload();
            executors.reload();
          }}
        >
          <span
            style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}
          >
            <RefreshCw size={14} /> 刷新
          </span>
        </Button>
      </div>

      <StateView
        loading={dashboard.loading}
        error={dashboard.error}
        loadingLabel="正在从 Rust Core 同步状态..."
        onRetry={dashboard.reload}
      >
        {dashboard.data && (
          <DashboardBody
            data={dashboard.data}
            proxy={proxy}
            executors={executors}
          />
        )}
      </StateView>
    </div>
  );
}

function DashboardBody({
  data,
  proxy,
  executors,
}: {
  data: DashboardData;
  proxy: AsyncState<ProxyData>;
  executors: AsyncState<Executor[]>;
}) {
  const currentTask = pickCurrentTask(data.tasks);
  const activeProjects = data.projects.filter((p) => p.active).length;
  const availableExecutors =
    executors.data?.filter((e) => e.available).length ?? 0;
  const runningTasks = data.tasks.filter(
    (t) => (t.lifecycle ?? '').toLowerCase() === 'running',
  ).length;

  return (
    <>
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fit, minmax(190px, 1fr))',
          gap: '1rem',
          marginBottom: '1rem',
        }}
      >
        <StatCard
          label="Gateway"
          value={data.gateway.online ? 'Running' : 'Offline'}
          hint={data.gateway.endpoint || 'Unknown'}
          tone={data.gateway.online ? 'success' : 'danger'}
        />
        <StatCard
          label="Projects"
          value={data.projects.length}
          hint={`${activeProjects} 个激活`}
        />
        <StatCard
          label="Executors"
          value={executors.data ? availableExecutors : '—'}
          hint={
            executors.data
              ? `共 ${executors.data.length} 个`
              : executors.loading
                ? '读取中...'
                : 'Unknown'
          }
          tone="info"
        />
        <StatCard
          label="Tasks Running"
          value={runningTasks}
          hint={`共 ${data.tasks.length} 个任务快照`}
          tone={runningTasks > 0 ? 'warning' : 'neutral'}
        />
      </div>

      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fit, minmax(340px, 1fr))',
          gap: '1rem',
          marginBottom: '1rem',
        }}
      >
        <GatewayCard data={data} />
        <ExecutorCard
          defaultKind={data.executor}
          executors={executors.data}
          loading={executors.loading}
          error={executors.error}
          onRetry={executors.reload}
        />
        <ProxyStatusCard
          proxy={proxy.data}
          loading={proxy.loading}
          error={proxy.error}
          onRetry={proxy.reload}
        />
      </div>

      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fit, minmax(340px, 1fr))',
          gap: '1rem',
          alignItems: 'start',
        }}
      >
        <Card>
          <CardHeader
            title="Projects"
            subtitle={`${data.projects.length} 个已挂载项目`}
            right={
              <Link
                to="/projects"
                style={{ color: theme.accent, fontSize: 12 }}
              >
                管理
              </Link>
            }
          />
          {data.projects.length === 0 ? (
            <EmptyState title="暂无项目" hint="前往「项目」页面挂载本地工作区。" />
          ) : (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
              {data.projects.slice(0, 6).map((p) => (
                <div
                  key={p.id || p.name}
                  style={{
                    display: 'flex',
                    justifyContent: 'space-between',
                    alignItems: 'center',
                    gap: 8,
                  }}
                >
                  <div style={{ minWidth: 0 }}>
                    <div
                      style={{
                        display: 'flex',
                        alignItems: 'center',
                        gap: 8,
                        fontSize: 14,
                        fontWeight: 600,
                      }}
                    >
                      {p.name}
                      {p.active && <Badge tone="accent">当前</Badge>}
                    </div>
                    <div
                      style={{
                        fontSize: 11,
                        color: theme.faint,
                        fontFamily: 'monospace',
                        overflow: 'hidden',
                        textOverflow: 'ellipsis',
                        whiteSpace: 'nowrap',
                      }}
                      title={p.path}
                    >
                      {p.path || 'Unknown'}
                    </div>
                  </div>
                  <Badge tone="neutral">{orUnknown(p.executor)}</Badge>
                </div>
              ))}
            </div>
          )}
        </Card>

        <Card>
          <CardHeader
            title="Current Task"
            subtitle={currentTask ? currentTask.project : '当前无活动任务'}
            right={
              currentTask ? (
                <Badge tone={statusTone(currentTask.lifecycle)}>
                  {orUnknown(currentTask.lifecycle)}
                </Badge>
              ) : undefined
            }
          />
          {!currentTask ? (
            <EmptyState title="当前没有任务" hint="Core 尚未记录任何任务状态。" />
          ) : (
            <div style={{ display: 'grid', gap: 10 }}>
              <KeyValue label="Task ID">
                <code>{currentTask.task_id}</code>
              </KeyValue>
              <div
                style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}
              >
                <KeyValue label="Iteration">{currentTask.iteration}</KeyValue>
                <KeyValue label="Executor">
                  {orUnknown(currentTask.executor)}
                </KeyValue>
              </div>
              <div
                style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}
              >
                <KeyValue label="开始">
                  {formatRelative(currentTask.started_at)}
                </KeyValue>
                <KeyValue label="耗时">
                  {formatDuration(currentTask.started_at, currentTask.finished_at)}
                </KeyValue>
              </div>
              <KeyValue label="Goal">
                {orUnknown(currentTask.goal)}
              </KeyValue>
              {(currentTask.summary || currentTask.error) && (
                <KeyValue label={currentTask.error ? 'Error' : 'Summary'}>
                  <span
                    style={{
                      color: currentTask.error ? theme.danger : theme.text,
                    }}
                  >
                    {orUnknown(currentTask.error || currentTask.summary)}
                  </span>
                </KeyValue>
              )}
              <div
                style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 12 }}
              >
                <KeyValue label="变更文件">
                  {currentTask.changed_files.length}
                </KeyValue>
                <KeyValue label="测试">
                  {currentTask.tests ? (
                    <Badge tone={statusTone(currentTask.tests.status)}>
                      {currentTask.tests.status}
                    </Badge>
                  ) : (
                    '—'
                  )}
                </KeyValue>
              </div>
            </div>
          )}
        </Card>

        <Card>
          <CardHeader
            title="Activity"
            subtitle="最近任务动态"
            right={
              data.activity.length > 0 ? (
                <span style={{ fontSize: 12, color: theme.faint }}>
                  {data.activity.length} 条
                </span>
              ) : undefined
            }
          />
          {data.activity.length === 0 ? (
            <EmptyState title="暂无动态" />
          ) : (
            <div style={{ display: 'flex', flexDirection: 'column', gap: 12 }}>
              {[...data.activity]
                .sort(
                  (a, b) =>
                    new Date(b.timestamp).getTime() -
                    new Date(a.timestamp).getTime(),
                )
                .slice(0, 8)
                .map((item, idx) => (
                  <div key={`${item.project}-${item.task_id}-${idx}`}>
                    <div
                      style={{
                        display: 'flex',
                        justifyContent: 'space-between',
                        gap: 8,
                        fontSize: 13,
                      }}
                    >
                      <span style={{ fontWeight: 600 }}>{item.project}</span>
                      <Badge tone={statusTone(item.status)}>
                        {orUnknown(item.status)}
                      </Badge>
                    </div>
                    <div style={{ fontSize: 12, color: theme.muted, marginTop: 2 }}>
                      {orUnknown(item.goal || item.summary)}
                    </div>
                    <div style={{ fontSize: 11, color: theme.faint, marginTop: 2 }}>
                      {formatRelative(item.timestamp)}
                    </div>
                  </div>
                ))}
            </div>
          )}
        </Card>
      </div>
    </>
  );
}
