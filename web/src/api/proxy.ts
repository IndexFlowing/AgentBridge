// web/src/api/proxy.ts
import { request } from './client';
import type { ProxyData, ProxyInput } from './types';

export const proxyApi = {
  get: () => request<ProxyData>('/api/proxy'),
  save: (data: ProxyInput) =>
    request<ProxyData>('/api/proxy', {
      method: 'PUT',
      body: JSON.stringify(data),
    }),
  test: (data: ProxyInput) =>
    request<string>('/api/proxy/test', {
      method: 'POST',
      body: JSON.stringify(data),
    }),
};
