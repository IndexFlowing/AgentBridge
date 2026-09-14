// web/src/api/skills.ts
// Agent Runtime read models: Agent root, Rules, Skills and AgentContext.
import { request } from './client';
import type { AgentContext, AgentView, SkillDetail, SkillSummary } from './types';

export const skillsApi = {
  list: () => request<SkillSummary[]>('/api/skills'),
  detail: (name: string) =>
    request<SkillDetail>(`/api/skills/${encodeURIComponent(name)}`),
  enable: (name: string) =>
    request<null>(`/api/skills/${encodeURIComponent(name)}/enable`, {
      method: 'PUT',
    }),
  disable: (name: string) =>
    request<null>(`/api/skills/${encodeURIComponent(name)}/disable`, {
      method: 'PUT',
    }),
  remove: (name: string) =>
    request<null>(`/api/skills/${encodeURIComponent(name)}`, {
      method: 'DELETE',
    }),
  agent: () => request<AgentView>('/api/agent'),
  context: (project?: string, skills?: string[]) => {
    const params = new URLSearchParams();
    if (project) params.set('project', project);
    if (skills && skills.length > 0) params.set('skills', skills.join(','));
    const query = params.toString();
    return request<AgentContext>(`/api/agent/context${query ? `?${query}` : ''}`);
  },
};
