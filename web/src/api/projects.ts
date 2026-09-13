// web/src/api/projects.ts
import { request } from './client';
import type { Project, ProjectInput } from './types';

export const projectsApi = {
  list: () => request<Project[]>('/api/projects'),
  save: (data: ProjectInput) =>
    request<Project[]>('/api/projects', {
      method: 'POST',
      body: JSON.stringify(data),
    }),
  remove: (id: string) =>
    request<Project[]>(`/api/projects/${encodeURIComponent(id)}`, {
      method: 'DELETE',
    }),
};
