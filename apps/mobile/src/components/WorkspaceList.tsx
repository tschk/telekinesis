import { ActivityIndicator, StyleSheet, Text, View } from 'react-native';
import type { WorkspaceMeta } from '../api/tkCloud';
import { Panel } from './Panel';
import { StatusBadge, type BadgeTone } from './StatusBadge';
import { colors, fontMono, fontSans } from '../theme';

type Props = {
  workspaces: WorkspaceMeta[];
  loading: boolean;
  note: string | null;
  error: string | null;
};

function statusTone(status: string): BadgeTone {
  const s = status.toLowerCase();
  if (s.includes('ready') || s.includes('active') || s === 'ok') return 'ok';
  if (s.includes('pending') || s.includes('starting')) return 'warn';
  if (s.includes('error') || s.includes('fail')) return 'danger';
  return 'muted';
}

export function WorkspaceList({ workspaces, loading, note, error }: Props) {
  return (
    <Panel title="Workspaces" accessibilityLabel="Workspaces">
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
    </Panel>
  );
}

function WorkspaceBody({ workspaces, loading, note, error }: Props) {
  if (loading && workspaces.length === 0) {
    return (
      <View style={styles.empty} accessibilityLabel="Loading workspaces">
        <ActivityIndicator color={colors.tkAccent} />
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
    <View style={styles.list} accessibilityRole="list">
      {workspaces.map((w) => (
        <View
          key={w.id}
          style={styles.wsCard}
          accessibilityRole="summary"
          accessibilityLabel={`Workspace ${w.name || w.id}, tier ${w.tier}, status ${w.status}`}
        >
          <View style={styles.wsRow}>
            <Text style={styles.wsName}>{w.name || w.id}</Text>
            <StatusBadge tone={statusTone(w.status)} label={w.status} />
          </View>
          <View style={styles.wsMeta}>
            <Text style={styles.metaText}>{w.tier}</Text>
            <Text style={styles.sep}>·</Text>
            <Text style={styles.metaText}>
              {w.computerBackend ?? 'no backend'}
            </Text>
          </View>
          <Text style={styles.wsId}>{w.id}</Text>
        </View>
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  note: {
    ...fontSans,
    color: colors.tkWarn,
    fontSize: 12,
    marginBottom: colors.tkSpace1,
  },
  hint: {
    ...fontSans,
    color: colors.tkMuted,
    fontSize: 12,
  },
  empty: {
    paddingVertical: colors.tkSpace3,
    alignItems: 'flex-start',
    gap: colors.tkSpace2,
  },
  list: {
    gap: colors.tkSpace2,
  },
  wsCard: {
    backgroundColor: colors.tkBg,
    borderRadius: colors.tkRadiusMd,
    borderColor: colors.tkBorder,
    borderWidth: 1,
    padding: colors.tkSpace3,
    gap: colors.tkSpace1,
  },
  wsRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: colors.tkSpace2,
  },
  wsName: {
    ...fontSans,
    color: colors.tkText,
    fontSize: 14,
    fontWeight: '600',
    flexShrink: 1,
  },
  wsMeta: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: colors.tkSpace2,
    marginTop: 2,
  },
  metaText: {
    ...fontSans,
    color: colors.tkMuted,
    fontSize: 12,
  },
  sep: {
    ...fontSans,
    color: colors.tkMuted,
    opacity: 0.6,
    fontSize: 12,
  },
  wsId: {
    ...fontMono,
    color: colors.tkMuted,
    fontSize: 11,
    marginTop: colors.tkSpace1,
  },
});
