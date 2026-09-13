// web/src/api/tasks.ts
import { request } from './client';
import type { TaskData } from './types';

export const tasksApi = {
  cancel: (projectName: string, taskId?: string) =>
    request<TaskData>(
      `/api/projects/${encodeURIComponent(projectName)}/tasks/cancel`,
      {
        method: 'POST',
        body: JSON.stringify({ task_id: taskId ?? null }),
      },
    ),
};
