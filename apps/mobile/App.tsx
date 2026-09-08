import { StatusBar } from 'expo-status-bar';
import { useCallback, useEffect, useState } from 'react';
import {
  ActivityIndicator,
  Pressable,
  SafeAreaView,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';
import {
  baseUrl,
  createWorkspace,
  getHealth,
  listWorkspaces,
  type WorkspaceMeta,
} from './src/api/tkCloud';

const FUTURE_TABS = ['Status', 'Steer', 'Diffs', 'Approvals'] as const;

export default function App() {
  const [healthOk, setHealthOk] = useState<boolean | null>(null);
  const [workspaces, setWorkspaces] = useState<WorkspaceMeta[]>([]);
  const [listNote, setListNote] = useState<string | null>(null);
  const [createName, setCreateName] = useState('');
  const [creating, setCreating] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setRefreshing(true);
    setError(null);
    try {
      await getHealth();
      setHealthOk(true);
    } catch {
      setHealthOk(false);
    }
    try {
      const result = await listWorkspaces();
      setWorkspaces(result.workspaces);
      setListNote(result.note ?? null);
    } catch (err) {
      setWorkspaces([]);
      setListNote(null);
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setRefreshing(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function handleCreate() {
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
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setCreating(false);
    }
  }

  const healthLabel =
    healthOk === null ? 'checking…' : healthOk ? 'ok' : 'down';

  return (
    <SafeAreaView style={styles.safe}>
      <StatusBar style="light" />
      <ScrollView contentContainerStyle={styles.scroll} keyboardShouldPersistTaps="handled">
        <View style={styles.header}>
          <Text style={styles.title}>Telekinesis</Text>
          <Text style={styles.subtitle}>mobile companion M0</Text>
        </View>

        <View style={styles.tabs}>
          {FUTURE_TABS.map((label) => (
            <View key={label} style={styles.tabChip}>
              <Text style={styles.tabText}>{label}</Text>
            </View>
          ))}
        </View>
        <Text style={styles.hint}>Future tabs — Status / Steer / Diffs / Approvals (not wired yet)</Text>

        {error ? (
          <View style={styles.errorBanner}>
            <Text style={styles.errorText}>{error}</Text>
          </View>
        ) : null}

        <View style={styles.card}>
          <View style={styles.cardRow}>
            <Text style={styles.cardTitle}>Cloud health</Text>
            <Pressable style={styles.ghostBtn} onPress={() => void refresh()} disabled={refreshing}>
              <Text style={styles.ghostBtnText}>{refreshing ? '…' : 'Refresh'}</Text>
            </Pressable>
          </View>
          <Text style={styles.mono}>
            status:{' '}
            <Text style={healthOk ? styles.ok : healthOk === false ? styles.bad : styles.muted}>
              {healthLabel}
            </Text>
          </Text>
          <Text style={styles.monoMuted}>{baseUrl()}</Text>
          {refreshing ? <ActivityIndicator color="#5eead4" style={{ marginTop: 8 }} /> : null}
        </View>

        <View style={styles.card}>
          <Text style={styles.cardTitle}>Create workspace</Text>
          <TextInput
            style={styles.input}
            placeholder="Name (optional)"
            placeholderTextColor="#6b7280"
            value={createName}
            onChangeText={setCreateName}
            editable={!creating}
            autoCapitalize="none"
            autoCorrect={false}
          />
          <Pressable
            style={[styles.primaryBtn, creating && styles.btnDisabled]}
            onPress={() => void handleCreate()}
            disabled={creating}
          >
            <Text style={styles.primaryBtnText}>{creating ? 'Creating…' : 'Create workspace'}</Text>
          </Pressable>
        </View>

        <View style={styles.card}>
          <Text style={styles.cardTitle}>Workspaces</Text>
          {listNote ? <Text style={styles.note}>{listNote}</Text> : null}
          {workspaces.length === 0 ? (
            <Text style={styles.hint}>No workspaces yet. Create one or point EXPO_PUBLIC_TK_CLOUD_URL at a running tk-cloud.</Text>
          ) : (
            workspaces.map((w) => (
              <View key={w.id} style={styles.wsCard}>
                <Text style={styles.wsName}>{w.name || w.id}</Text>
                <Text style={styles.monoMuted}>id: {w.id}</Text>
                <Text style={styles.monoMuted}>
                  tier: {w.tier} · status: {w.status}
                </Text>
                {w.computerBackend ? (
                  <Text style={styles.monoMuted}>backend: {w.computerBackend}</Text>
                ) : null}
              </View>
            ))
          )}
        </View>
      </ScrollView>
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  safe: {
    flex: 1,
    backgroundColor: '#0b0f14',
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
    color: '#f3f4f6',
    fontSize: 28,
    fontWeight: '700',
  },
  subtitle: {
    color: '#9ca3af',
    fontSize: 14,
    marginTop: 2,
  },
  tabs: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: 8,
  },
  tabChip: {
    backgroundColor: '#1a2230',
    borderColor: '#2a3548',
    borderWidth: 1,
    borderRadius: 999,
    paddingHorizontal: 12,
    paddingVertical: 6,
    opacity: 0.7,
  },
  tabText: {
    color: '#94a3b8',
    fontSize: 12,
    fontWeight: '600',
  },
  hint: {
    color: '#6b7280',
    fontSize: 12,
  },
  note: {
    color: '#fbbf24',
    fontSize: 12,
    marginBottom: 8,
  },
  errorBanner: {
    backgroundColor: '#3f1d1d',
    borderColor: '#7f1d1d',
    borderWidth: 1,
    borderRadius: 10,
    padding: 12,
  },
  errorText: {
    color: '#fecaca',
    fontSize: 13,
  },
  card: {
    backgroundColor: '#151b24',
    borderRadius: 12,
    borderColor: '#243044',
    borderWidth: 1,
    padding: 14,
    gap: 8,
  },
  cardRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
  },
  cardTitle: {
    color: '#e5e7eb',
    fontSize: 16,
    fontWeight: '600',
  },
  mono: {
    color: '#d1d5db',
    fontFamily: 'monospace',
    fontSize: 13,
  },
  monoMuted: {
    color: '#9ca3af',
    fontFamily: 'monospace',
    fontSize: 12,
  },
  ok: { color: '#5eead4' },
  bad: { color: '#f87171' },
  muted: { color: '#9ca3af' },
  input: {
    backgroundColor: '#0b0f14',
    borderColor: '#2a3548',
    borderWidth: 1,
    borderRadius: 8,
    color: '#f3f4f6',
    paddingHorizontal: 12,
    paddingVertical: 10,
    fontSize: 15,
  },
  primaryBtn: {
    backgroundColor: '#0d9488',
    borderRadius: 8,
    paddingVertical: 12,
    alignItems: 'center',
  },
  btnDisabled: { opacity: 0.6 },
  primaryBtnText: {
    color: '#ecfeff',
    fontWeight: '700',
    fontSize: 15,
  },
  ghostBtn: {
    paddingHorizontal: 10,
    paddingVertical: 6,
  },
  ghostBtnText: {
    color: '#5eead4',
    fontWeight: '600',
    fontSize: 13,
  },
  wsCard: {
    backgroundColor: '#0f141c',
    borderRadius: 8,
    borderColor: '#1f2937',
    borderWidth: 1,
    padding: 10,
    gap: 2,
    marginTop: 4,
  },
  wsName: {
    color: '#f9fafb',
    fontSize: 15,
    fontWeight: '600',
  },
});
