import { ActivityIndicator, Pressable, StyleSheet, Text, View } from 'react-native';
import { Panel } from './Panel';
import { StatusBadge, type BadgeTone } from './StatusBadge';
import { colors, fontMono, fontSans } from '../theme';

type Props = {
  healthOk: boolean | null;
  cloudUrl: string;
  refreshing: boolean;
  onRefresh: () => void;
};

export function HealthCard({ healthOk, cloudUrl, refreshing, onRefresh }: Props) {
  const tone: BadgeTone =
    healthOk === null ? 'muted' : healthOk ? 'ok' : 'danger';
  const healthLabel =
    healthOk === null ? 'checking…' : healthOk ? 'ok' : 'down';

  return (
    <Panel
      title="Cloud health"
      actions={
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
      }
    >
      <View style={styles.row}>
        <Text style={styles.mono}>status</Text>
        <StatusBadge tone={tone} label={healthLabel} />
      </View>
      <Text
        style={styles.monoMuted}
        accessibilityLabel={`tk-cloud base URL ${cloudUrl}`}
      >
        {cloudUrl}
      </Text>
      {healthOk === null ? (
        <ActivityIndicator
          color={colors.tkAccent}
          style={styles.spinner}
          accessibilityLabel="Checking cloud health"
        />
      ) : null}
    </Panel>
  );
}

const styles = StyleSheet.create({
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: colors.tkSpace2,
  },
  mono: {
    ...fontMono,
    color: colors.tkText,
    fontSize: 12,
  },
  monoMuted: {
    ...fontMono,
    color: colors.tkMuted,
    fontSize: 11,
  },
  ghostBtn: {
    paddingHorizontal: colors.tkSpace2,
    paddingVertical: colors.tkSpace1,
    borderRadius: colors.tkRadiusMd,
    borderWidth: 1,
    borderColor: colors.tkBorder,
  },
  ghostBtnText: {
    ...fontSans,
    color: colors.tkText,
    fontWeight: '600',
    fontSize: 12,
  },
  spinner: { marginTop: colors.tkSpace2 },
});
