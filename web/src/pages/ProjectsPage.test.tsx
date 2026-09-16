import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@testing-library/react';
import type { Project } from '../api';
import { executorsApi, projectsApi } from '../api';
import { ProjectsPage } from './ProjectsPage';

vi.mock('../api', () => ({
  projectsApi: { list: vi.fn(), save: vi.fn(), remove: vi.fn() },
  executorsApi: { available: vi.fn() },
}));

const projectsMock = projectsApi as unknown as {
  list: ReturnType<typeof vi.fn>;
  save: ReturnType<typeof vi.fn>;
  remove: ReturnType<typeof vi.fn>;
};
const executorsMock = executorsApi as unknown as {
  available: ReturnType<typeof vi.fn>;
};

const alpha: Project = {
  id: 'p1',
  name: 'alpha',
  path: 'D:\\workspaces\\alpha',
  description: '',
  readonly: false,
  active: true,
  git_repository: false,
  project_type: [],
  executor: 'opencode',
};

beforeEach(() => {
  vi.clearAllMocks();
  projectsMock.list.mockResolvedValue([]);
  projectsMock.save.mockResolvedValue([]);
  projectsMock.remove.mockResolvedValue([]);
  executorsMock.available.mockResolvedValue(['opencode', 'antigravity']);
});

afterEach(() => {
  cleanup();
});

async function createSelect() {
  return (await screen.findByLabelText('执行器')) as HTMLSelectElement;
}

describe('ProjectsPage executor configuration', () => {
  it('loads available executors dynamically into the create form', async () => {
    render(<ProjectsPage />);

    const select = await createSelect();
    await waitFor(() => {
      expect(screen.getByRole('option', { name: 'opencode' })).toBeTruthy();
      expect(screen.getByRole('option', { name: 'antigravity' })).toBeTruthy();
    });
    expect(select.value).toBe('opencode');
  });

  it('submits the selected executor when creating a project', async () => {
    render(<ProjectsPage />);

    const select = await createSelect();
    await waitFor(() =>
      expect(screen.getByRole('option', { name: 'antigravity' })).toBeTruthy(),
    );
    fireEvent.change(select, { target: { value: 'antigravity' } });
    fireEvent.change(screen.getByPlaceholderText('例如: agentbridge-core'), {
      target: { value: 'beta' },
    });
    fireEvent.change(
      screen.getByPlaceholderText('例如: D:\\\\Projects\\\\AgentBridge'),
      { target: { value: 'D:\\workspaces\\beta' } },
    );
    fireEvent.click(screen.getByRole('button', { name: '挂载项目' }));

    await waitFor(() => {
      expect(projectsMock.save).toHaveBeenCalledWith({
        name: 'beta',
        path: 'D:\\workspaces\\beta',
        executor: 'antigravity',
      });
    });
  });

  it('edits an existing project and persists the new executor', async () => {
    projectsMock.list.mockResolvedValue([alpha]);
    render(<ProjectsPage />);

    await screen.findByText('alpha');
    fireEvent.click(screen.getByRole('button', { name: '修改执行器-alpha' }));

    const editSelect = (await screen.findByLabelText(
      '执行器-alpha',
    )) as HTMLSelectElement;
    fireEvent.change(editSelect, { target: { value: 'antigravity' } });
    fireEvent.click(screen.getByRole('button', { name: '保存' }));

    await waitFor(() => {
      expect(projectsMock.save).toHaveBeenCalledWith({
        id: 'p1',
        name: 'alpha',
        path: 'D:\\workspaces\\alpha',
        executor: 'antigravity',
      });
    });
  });
});
