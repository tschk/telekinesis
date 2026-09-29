import { useEffect, useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { connectWorkspaceStream } from '../api/tkCloud';
import { colors, fontSans } from '../theme';

const FUTURE_TABS = ['Status', 'Steer', 'Diffs', 'Approvals'] as const;

type Props = {
  /** When set, Status attaches the workspace event stream. */
  workspaceId?: string | null;
  baseUrl?: string;
};

export function FutureTabs({ workspaceId, baseUrl }: Props) {
  const [streamStatus, setStreamStatus] = useState<string | null>(null);

  useEffect(() => {
    if (!workspaceId || !baseUrl) {
      setStreamStatus(null);
      return;
    }
    const stream = connectWorkspaceStream({
      baseUrl,
      workspaceId,
      onStatus: setStreamStatus,
      onUnavailable: () => setStreamStatus('unavailable'),
      onEvents: () => {
        // Steer / Diffs / Approvals render these later. Status only attaches.
      },
    });
    return () => stream.close();
  }, [workspaceId, baseUrl]);

  return (
    <View>
      <View
        style={styles.tabs}
        accessibilityRole="tablist"
        accessibilityLabel="Companion tabs. Status, Steer, Diffs, and Approvals are not available yet."
      >
        {FUTURE_TABS.map((label) => (
          <View
            key={label}
            style={styles.tabChip}
            accessibilityRole="tab"
            accessibilityState={{ disabled: true, selected: false }}
            accessibilityLabel={`${label} tab, coming soon`}
          >
            <Text style={styles.tabText}>{label}</Text>
          </View>
        ))}
      </View>
      <Text style={styles.hint}>
        {streamStatus
          ? `Status stream ${streamStatus}. Steer, Diffs, and Approvals still read the same events.`
          : 'Status attaches when a workspace is selected. Steer / Diffs / Approvals are not wired yet.'}
      </Text>
    </View>
  );
}

const styles = StyleSheet.create({
  tabs: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: colors.tkSpace2,
  },
  tabChip: {
    backgroundColor: colors.tkSurface2,
    borderColor: colors.tkBorder,
    borderWidth: 1,
    borderRadius: 999,
    paddingHorizontal: colors.tkSpace3,
    paddingVertical: colors.tkSpace1 + 2,
    opacity: 0.75,
  },
  tabText: {
    ...fontSans,
    color: colors.tkMuted,
    fontSize: 12,
    fontWeight: '600',
  },
  hint: {
    ...fontSans,
    color: colors.tkMuted,
    fontSize: 12,
    marginTop: colors.tkSpace2,
  },
});
