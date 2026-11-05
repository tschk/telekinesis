import { afterEach, expect, test, vi } from 'vitest';
import { createWorkspace, execInWorkspace, getHealth, getWorkspace, listWorkspaces } from '../src/lib/cloudApi';

afterEach(() => vi.unstubAllGlobals());

test.each([401, 403, 503])('preserves HTTP %i from every cloud operation', async (status) => {
  vi.stubGlobal('fetch', vi.fn(async () => new Response('denied', { status })));
  for (const result of await Promise.all([
    getHealth(), listWorkspaces(), createWorkspace(), getWorkspace('id'),
    execInWorkspace('id', { source: 'echo test' }),
  ])) {
    expect(result).toMatchObject({ status });
  }
});
