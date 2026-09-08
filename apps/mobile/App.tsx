import { StatusBar } from 'expo-status-bar';
import { useCallback, useEffect, useState } from 'react';
import { RefreshControl, SafeAreaView, ScrollView, StyleSheet, Text, View } from 'react-native';
import {
  baseUrl,
  createWorkspace,
  formatTkCloudError,
  getHealth,
  isAbortError,
  listWorkspaces,
  type WorkspaceMeta,
} from './src/api/tkCloud';
import { CreateWorkspaceCard } from './src/components/CreateWorkspaceCard';
import { ErrorBanner } from './src/components/ErrorBanner';
import { FutureTabs } from './src/components/FutureTabs';
import { HealthCard } from './src/components/HealthCard';
import { WorkspaceList } from './src/components/WorkspaceList';
import { colors } from './src/theme';

export default function App() {
  const [healthOk, setHealthOk] = useState<boolean | null>(null);
  const [workspaces, setWorkspaces] = useState<WorkspaceMeta[]>([]);
  const [listNote, setListNote] = useState<string | null>(null);
  const [createName, setCreateName] = useState('');
  const [creating, setCreating] = useState(false);
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async (opts?: { pull?: boolean; signal?: AbortSignal }) => {
    setBusy(true);
    if (opts?.pull) setRefreshing(true);
    setError(null);
    const signal = opts?.signal;

    const [healthResult, listResult] = await Promise.allSettled([
      getHealth({ signal }),
      listWorkspaces({ signal }),
    ]);

    if (signal?.aborted) {
      setRefreshing(false);
      setBusy(false);
      setLoading(false);
      return;
    }

    if (healthResult.status === 'fulfilled') {
      setHealthOk(healthResult.value.ok);
    } else if (!isAbortError(healthResult.reason)) {
      setHealthOk(false);
    }

    if (listResult.status === 'fulfilled') {
      const result = listResult.value;
      setListNote(result.note ?? null);
      setWorkspaces((prev) => {
        // Missing list endpoint: keep session-created workspaces instead of wiping.
        if (result.note && result.workspaces.length === 0) return prev;
        return result.workspaces;
      });
    } else if (!isAbortError(listResult.reason)) {
      setListNote(null);
      setError(formatTkCloudError(listResult.reason));
    }

    setRefreshing(false);
    setBusy(false);
    setLoading(false);
  }, []);

  useEffect(() => {
    const ac = new AbortController();
    void refresh({ signal: ac.signal });
    return () => ac.abort();
  }, [refresh]);

  async function handleCreate() {
    if (creating) return;
    setCreating(true);
    setError(null);
    try {
      const meta = await createWorkspace(
        createName.trim() ? { name: createName.trim() } : undefined,
      );
      setCreateName('');
      setWorkspaces((prev) => [meta, ...prev.filter((w) => w.id !== meta.id)]);
      await refresh();
    } catch (err) {
      if (!isAbortError(err)) setError(formatTkCloudError(err));
    } finally {
      setCreating(false);
    }
  }

  return (
    <SafeAreaView style={styles.safe}>
      <StatusBar style="light" />
      <ScrollView
        contentContainerStyle={styles.scroll}
        keyboardShouldPersistTaps="handled"
        accessibilityLabel="Telekinesis mobile companion"
        refreshControl={
          <RefreshControl
            refreshing={refreshing}
            onRefresh={() => void refresh({ pull: true })}
            tintColor={colors.accent}
            colors={[colors.accent]}
            progressBackgroundColor={colors.card}
          />
        }
      >
        <View style={styles.header}>
          <Text style={styles.title} accessibilityRole="header">
            Telekinesis
          </Text>
          <Text style={styles.subtitle}>mobile companion M0</Text>
        </View>

        <FutureTabs />

        {error ? (
          <ErrorBanner message={error} onRetry={() => void refresh()} />
        ) : null}

        <HealthCard
          healthOk={healthOk}
          cloudUrl={baseUrl()}
          refreshing={busy}
          onRefresh={() => void refresh()}
        />

        <CreateWorkspaceCard
          name={createName}
          onChangeName={setCreateName}
          creating={creating}
          onCreate={() => void handleCreate()}
        />

        <WorkspaceList
          workspaces={workspaces}
          loading={loading}
          note={listNote}
          error={error}
        />
      </ScrollView>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  safe: {
    flex: 1,
    backgroundColor: colors.bg,
  },
  scroll: {
    padding: 16,
    paddingBottom: 48,
    gap: 12,
  },
  header: {
    marginBottom: 4,
  },
  title: {
    color: colors.text,
    fontSize: 28,
    fontWeight: '700',
  },
  subtitle: {
    color: colors.textMuted,
    fontSize: 14,
    marginTop: 2,
  },
});
