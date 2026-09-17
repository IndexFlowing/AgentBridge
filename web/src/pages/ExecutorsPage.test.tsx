import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@testing-library/react';
import type { Executor, ProxyRecord } from '../api';
import { executorsApi, proxyApi } from '../api';
import { ExecutorsPage } from './ExecutorsPage';

vi.mock('../api', () => ({
  executorsApi: {
    list: vi.fn(),
    available: vi.fn(),
    save: vi.fn(),
    remove: vi.fn(),
    test: vi.fn(),
  },
  proxyApi: {
    list: vi.fn(),
    verify: vi.fn(),
  },
}));

const executorsMock = executorsApi as unknown as {
  list: ReturnType<typeof vi.fn>;
  save: ReturnType<typeof vi.fn>;
  remove: ReturnType<typeof vi.fn>;
  test: ReturnType<typeof vi.fn>;
};
const proxyMock = proxyApi as unknown as {
  list: ReturnType<typeof vi.fn>;
  verify: ReturnType<typeof vi.fn>;
};

const proxy: ProxyRecord = {
  id: 'p1',
  name: '公司代理',
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

function executor(proxyId?: string): Executor {
  return {
    id: 'builtin-antigravity',
    name: 'Antigravity',
    kind: 'antigravity',
    command: 'antigravity',
    proxy_id: proxyId,
    enabled: true,
    available: true,
    status: 'available',
    detected: true,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  executorsMock.list.mockResolvedValue([]);
  executorsMock.save.mockResolvedValue([]);
  proxyMock.list.mockResolvedValue([proxy]);
});

afterEach(() => {
  cleanup();
});

describe('ExecutorsPage proxy binding', () => {
  it('binds a proxy to an executor and persists it', async () => {
    executorsMock.list.mockResolvedValue([executor()]);
    render(<ExecutorsPage />);

    const select = (await screen.findByLabelText(
      '代理绑定-Antigravity',
    )) as HTMLSelectElement;
    expect(select.value).toBe('');

    fireEvent.change(select, { target: { value: 'p1' } });

    await waitFor(() => {
      expect(executorsMock.save).toHaveBeenCalledWith(
        expect.objectContaining({
          id: 'builtin-antigravity',
          proxy_id: 'p1',
        }),
      );
    });
  });

  it('echoes the persisted proxy binding', async () => {
    executorsMock.list.mockResolvedValue([executor('p1')]);
    render(<ExecutorsPage />);

    const select = (await screen.findByLabelText(
      '代理绑定-Antigravity',
    )) as HTMLSelectElement;
    expect(select.value).toBe('p1');
  });

  it('renders executor kind, executable path, and error diagnostics', async () => {
    executorsMock.list.mockResolvedValue([
      {
        ...executor(),
        executable: 'C:\\bin\\agy.cmd',
        version: '1.2.3',
        available: false,
        error: '找不到可执行文件',
      },
    ]);
    render(<ExecutorsPage />);

    expect(await screen.findByText('antigravity')).toBeDefined();
    expect(await screen.findByText('C:\\bin\\agy.cmd')).toBeDefined();
    expect(await screen.findByText('找不到可执行文件')).toBeDefined();
    expect(await screen.findByText(/\(1\.2\.3\)/)).toBeDefined();
  });

  it('triggers health check and displays result notice', async () => {
    executorsMock.list.mockResolvedValue([executor()]);
    executorsMock.test.mockResolvedValue({
      ...executor(),
      version: '1.0.0',
      available: true,
    });
    render(<ExecutorsPage />);

    const button = await screen.findByRole('button', { name: '健康检查' });
    fireEvent.click(button);

    await waitFor(() => {
      expect(executorsMock.test).toHaveBeenCalledWith('builtin-antigravity');
      expect(
        screen.getByText(/执行器「Antigravity」健康检查完成：可用 \(1.0.0\)/),
      ).toBeDefined();
    });
  });

  it('triggers proxy verification when bound and displays result notice', async () => {
    executorsMock.list.mockResolvedValue([executor('p1')]);
    proxyMock.verify.mockResolvedValue({
      success: true,
      latency_ms: 45,
      message: 'OK',
    });
    render(<ExecutorsPage />);

    const button = await screen.findByRole('button', { name: '验证代理' });
    fireEvent.click(button);

    await waitFor(() => {
      expect(proxyMock.verify).toHaveBeenCalledWith('p1');
      expect(
        screen.getByText(/代理「公司代理」连接验证成功：OK（45ms）/),
      ).toBeDefined();
    });
  });
});
