// web/src/pages/ProjectsPage.tsx
import { useEffect, useState } from 'react';
import type { FormEvent } from 'react';
import { Pencil, Trash2 } from 'lucide-react';
import type { Project } from '../api';
import { executorsApi, projectsApi } from '../api';
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

function mergeExecutor(current: string, available: string[]): string[] {
  if (!current || available.includes(current)) return available;
  return [current, ...available];
}

export function ProjectsPage() {
  const { data, loading, error, reload } = useAsync(() => projectsApi.list());
  const {
    data: availableExecutors,
    error: executorsError,
  } = useAsync(() => executorsApi.available());
  const [name, setName] = useState('');
  const [path, setPath] = useState('');
  const [executor, setExecutor] = useState('');
  const [saving, setSaving] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editExecutor, setEditExecutor] = useState('');
  const [savingEdit, setSavingEdit] = useState(false);

  const executorOptions = availableExecutors ?? [];

  useEffect(() => {
    if (!executor && executorOptions.length > 0) {
      setExecutor(executorOptions[0]);
    }
  }, [executor, executorOptions]);

  const handleAdd = async (e: FormEvent) => {
    e.preventDefault();
    setSaving(true);
    setActionError(null);
    try {
      await projectsApi.save({
        name: name.trim(),
        path: path.trim(),
        executor,
      });
      setName('');
      setPath('');
      reload();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  const startEdit = (project: Project) => {
    setActionError(null);
    setEditingId(project.id || project.name);
    setEditExecutor(project.executor);
  };

  const cancelEdit = () => {
    setEditingId(null);
    setEditExecutor('');
  };

  const handleSaveExecutor = async (project: Project) => {
    setSavingEdit(true);
    setActionError(null);
    try {
      await projectsApi.save({
        id: project.id,
        name: project.name,
        path: project.path,
        executor: editExecutor,
      });
      setEditingId(null);
      setEditExecutor('');
      reload();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    } finally {
      setSavingEdit(false);
    }
  };

  const handleDelete = async (project: Project) => {
    if (!window.confirm(`确定要移除项目「${project.name}」的挂载吗？(仅删除配置，不影响本地文件)`)) {
      return;
    }
    setActionError(null);
    try {
      await projectsApi.remove(project.id);
      reload();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : String(err));
    }
  };

  return (
    <div>
      <h1 style={{ fontSize: 22, margin: '0 0 4px' }}>项目</h1>
      <p style={{ color: theme.muted, fontSize: 13, margin: '0 0 20px' }}>
        管理已挂载到 AgentBridge 的本地工作区
      </p>

      <Card style={{ marginBottom: '1rem' }}>
        <h3 style={{ margin: '0 0 1rem', color: theme.accent, fontSize: 15 }}>
          + 挂载新项目
        </h3>
        <form
          onSubmit={handleAdd}
          style={{ display: 'flex', gap: '1rem', alignItems: 'flex-end', flexWrap: 'wrap' }}
        >
          <div style={{ flex: '1 1 200px' }}>
            <Label>项目名称 (英文或数字)</Label>
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="例如: agentbridge-core"
              required
            />
          </div>
          <div style={{ flex: '2 1 320px' }}>
            <Label>本地绝对路径</Label>
            <Input
              value={path}
              onChange={(e) => setPath(e.target.value)}
              placeholder="例如: D:\\Projects\\AgentBridge"
              required
            />
          </div>
          <div style={{ flex: '1 1 180px' }}>
            <Label>执行器</Label>
            <Select
              aria-label="执行器"
              value={executor}
              onChange={(e) => setExecutor(e.target.value)}
              disabled={saving || executorOptions.length === 0}
              required
            >
              {executorOptions.length === 0 && (
                <option value="" disabled>
                  暂无可用执行器
                </option>
              )}
              {executorOptions.map((ex) => (
                <option key={ex} value={ex}>
                  {ex}
                </option>
              ))}
            </Select>
          </div>
          <Button type="submit" disabled={saving || !executor}>
            {saving ? '挂载中...' : '挂载项目'}
          </Button>
        </form>
        {actionError && <InlineError message={actionError} />}
        {executorsError && (
          <InlineError message={`加载执行器列表失败：${executorsError}`} />
        )}
      </Card>

      <StateView
        loading={loading}
        error={error}
        onRetry={reload}
        empty={!!data && data.length === 0}
        emptyTitle="暂无项目"
        emptyHint="使用上方表单挂载第一个本地工作区。"
      >
        <div style={{ display: 'flex', flexDirection: 'column', gap: '1rem' }}>
          {data?.map((project) => {
            const key = project.id || project.name;
            const isEditing = editingId === key;
            const editOptions = mergeExecutor(
              project.executor,
              executorOptions,
            );
            return (
              <Card key={key} active={project.active}>
                <div
                  style={{
                    display: 'flex',
                    justifyContent: 'space-between',
                    gap: 12,
                    alignItems: 'flex-start',
                  }}
                >
                  <div style={{ minWidth: 0 }}>
                    <div
                      style={{
                        display: 'flex',
                        alignItems: 'center',
                        gap: 8,
                        fontSize: 16,
                        fontWeight: 700,
                      }}
                    >
                      {project.name}
                      {project.active && <Badge tone="accent">当前激活</Badge>}
                      {project.readonly && <Badge tone="neutral">只读</Badge>}
                    </div>
                    <div
                      style={{
                        color: theme.muted,
                        fontFamily: 'monospace',
                        fontSize: 12,
                        marginTop: 8,
                        wordBreak: 'break-all',
                      }}
                    >
                      {project.path}
                    </div>
                    <div style={{ display: 'flex', gap: 6, marginTop: 10, flexWrap: 'wrap' }}>
                      {!isEditing && (
                        <Badge tone="neutral">{project.executor}</Badge>
                      )}
                      {project.project_type.map((type) => (
                        <Badge key={type} tone="info">
                          {type}
                        </Badge>
                      ))}
                      {project.git_repository && <Badge tone="neutral">Git</Badge>}
                    </div>
                    {isEditing && (
                      <div
                        style={{
                          display: 'flex',
                          gap: 8,
                          alignItems: 'center',
                          marginTop: 12,
                        }}
                      >
                        <Select
                          aria-label={`执行器-${project.name}`}
                          value={editExecutor}
                          onChange={(e) => setEditExecutor(e.target.value)}
                          disabled={savingEdit || editOptions.length === 0}
                          style={{ maxWidth: 220 }}
                        >
                          {editOptions.map((ex) => (
                            <option key={ex} value={ex}>
                              {ex}
                            </option>
                          ))}
                        </Select>
                        <Button
                          onClick={() => handleSaveExecutor(project)}
                          disabled={savingEdit || !editExecutor}
                        >
                          {savingEdit ? '保存中...' : '保存'}
                        </Button>
                        <Button
                          variant="secondary"
                          onClick={cancelEdit}
                          disabled={savingEdit}
                        >
                          取消
                        </Button>
                      </div>
                    )}
                  </div>
                  <div style={{ display: 'flex', gap: 4, alignItems: 'center' }}>
                    <Button
                      variant="ghost"
                      onClick={() => startEdit(project)}
                      disabled={isEditing}
                      title="修改执行器"
                      aria-label={`修改执行器-${project.name}`}
                      style={{ padding: 8 }}
                    >
                      <Pencil size={18} />
                    </Button>
                    <Button
                      variant="ghost"
                      onClick={() => handleDelete(project)}
                      title="移除挂载"
                      style={{ color: theme.danger, padding: 8 }}
                    >
                      <Trash2 size={18} />
                    </Button>
                  </div>
                </div>
              </Card>
            );
          })}
        </div>
      </StateView>
    </div>
  );
}
