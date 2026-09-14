// web/src/pages/AgentContextPage.tsx
import { useEffect, useState } from 'react';
import { projectsApi, skillsApi } from '../api';
import { useAsync } from '../hooks/useAsync';
import { StateView } from '../components/StateView';
import { Badge, Button, Card, Input, Label, Select, StatCard } from '../components/ui';
import { theme } from '../theme';

function splitCsv(raw: string): string[] {
  return raw
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);
}

export function AgentContextPage() {
  const projects = useAsync(() => projectsApi.list());
  const [project, setProject] = useState('');
  const [requested, setRequested] = useState('');
  const [applied, setApplied] = useState<{ project: string; requested: string } | null>(
    null,
  );

  useEffect(() => {
    if (!applied && projects.data && projects.data.length > 0) {
      const def =
        projects.data.find((p) => p.active)?.name ?? projects.data[0].name;
      setProject(def);
      setApplied({ project: def, requested: '' });
    }
  }, [projects.data, applied]);

  const context = useAsync(
    () =>
      applied
        ? skillsApi.context(applied.project, splitCsv(applied.requested))
        : Promise.resolve(null),
    [applied?.project, applied?.requested],
  );

  const data = context.data;

  return (
    <div>
      <h1 style={{ fontSize: 22, margin: '0 0 4px' }}>Agent Context</h1>
      <p style={{ color: theme.muted, fontSize: 13, margin: '0 0 20px' }}>
        诊断当前项目任务实际加载的 Rules 与 Skills（只读，不执行任务）
      </p>

      <Card style={{ marginBottom: '1rem' }}>
        <div
          style={{
            display: 'flex',
            gap: '1rem',
            alignItems: 'flex-end',
            flexWrap: 'wrap',
          }}
        >
          <div style={{ flex: '1 1 220px' }}>
            <Label>项目</Label>
            <Select value={project} onChange={(e) => setProject(e.target.value)}>
              {projects.data?.map((p) => (
                <option key={p.id || p.name} value={p.name}>
                  {p.name}
                  {p.active ? '（当前）' : ''}
                </option>
              ))}
            </Select>
          </div>
          <div style={{ flex: '2 1 320px' }}>
            <Label>附加请求 Skills（逗号分隔，可留空）</Label>
            <Input
              value={requested}
              onChange={(e) => setRequested(e.target.value)}
              placeholder="例如: rust-skills, design-pattern-review"
            />
          </div>
          <Button
            onClick={() => setApplied({ project, requested })}
            disabled={!project}
          >
            解析上下文
          </Button>
        </div>
      </Card>

      <StateView
        loading={context.loading}
        error={context.error}
        onRetry={context.reload}
        empty={!!data && data.skills.length === 0 && data.rules.length === 0}
        emptyTitle="上下文为空"
        emptyHint="该项目的 Agent 根目录未加载任何 Rule/Skill。"
      >
        {data && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: '1rem' }}>
            <Card>
              <div
                style={{
                  display: 'grid',
                  gridTemplateColumns: 'repeat(auto-fit, minmax(180px, 1fr))',
                  gap: '1rem',
                }}
              >
                <StatCard label="Rules" value={data.rules.length} tone="accent" />
                <StatCard label="Skills" value={data.skills.length} tone="info" />
                <StatCard
                  label="Agent"
                  value={data.manifest?.name?.trim() || '未命名'}
                  tone="success"
                />
              </div>
              <div
                style={{
                  marginTop: 12,
                  fontFamily: 'monospace',
                  fontSize: 12,
                  color: theme.faint,
                  display: 'flex',
                  flexDirection: 'column',
                  gap: 4,
                }}
              >
                <div>PROJECT: {data.project}</div>
                <div>WORKSPACE: {data.workspace}</div>
                <div>AGENT_ROOT: {data.agent_root}</div>
                <div>TASK_ID: {data.task_id}</div>
              </div>
            </Card>

            <Card>
              <h3 style={{ margin: '0 0 12px', color: theme.accent, fontSize: 15 }}>
                已加载 Skills（{data.skills.length}）
              </h3>
              {data.skills.length === 0 ? (
                <div style={{ color: theme.muted, fontSize: 13 }}>无</div>
              ) : (
                <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
                  {data.skills.map((skill) => (
                    <Badge key={skill} tone="info">
                      {skill}
                    </Badge>
                  ))}
                </div>
              )}
            </Card>

            <Card>
              <h3 style={{ margin: '0 0 12px', color: theme.accent, fontSize: 15 }}>
                已加载 Rules（{data.rules.length}）
              </h3>
              {data.rules.length === 0 ? (
                <div style={{ color: theme.muted, fontSize: 13 }}>无</div>
              ) : (
                <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                  {data.rules.map((rule) => (
                    <details
                      key={`${rule.path}:${rule.name}`}
                      style={{
                        border: `1px solid ${theme.border}`,
                        borderRadius: 8,
                        background: theme.surfaceAlt,
                        padding: '8px 12px',
                      }}
                    >
                      <summary
                        style={{
                          cursor: 'pointer',
                          fontWeight: 600,
                          display: 'flex',
                          gap: 8,
                        }}
                      >
                        {rule.name}
                        <span
                          style={{
                            fontFamily: 'monospace',
                            fontSize: 12,
                            color: theme.faint,
                            fontWeight: 400,
                          }}
                        >
                          {rule.path}
                        </span>
                      </summary>
                      <pre
                        style={{
                          margin: '10px 0 0',
                          whiteSpace: 'pre-wrap',
                          wordBreak: 'break-word',
                          fontFamily: 'monospace',
                          fontSize: 12,
                          color: theme.muted,
                          maxHeight: 320,
                          overflow: 'auto',
                        }}
                      >
                        {rule.content}
                      </pre>
                    </details>
                  ))}
                </div>
              )}
            </Card>
          </div>
        )}
      </StateView>
    </div>
  );
}
