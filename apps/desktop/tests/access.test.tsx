// @vitest-environment jsdom
import { act } from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockRejectedValue(new Error('unavailable')) }));
vi.mock('@tk/workspace-stream', () => ({ connectWorkspaceStream: () => ({ close() {} }), eventText: () => '' }));
import App from '../src/App';

const list = vi.fn();
const workspace = { id: 'private', name: 'Private workspace', tier: 'free', status: 'ready', createdAt: '' };
let root: Root;
let container: HTMLDivElement;

beforeEach(async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  Element.prototype.scrollIntoView = () => {};
  list.mockReset();
  list.mockResolvedValue(new Response(JSON.stringify([workspace])));
  vi.stubGlobal('fetch', vi.fn((url: string) => url.endsWith('/health')
    ? Promise.resolve(new Response(JSON.stringify({ ok: true }))) : list()));
  container = document.createElement('div');
  document.body.append(container);
  root = createRoot(container);
  await act(async () => { root.render(<App />); });
  await click('Cloud');
});
afterEach(async () => { await act(async () => root.unmount()); container.remove(); vi.unstubAllGlobals(); });

async function click(label: string) {
  const button = [...container.querySelectorAll('button')].find((item) => item.textContent === label);
  if (!button) throw new Error(`Missing button ${label}`);
  await act(async () => { button.click(); });
}

test.each([401, 403])('clears the workspace and distinguishes denial %i', async (status) => {
  expect(container.textContent).toContain('Private workspace');
  list.mockResolvedValueOnce(new Response('denied', { status }));
  await click('Refresh');
  expect(container.textContent).not.toContain('Private workspace');
  expect(container.textContent).toContain(status === 401 ? 'Authentication required (401)' : 'Access denied (403)');
});

test('a late list response cannot undo a denial', async () => {
  let resolve!: (value: Response) => void;
  list.mockImplementationOnce(() => new Promise<Response>((done) => { resolve = done; }));
  await click('Reload');
  list.mockResolvedValueOnce(new Response('denied', { status: 403 }));
  await click('Reload');
  await act(async () => resolve(new Response(JSON.stringify([workspace]))));
  expect(container.textContent).not.toContain('Private workspace');
  expect(container.textContent).toContain('Access denied (403)');
});

test('a 503 remains an unavailable error, not an authorization denial', async () => {
  list.mockResolvedValueOnce(new Response('unavailable', { status: 503 }));
  await click('Refresh');
  expect(container.textContent).toContain('HTTP 503');
  expect(container.textContent).not.toContain('Authentication required');
  expect(container.textContent).not.toContain('Access denied (403)');
});
