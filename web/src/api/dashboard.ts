// web/src/api/dashboard.ts
import { request } from './client';
import type { DashboardData } from './types';

export const dashboardApi = {
  get: () => request<DashboardData>('/api/system/dashboard'),
};
