import {
  ChivoMono_400Regular,
  ChivoMono_600SemiBold,
  ChivoMono_700Bold,
} from '@expo-google-fonts/chivo-mono';
import { useFonts } from 'expo-font';
import { StatusBar } from 'expo-status-bar';
import { useCallback, useEffect, useState } from 'react';
import {
  ActivityIndicator,
  RefreshControl,
  SafeAreaView,
  ScrollView,
  StyleSheet,
  Text,
  View,
} from 'react-native';
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
import { StatusBadge, type BadgeTone } from './src/components/StatusBadge';
import { WorkspaceList } from './src/components/WorkspaceList';
import { colors, fontSans, tkShadowSm } from './src/theme';

export default function App() {
  const [fontsLoaded] = useFonts({
    ChivoMono: ChivoMono_400Regular,
    ChivoMono_400Regular,
    ChivoMono_600SemiBold,
    ChivoMono_700Bold,
  });

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

  const apiTone: BadgeTone =
    healthOk === null ? 'muted' : healthOk ? 'ok' : 'danger';
  const apiLabel =
    healthOk === null ? 'API …' : healthOk ? 'API healthy' : 'API down';

  if (!fontsLoaded) {
    return (
      <SafeAreaView style={[styles.safe, styles.boot]}>
        <StatusBar style="light" />
        <ActivityIndicator color={colors.tkAccent} />
      </SafeAreaView>
    );
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
            tintColor={colors.tkAccent}
            colors={[colors.tkAccent]}
            progressBackgroundColor={colors.tkSurface}
          />
        }
      >
        <View style={styles.header}>
          <View style={styles.brand}>
            <View style={styles.mark} accessibilityElementsHidden />
            <View style={styles.brandText}>
              <Text style={styles.title} accessibilityRole="header">
                Telekinesis
              </Text>
              <Text style={styles.subtitle}>mobile companion</Text>
            </View>
          </View>
          <StatusBadge tone={apiTone} label={apiLabel} />
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
    backgroundColor: colors.tkBg,
  },
  boot: {
    alignItems: 'center',
    justifyContent: 'center',
  },
  scroll: {
    padding: colors.tkSpace4,
    paddingBottom: colors.tkSpace6,
    gap: colors.tkSpace3,
  },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: colors.tkSpace3,
    backgroundColor: colors.tkSurface,
    borderColor: colors.tkBorder,
    borderWidth: 1,
    borderRadius: colors.tkRadiusLg,
    paddingHorizontal: colors.tkSpace4,
    paddingVertical: colors.tkSpace3,
    ...tkShadowSm,
  },
  brand: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: colors.tkSpace2,
    flexShrink: 1,
  },
  mark: {
    width: 10,
    height: 10,
    borderRadius: 999,
    backgroundColor: colors.tkAccent,
    shadowColor: colors.tkAccent,
    shadowOffset: { width: 0, height: 0 },
    shadowOpacity: 0.35,
    shadowRadius: 4,
  },
  brandText: {
    flexShrink: 1,
  },
  title: {
    ...fontSans,
    color: colors.tkText,
    fontSize: 16,
    fontWeight: '600',
    letterSpacing: 0.2,
  },
  subtitle: {
    ...fontSans,
    color: colors.tkMuted,
    fontSize: 11,
    letterSpacing: 0.6,
    textTransform: 'uppercase',
    marginTop: 1,
  },
});
