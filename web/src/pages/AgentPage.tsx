// web/src/pages/AgentPage.tsx
import { useState } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { skillsApi } from '../api';
import type { AgentRule } from '../api';
import { useAsync } from '../hooks/useAsync';
import { StateView } from '../components/StateView';
import { Badge, Card, StatCard } from '../components/ui';
import { theme } from '../theme';

export function AgentPage() {
  const { data, loading, error, reload } = useAsync(() => skillsApi.agent());

  return (
    <div>
      <h1 style={{ fontSize: 22, margin: '0 0 4px' }}>Agent</h1>
      <p style={{ color: theme.muted, fontSize: 13, margin: '0 0 20px' }}>
        AgentBridge Agent 根目录（agent.yaml / Rules / Skills）的运行时视图
      </p>

      <StateView
        loading={loading}
        error={error}
        onRetry={reload}
        empty={!!data && !data.found}
        emptyTitle="未找到 Agent 根目录"
        emptyHint="运行一次 AgentBridge 以初始化 ~/.agentbridge/.agent。"
      >
        {data && data.found && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: '1rem' }}>
            <Card>
              <div
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  gap: 12,
                  flexWrap: 'wrap',
                }}
              >
                <div>
                  <div style={{ fontSize: 18, fontWeight: 700 }}>
                    {data.manifest?.name?.trim() || '（未命名 Agent）'}
                    {data.manifest?.version && (
                      <span style={{ marginLeft: 8 }}>
                        <Badge tone="neutral">v{data.manifest.version}</Badge>
                      </span>
                    )}
                  </div>
                  {data.manifest?.description?.trim() && (
                    <div
                      style={{
                        color: theme.muted,
                        fontSize: 13,
                        marginTop: 8,
                        maxWidth: 720,
                        whiteSpace: 'pre-wrap',
                      }}
                    >
                      {data.manifest.description}
                    </div>
                  )}
                </div>
                <div
                  style={{
                    fontFamily: 'monospace',
                    fontSize: 12,
                    color: theme.faint,
                    wordBreak: 'break-all',
                    maxWidth: 360,
                  }}
                >
                  {data.agent_dir}
                </div>
              </div>
            </Card>

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
                label="启用 Skills"
                value={data.skills.filter((s) => s.enabled).length}
                tone="success"
              />
            </div>

            <Card>
              <h3 style={{ margin: '0 0 12px', color: theme.accent, fontSize: 15 }}>
                Rules（{data.rules.length}）
              </h3>
              {data.rules.length === 0 ? (
                <div style={{ color: theme.muted, fontSize: 13 }}>未加载任何 Rule。</div>
              ) : (
                <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                  {data.rules.map((rule) => (
                    <RuleRow key={`${rule.path}:${rule.name}`} rule={rule} />
                  ))}
                </div>
              )}
            </Card>

            <Card>
              <h3 style={{ margin: '0 0 12px', color: theme.accent, fontSize: 15 }}>
                Skills（{data.skills.length}）
              </h3>
              {data.skills.length === 0 ? (
                <div style={{ color: theme.muted, fontSize: 13 }}>
                  未在 Agent 根目录发现 Skill。
                </div>
              ) : (
                <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
                  {data.skills.map((skill) => (
                    <div
                      key={skill.id}
                      style={{
                        display: 'flex',
                        justifyContent: 'space-between',
                        gap: 12,
                        alignItems: 'flex-start',
                        paddingBottom: 10,
                        borderBottom: `1px solid ${theme.border}`,
                      }}
                    >
                      <div style={{ minWidth: 0 }}>
                        <div
                          style={{
                            display: 'flex',
                            alignItems: 'center',
                            gap: 8,
                            fontWeight: 600,
                          }}
                        >
                          {skill.name}
                          <Badge tone={skill.enabled ? 'success' : 'neutral'}>
                            {skill.enabled ? '已启用' : '未启用'}
                          </Badge>
                          <Badge tone="info">{skill.source}</Badge>
                        </div>
                        <div style={{ color: theme.muted, fontSize: 12, marginTop: 4 }}>
                          {skill.description}
                        </div>
                      </div>
                      <div
                        style={{
                          color: theme.faint,
                          fontFamily: 'monospace',
                          fontSize: 12,
                          textAlign: 'right',
                        }}
                      >
                        v{skill.version}
                      </div>
                    </div>
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

function RuleRow({ rule }: { rule: AgentRule }) {
  const [open, setOpen] = useState(false);
  return (
    <div
      style={{
        border: `1px solid ${theme.border}`,
        borderRadius: 8,
        background: theme.surfaceAlt,
      }}
    >
      <button
        onClick={() => setOpen((v) => !v)}
        style={{
          width: '100%',
          display: 'flex',
          alignItems: 'center',
          gap: 8,
          padding: '10px 12px',
          background: 'transparent',
          border: 'none',
          color: theme.text,
          cursor: 'pointer',
          textAlign: 'left',
        }}
      >
        {open ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
        <span style={{ fontWeight: 600 }}>{rule.name}</span>
        <span
          style={{
            marginLeft: 'auto',
            fontFamily: 'monospace',
            fontSize: 12,
            color: theme.faint,
          }}
        >
          {rule.path}
        </span>
      </button>
      {open && (
        <pre
          style={{
            margin: 0,
            padding: '0 14px 14px',
            whiteSpace: 'pre-wrap',
            wordBreak: 'break-word',
            fontFamily: 'monospace',
            fontSize: 12,
            color: theme.muted,
            maxHeight: 360,
            overflow: 'auto',
          }}
        >
          {rule.content}
        </pre>
      )}
    </div>
  );
}
