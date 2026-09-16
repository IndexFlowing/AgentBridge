import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@testing-library/react';
import type { ProxyRecord } from '../api';
import { proxyApi } from '../api';
import { ProxyPage } from './ProxyPage';

vi.mock('../api', () => ({
  proxyApi: {
    list: vi.fn(),
    create: vi.fn(),
    update: vi.fn(),
    remove: vi.fn(),
    setEnabled: vi.fn(),
    verify: vi.fn(),
  },
}));

const proxyMock = proxyApi as unknown as {
  list: ReturnType<typeof vi.fn>;
  create: ReturnType<typeof vi.fn>;
  update: ReturnType<typeof vi.fn>;
  remove: ReturnType<typeof vi.fn>;
  setEnabled: ReturnType<typeof vi.fn>;
  verify: ReturnType<typeof vi.fn>;
};

const alpha: ProxyRecord = {
  id: 'p1',
  name: 'alpha',
  enabled: true,
  kind: 'http',
  host: '127.0.0.1',
  port: 7890,
  username_configured: false,
  password_configured: false,
  is_default: true,
  test_url: '',
  last_verified_at: null,
  last_verified_ok: null,
  last_verified_latency_ms: null,
};

beforeEach(() => {
  vi.clearAllMocks();
  proxyMock.list.mockResolvedValue([]);
  proxyMock.create.mockResolvedValue([]);
  proxyMock.update.mockResolvedValue([]);
  proxyMock.remove.mockResolvedValue([]);
  proxyMock.setEnabled.mockResolvedValue([]);
  proxyMock.verify.mockResolvedValue({
    success: true,
    latency_ms: 12,
    message: '代理连接成功',
    target: 'https://example.com',
  });
});

afterEach(() => {
  cleanup();
});

describe('ProxyPage', () => {
  it('renders persisted proxies with verification state', async () => {
    proxyMock.list.mockResolvedValue([
      { ...alpha, last_verified_at: '2026-01-01T00:00:00Z', last_verified_ok: true, last_verified_latency_ms: 20 },
    ]);
    render(<ProxyPage />);

    await screen.findByText('alpha');
    expect(screen.getByText('默认')).toBeTruthy();
    expect(screen.getByText(/验证成功/)).toBeTruthy();
  });

  it('creates a proxy with the entered connection settings', async () => {
    render(<ProxyPage />);
    await waitFor(() => expect(proxyMock.list).toHaveBeenCalled());

    fireEvent.change(screen.getByLabelText('代理名称'), {
      target: { value: '公司代理' },
    });
    fireEvent.change(screen.getByLabelText('代理主机'), {
      target: { value: 'proxy.corp.local' },
    });
    fireEvent.change(screen.getByLabelText('代理端口'), {
      target: { value: '8080' },
    });
    fireEvent.click(screen.getByRole('button', { name: '保存代理' }));

    await waitFor(() => {
      expect(proxyMock.create).toHaveBeenCalledWith(
        expect.objectContaining({
          name: '公司代理',
          host: 'proxy.corp.local',
          port: 8080,
          kind: 'http',
          enabled: true,
        }),
      );
    });
  });

  it('runs a real connectivity verification for a saved proxy', async () => {
    proxyMock.list.mockResolvedValue([alpha]);
    render(<ProxyPage />);

    await screen.findByText('alpha');
    fireEvent.click(screen.getByRole('button', { name: '连接验证-alpha' }));

    await waitFor(() => {
      expect(proxyMock.verify).toHaveBeenCalledWith('p1');
    });
    await screen.findByText(/验证成功/);
  });

  it('deletes a proxy after confirmation', async () => {
    proxyMock.list.mockResolvedValue([alpha]);
    const confirmSpy = vi.spyOn(window, 'confirm').mockReturnValue(true);
    render(<ProxyPage />);

    await screen.findByText('alpha');
    fireEvent.click(screen.getByRole('button', { name: '删除-alpha' }));

    await waitFor(() => {
      expect(proxyMock.remove).toHaveBeenCalledWith('p1');
    });
    confirmSpy.mockRestore();
  });
});
