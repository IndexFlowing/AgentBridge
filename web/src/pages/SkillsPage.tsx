// web/src/pages/SkillsPage.tsx
import type { ReactNode } from 'react';
import { useState } from 'react';
import { ChevronDown, ChevronRight, Power, Trash2 } from 'lucide-react';
import { skillsApi } from '../api';
import type { SkillSummary } from '../api';
import { useAsync } from '../hooks/useAsync';
import { StateView } from '../components/StateView';
import { Badge, Button, Card, InlineError } from '../components/ui';
import { theme } from '../theme';

export function SkillsPage() {
  const installed = useAsync(() => skillsApi.list());
  const agent = useAsync(() => skillsApi.agent());
  const [actionError, setActionError] = useState<string | null>(null);

  const toggle = async (skill: SkillSummary) => {
    setActionError(null);
    try {
      if (skill.enabled) await skillsApi.disable(skill.name);
      else await skillsApi.enable(skill.name);
      installed.reload();
      agent.reload();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    }
  };

  const remove = async (skill: SkillSummary) => {
    if (!window.confirm(`确定要移除技能「${skill.name}」吗？`)) return;
    setActionError(null);
    try {
      await skillsApi.remove(skill.name);
      installed.reload();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    }
  };

  return (
    <div>
      <h1 style={{ fontSize: 22, margin: '0 0 4px' }}>技能</h1>
      <p style={{ color: theme.muted, fontSize: 13, margin: '0 0 20px' }}>
        已安装 Skill 与 Agent 根目录 Skill 的加载状态
      </p>
      {actionError && <InlineError message={actionError} />}

      <Card style={{ marginBottom: '1rem' }}>
        <h3 style={{ margin: '0 0 12px', color: theme.accent, fontSize: 15 }}>
          Agent 根目录 Skills（{agent.data ? agent.data.skills.length : '…'}）
        </h3>
        <StateView
          loading={agent.loading}
          error={agent.error}
          onRetry={agent.reload}
          empty={!!agent.data && agent.data.skills.length === 0}
          emptyTitle="未发现 Agent Skill"
        >
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            {agent.data?.skills.map((skill) => (
              <SkillRow
                key={skill.id}
                name={skill.name}
                description={skill.description}
                version={skill.version}
                source={skill.source}
                enabled={skill.enabled}
              />
            ))}
          </div>
        </StateView>
      </Card>

      <Card>
        <h3 style={{ margin: '0 0 12px', color: theme.accent, fontSize: 15 }}>
          已安装 Skills（{installed.data ? installed.data.length : '…'}）
        </h3>
        <StateView
          loading={installed.loading}
          error={installed.error}
          onRetry={installed.reload}
          empty={!!installed.data && installed.data.length === 0}
          emptyTitle="暂无已安装技能"
          emptyHint="通过 CLI `agentbridge skill install <source>` 安装技能。"
        >
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            {installed.data?.map((skill) => (
              <SkillRow
                key={skill.id}
                name={skill.name}
                description={skill.description}
                version={skill.version}
                source={skill.source}
                enabled={skill.enabled}
                actions={
                  <>
                    <Button
                      variant="ghost"
                      title={skill.enabled ? '禁用' : '启用'}
                      onClick={() => toggle(skill)}
                      style={{
                        padding: 8,
                        color: skill.enabled ? theme.success : theme.faint,
                      }}
                    >
                      <Power size={16} />
                    </Button>
                    <Button
                      variant="ghost"
                      title="移除"
                      onClick={() => remove(skill)}
                      style={{ padding: 8, color: theme.danger }}
                    >
                      <Trash2 size={16} />
                    </Button>
                  </>
                }
              />
            ))}
          </div>
        </StateView>
      </Card>
    </div>
  );
}

interface SkillRowProps {
  name: string;
  description: string;
  version: string;
  source: string;
  enabled: boolean;
  actions?: ReactNode;
}

function SkillRow({
  name,
  description,
  version,
  source,
  enabled,
  actions,
}: SkillRowProps) {
  const [open, setOpen] = useState(false);
  const detail = useAsync(
    () => (open ? skillsApi.detail(name) : Promise.resolve(null)),
    [open, name],
  );

  return (
    <div style={{ borderBottom: `1px solid ${theme.border}`, paddingBottom: 6 }}>
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          gap: 12,
          alignItems: 'center',
        }}
      >
        <div style={{ minWidth: 0 }}>
          <div
            style={{ display: 'flex', alignItems: 'center', gap: 8, fontWeight: 600 }}
          >
            {name}
            <Badge tone={enabled ? 'success' : 'neutral'}>
              {enabled ? '已启用' : '未启用'}
            </Badge>
            <Badge tone="info">{source}</Badge>
            <Badge tone="neutral">v{version}</Badge>
          </div>
          {description && (
            <div style={{ color: theme.muted, fontSize: 12, marginTop: 4 }}>
              {description}
            </div>
          )}
        </div>
        <div style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
          {actions}
          <Button
            variant="ghost"
            title={open ? '收起详情' : '查看详情'}
            onClick={() => setOpen((v) => !v)}
            style={{ padding: 8 }}
          >
            {open ? <ChevronDown size={16} /> : <ChevronRight size={16} />}
          </Button>
        </div>
      </div>
      {open && (
        <StateView loading={detail.loading} error={detail.error}>
          {detail.data && (
            <div
              style={{
                marginTop: 10,
                border: `1px solid ${theme.border}`,
                borderRadius: 8,
                background: theme.surfaceAlt,
                padding: 12,
              }}
            >
              <div
                style={{
                  fontFamily: 'monospace',
                  fontSize: 12,
                  color: theme.faint,
                  wordBreak: 'break-all',
                  marginBottom: 8,
                }}
              >
                {detail.data.path}
              </div>
              {detail.data.resources.length > 0 && (
                <div
                  style={{
                    display: 'flex',
                    gap: 6,
                    flexWrap: 'wrap',
                    marginBottom: 8,
                  }}
                >
                  {detail.data.resources.map((res) => (
                    <Badge key={res} tone="neutral">
                      {res}
                    </Badge>
                  ))}
                </div>
              )}
              <pre
                style={{
                  margin: 0,
                  whiteSpace: 'pre-wrap',
                  wordBreak: 'break-word',
                  fontFamily: 'monospace',
                  fontSize: 12,
                  color: theme.muted,
                  maxHeight: 320,
                  overflow: 'auto',
                }}
              >
                {detail.data.content}
              </pre>
            </div>
          )}
        </StateView>
      )}
    </div>
  );
}
