import { ActivityIndicator, Pressable, StyleSheet, Text, View } from 'react-native';
import { colors } from '../theme';

type Props = {
  healthOk: boolean | null;
  cloudUrl: string;
  refreshing: boolean;
  onRefresh: () => void;
};

export function HealthCard({ healthOk, cloudUrl, refreshing, onRefresh }: Props) {
  const healthLabel =
    healthOk === null ? 'checking…' : healthOk ? 'ok' : 'down';
  const statusStyle =
    healthOk === true ? styles.ok : healthOk === false ? styles.bad : styles.muted;

  return (
    <View style={styles.card}>
      <View style={styles.cardRow}>
        <Text style={styles.cardTitle} accessibilityRole="header">
          Cloud health
        </Text>
        <Pressable
          style={styles.ghostBtn}
          onPress={onRefresh}
          disabled={refreshing}
          accessibilityRole="button"
          accessibilityLabel="Refresh cloud health and workspaces"
          accessibilityState={{ disabled: refreshing, busy: refreshing }}
          hitSlop={8}
        >
          <Text style={styles.ghostBtnText}>{refreshing ? '…' : 'Refresh'}</Text>
        </Pressable>
      </View>
      <Text
        style={styles.mono}
        accessibilityLabel={`Cloud health ${healthLabel}`}
        accessibilityLiveRegion="polite"
      >
        status: <Text style={statusStyle}>{healthLabel}</Text>
      </Text>
      <Text
        style={styles.monoMuted}
        accessibilityLabel={`tk-cloud base URL ${cloudUrl}`}
      >
        {cloudUrl}
      </Text>
      {healthOk === null ? (
        <ActivityIndicator
          color={colors.accent}
          style={styles.spinner}
          accessibilityLabel="Checking cloud health"
        />
      ) : null}
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
  cardRow: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
  },
  cardTitle: {
    color: colors.textSecondary,
    fontSize: 16,
    fontWeight: '600',
  },
  mono: {
    color: '#d1d5db',
    fontFamily: 'monospace',
    fontSize: 13,
  },
  monoMuted: {
    color: colors.textMuted,
    fontFamily: 'monospace',
    fontSize: 12,
  },
  ok: { color: colors.accent },
  bad: { color: colors.bad },
  muted: { color: colors.textMuted },
  ghostBtn: {
    paddingHorizontal: 10,
    paddingVertical: 6,
  },
  ghostBtnText: {
    color: colors.accent,
    fontWeight: '600',
    fontSize: 13,
  },
  spinner: { marginTop: 8 },
});
