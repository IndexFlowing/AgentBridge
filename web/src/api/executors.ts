// web/src/api/executors.ts
import { request } from './client';
import type { Executor, ExecutorInput } from './types';

export const executorsApi = {
  list: () => request<Executor[]>('/api/executors'),
  available: () => request<string[]>('/api/executors/available'),
  save: (data: ExecutorInput) =>
    request<Executor[]>('/api/executors', {
      method: 'POST',
      body: JSON.stringify(data),
    }),
  remove: (id: string) =>
    request<Executor[]>(`/api/executors/${encodeURIComponent(id)}`, {
      method: 'DELETE',
    }),
  test: (id: string) =>
    request<Executor>(`/api/executors/${encodeURIComponent(id)}/test`, {
      method: 'POST',
    }),
};
