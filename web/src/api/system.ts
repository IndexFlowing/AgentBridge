// web/src/api/system.ts
import { request } from './client';
import type { ConnectionData, ConnectionInput } from './types';

export const systemApi = {
  getConnection: () => request<ConnectionData>('/api/system/connection'),
  saveConnection: (data: ConnectionInput) =>
    request<ConnectionData>('/api/system/connection', {
      method: 'PUT',
      body: JSON.stringify(data),
    }),
};
