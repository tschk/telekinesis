import { ActivityIndicator, StyleSheet, Text, View } from 'react-native';
import type { WorkspaceMeta } from '../api/tkCloud';
import { colors } from '../theme';

type Props = {
  workspaces: WorkspaceMeta[];
  loading: boolean;
  note: string | null;
  error: string | null;
};

export function WorkspaceList({ workspaces, loading, note, error }: Props) {
  return (
    <View
      style={styles.card}
      accessibilityLabel="Workspaces"
    >
      <Text style={styles.cardTitle} accessibilityRole="header">
        Workspaces
      </Text>
      {note ? (
        <Text
          style={styles.note}
          accessibilityRole="text"
          accessibilityLiveRegion="polite"
        >
          {note}
        </Text>
      ) : null}
      <WorkspaceBody
        workspaces={workspaces}
        loading={loading}
        note={note}
        error={error}
      />
    </View>
  );
}

function WorkspaceBody({ workspaces, loading, note, error }: Props) {
  if (loading && workspaces.length === 0) {
    return (
      <View style={styles.empty} accessibilityLabel="Loading workspaces">
        <ActivityIndicator color={colors.accent} />
        <Text style={styles.hint}>Loading workspaces…</Text>
      </View>
    );
  }

  if (workspaces.length === 0) {
    let emptyCopy =
      'No workspaces yet. Create one above, or point EXPO_PUBLIC_TK_CLOUD_URL at a running tk-cloud.';
    if (error) {
      emptyCopy = 'Could not load workspaces. Retry from the error banner or pull to refresh.';
    } else if (note) {
      emptyCopy =
        'Create a workspace above — it stays in this session until GET /v1/workspaces exists.';
    }
    return (
      <Text style={styles.hint} accessibilityLabel={emptyCopy}>
        {emptyCopy}
      </Text>
    );
  }

  return (
    <View accessibilityRole="list">
      {workspaces.map((w) => (
        <View
          key={w.id}
          style={styles.wsCard}
          accessibilityRole="summary"
          accessibilityLabel={`Workspace ${w.name || w.id}, tier ${w.tier}, status ${w.status}`}
        >
          <Text style={styles.wsName}>{w.name || w.id}</Text>
          <Text style={styles.monoMuted}>id: {w.id}</Text>
          <Text style={styles.monoMuted}>
            tier: {w.tier} · status: {w.status}
          </Text>
          {w.computerBackend ? (
            <Text style={styles.monoMuted}>backend: {w.computerBackend}</Text>
          ) : null}
        </View>
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  card: {
    backgroundColor: colors.card,
    borderRadius: 12,
    borderColor: colors.cardBorder,
    borderWidth: 1,
    padding: 14,
    gap: 8,
  },
  cardTitle: {
    color: colors.textSecondary,
    fontSize: 16,
    fontWeight: '600',
  },
  note: {
    color: colors.warning,
    fontSize: 12,
    marginBottom: 4,
  },
  hint: {
    color: colors.hint,
    fontSize: 12,
  },
  empty: {
    paddingVertical: 12,
    alignItems: 'flex-start',
    gap: 8,
  },
  wsCard: {
    backgroundColor: colors.wsBg,
    borderRadius: 8,
    borderColor: colors.wsBorder,
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
  monoMuted: {
    color: colors.textMuted,
    fontFamily: 'monospace',
    fontSize: 12,
  },
});
