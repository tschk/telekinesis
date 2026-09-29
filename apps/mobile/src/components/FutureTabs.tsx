import { useEffect, useRef, useState } from 'react';
import { Pressable, StyleSheet, Text, TextInput, View } from 'react-native';
import { connectWorkspaceStream, eventText, type WorkspaceStream } from '../api/tkCloud';
import { diffFromEvent } from '@tk/diff-view';
import { colors, fontSans } from '../theme';

const FUTURE_TABS = ['Status', 'Steer', 'Diffs', 'Approvals'] as const;

type Props = {
  /** When set, Status attaches the workspace event stream. */
  workspaceId?: string | null;
  baseUrl?: string;
};

export function FutureTabs({ workspaceId, baseUrl }: Props) {
  const [streamStatus, setStreamStatus] = useState<string | null>(null);
  const [steer, setSteer] = useState('');
  const [pendingApproval, setPendingApproval] = useState<string | null>(null);
  const [lastEvent, setLastEvent] = useState<string | null>(null);
  const [diff, setDiff] = useState<string | null>(null);
  const streamRef = useRef<WorkspaceStream | null>(null);

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
      onEvents: (events) => {
        const last = events.at(-1);
        if (!last) return;
        setLastEvent(eventText(last) ?? last.kind);
        const patch = diffFromEvent(last);
        if (patch) setDiff(patch);
        if (last.kind === 'approval_requested') {
          const payload = last.payload;
          const id =
            payload && typeof payload === 'object' && 'id' in payload
            && typeof payload.id === 'string'
              ? payload.id
              : String(last.id);
          setPendingApproval(id);
        }
      },
    });
    streamRef.current = stream;
    return () => {
      stream.close();
      streamRef.current = null;
    };
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
          ? `Status stream ${streamStatus}${lastEvent ? ` · ${lastEvent}` : ''}`
          : 'Status attaches when a workspace is selected. Diffs are not wired yet.'}
      </Text>
      {workspaceId ? (
        <View style={styles.steerRow}>
          <TextInput
            style={styles.steerInput}
            value={steer}
            onChangeText={setSteer}
            placeholder="Steer the running session"
            placeholderTextColor={colors.tkMuted}
            accessibilityLabel="Steer the running session"
          />
          <Pressable
            accessibilityRole="button"
            accessibilityLabel="Send steer"
            disabled={!steer.trim()}
            onPress={() => {
              const text = steer.trim();
              if (!text) return;
              streamRef.current?.send('steer', { text });
              setSteer('');
            }}
            style={styles.steerButton}
          >
            <Text style={styles.steerButtonText}>Steer</Text>
          </Pressable>
        </View>
      ) : null}
      {pendingApproval ? (
        <View style={styles.steerRow}>
          <Pressable
            accessibilityRole="button"
            accessibilityLabel="Approve"
            onPress={() => {
              streamRef.current?.send('approval_response', {
                id: pendingApproval,
                approved: true,
              });
              setPendingApproval(null);
            }}
            style={styles.steerButton}
          >
            <Text style={styles.steerButtonText}>Approve</Text>
          </Pressable>
          <Pressable
            accessibilityRole="button"
            accessibilityLabel="Deny"
            onPress={() => {
              streamRef.current?.send('approval_response', {
                id: pendingApproval,
                approved: false,
              });
              setPendingApproval(null);
            }}
            style={styles.steerButton}
          >
            <Text style={styles.steerButtonText}>Deny</Text>
          </Pressable>
        </View>
      ) : null}
      {diff ? (
        <Text style={styles.hint} accessibilityLabel="Latest workspace diff">
          {diff}
        </Text>
      ) : null}
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
  steerRow: {
    flexDirection: 'row',
    gap: colors.tkSpace2,
    marginTop: colors.tkSpace2,
    alignItems: 'center',
  },
  steerInput: {
    ...fontSans,
    flex: 1,
    color: colors.tkText,
    borderColor: colors.tkBorder,
    borderWidth: 1,
    borderRadius: 8,
    paddingHorizontal: colors.tkSpace2,
    paddingVertical: colors.tkSpace1,
    fontSize: 13,
  },
  steerButton: {
    backgroundColor: colors.tkSurface2,
    borderColor: colors.tkBorder,
    borderWidth: 1,
    borderRadius: 8,
    paddingHorizontal: colors.tkSpace3,
    paddingVertical: colors.tkSpace1 + 2,
  },
  steerButtonText: {
    ...fontSans,
    color: colors.tkText,
    fontSize: 12,
    fontWeight: '600',
  },
});
