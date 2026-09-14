// web/src/api/index.ts
// Module boundaries for the Web API client.
//
// Implemented (backed by src/api/): dashboard, system, proxy, projects,
// executors, tasks.
//
// Reserved for future P1 modules (NO backend contract exists yet, so no
// functions are exported for them): provider, skills, multi-user, cloud.
export { ApiError, request } from './client';
export * from './types';

export { dashboardApi } from './dashboard';
export { systemApi } from './system';
export { proxyApi } from './proxy';
export { projectsApi } from './projects';
export { executorsApi } from './executors';
export { tasksApi } from './tasks';
export { skillsApi } from './skills';
