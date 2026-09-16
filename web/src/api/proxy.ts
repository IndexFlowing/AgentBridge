// web/src/api/proxy.ts
import { request } from './client';
import type {
  ProxyData,
  ProxyInput,
  ProxyRecord,
  ProxyRecordInput,
  ProxyVerifyResult,
} from './types';

export const proxyApi = {
  get: () => request<ProxyData>('/api/proxy'),
  save: (data: ProxyInput) =>
    request<ProxyData>('/api/proxy', {
      method: 'PUT',
      body: JSON.stringify(data),
    }),
  test: (data: ProxyInput) =>
    request<ProxyVerifyResult>('/api/proxy/test', {
      method: 'POST',
      body: JSON.stringify(data),
    }),

  list: () => request<ProxyRecord[]>('/api/proxies'),
  create: (data: ProxyRecordInput) =>
    request<ProxyRecord[]>('/api/proxies', {
      method: 'POST',
      body: JSON.stringify(data),
    }),
  update: (id: string, data: ProxyRecordInput) =>
    request<ProxyRecord[]>(`/api/proxies/${encodeURIComponent(id)}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    }),
  remove: (id: string) =>
    request<ProxyRecord[]>(`/api/proxies/${encodeURIComponent(id)}`, {
      method: 'DELETE',
    }),
  setEnabled: (id: string, enabled: boolean) =>
    request<ProxyRecord[]>(
      `/api/proxies/${encodeURIComponent(id)}/enabled`,
      {
        method: 'PUT',
        body: JSON.stringify({ enabled }),
      },
    ),
  verify: (id: string) =>
    request<ProxyVerifyResult>(
      `/api/proxies/${encodeURIComponent(id)}/test`,
      { method: 'POST' },
    ),
};
