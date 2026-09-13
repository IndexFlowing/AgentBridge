// web/src/api/client.ts
// Unified HTTP client for the Rust Core management API.
// Every module-level API (dashboard/proxy/project/executor/task/system) goes
// through `request` so error handling and headers stay consistent.

const API_BASE =
  process.env.NODE_ENV === 'development' ? 'http://127.0.0.1:8030' : '';

export class ApiError extends Error {
  readonly status: number;
  readonly path: string;

  constructor(message: string, status: number, path: string) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.path = path;
  }
}

async function readErrorMessage(res: Response): Promise<string> {
  let msg = `HTTP ${res.status} ${res.statusText || ''}`.trim();
  try {
    const text = await res.text();
    if (!text) return msg;
    try {
      const parsed = JSON.parse(text);
      if (typeof parsed === 'string') return parsed;
      return parsed.message || parsed.error || msg;
    } catch {
      return text;
    }
  } catch {
    return msg;
  }
}

export async function request<T>(
  path: string,
  options?: RequestInit,
): Promise<T> {
  let res: Response;
  try {
    res = await fetch(`${API_BASE}${path}`, {
      ...options,
      headers: {
        'Content-Type': 'application/json',
        ...(options?.headers || {}),
      },
    });
  } catch (err) {
    const reason = err instanceof Error ? err.message : String(err);
    throw new ApiError(`无法连接 Rust Core: ${reason}`, 0, path);
  }

  if (!res.ok) {
    throw new ApiError(await readErrorMessage(res), res.status, path);
  }

  if (res.status === 204) return null as T;
  const text = await res.text();
  if (!text) return null as T;
  try {
    return JSON.parse(text) as T;
  } catch {
    return text as unknown as T;
  }
}
