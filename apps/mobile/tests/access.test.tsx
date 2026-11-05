import { afterEach, beforeEach, describe, expect, mock, test } from 'bun:test';
import React from 'react';
import { act, create, type ReactTestRenderer } from 'react-test-renderer';

const list = mock(async (): Promise<{ workspaces: typeof workspace[] }> => ({ workspaces: [] }));
const createWorkspace = mock(async () => workspace);
class TkCloudError extends Error {
  code = 'http';
  constructor(public status: number) { super(`HTTP ${status}`); }
}
mock.module('react-native', () => ({
  ActivityIndicator: 'ActivityIndicator', RefreshControl: 'RefreshControl',
  SafeAreaView: 'SafeAreaView', ScrollView: 'ScrollView', Text: 'Text', View: 'View',
  Pressable: 'Pressable', TextInput: 'TextInput',
  StyleSheet: { create: (styles: unknown) => styles },
}));
mock.module('@expo-google-fonts/chivo-mono', () => ({
  ChivoMono_400Regular: 1, ChivoMono_600SemiBold: 2, ChivoMono_700Bold: 3,
}));
mock.module('expo-font', () => ({ useFonts: () => [true] }));
mock.module('expo-status-bar', () => ({ StatusBar: 'StatusBar' }));
mock.module('../src/components/FutureTabs', () => ({ FutureTabs: 'FutureTabs' }));
mock.module('../src/api/tkCloud', () => ({
  baseUrl: () => 'https://example.test', createWorkspace,
  getHealth: async () => ({ ok: true }), listWorkspaces: list,
  isTkCloudError: (err: unknown) => err instanceof TkCloudError,
  isAbortError: () => false,
  formatTkCloudError: (err: Error) => err.message,
  TkCloudError,
}));
const { default: App } = await import('../App');
const workspace = { id: 'private', name: 'Private workspace', tier: 'free', status: 'ready' };
let app: ReactTestRenderer;

beforeEach(async () => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  list.mockReset();
  createWorkspace.mockReset();
  list.mockResolvedValue({ workspaces: [workspace] });
  await act(async () => { app = create(<App />); });
});
afterEach(async () => { await act(async () => app.unmount()); });

function visible() {
  return app.root.findAll((node) => String(node.type) === 'Text').flatMap((node) => node.children.filter((child) => typeof child === 'string')).join(' ');
}
async function refresh() {
  const button = app.root.findByProps({ accessibilityLabel: 'Refresh cloud health and workspaces' });
  await act(async () => { button.props.onPress(); });
}

describe('workspace authorization', () => {
  for (const status of [401, 403]) {
    test(`clears the selected workspace on ${status}`, async () => {
      expect(visible()).toContain('Private workspace');
      list.mockRejectedValueOnce(new TkCloudError(status));
      await refresh();
      expect(visible()).not.toContain('Private workspace');
      expect(visible()).toContain(status === 401 ? 'Authentication required (401)' : 'Access denied (403)');
      expect(app.root.find((node) => String(node.type) === 'FutureTabs').props.workspaceId).toBeNull();
    });
  }
  test('retains workspaces on a transient failure', async () => {
    list.mockRejectedValueOnce(new TkCloudError(503));
    await refresh();
    expect(visible()).toContain('Private workspace');
    expect(visible()).toContain('HTTP 503');
  });
  test('ignores an older refresh after an authorization failure', async () => {
    let resolve!: (value: { workspaces: typeof workspace[] }) => void;
    list.mockImplementationOnce(() => new Promise((done) => { resolve = done; }));
    await refresh();
    list.mockRejectedValueOnce(new TkCloudError(403));
    await refresh();
    await act(async () => { resolve({ workspaces: [workspace] }); });
    expect(visible()).not.toContain('Private workspace');
    expect(visible()).toContain('Access denied (403)');
  });
});
