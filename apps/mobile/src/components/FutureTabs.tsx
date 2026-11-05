import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { Pressable, StyleSheet, Text, TextInput, View } from 'react-native';
import { connectWorkspaceStream, eventText } from '../api/tkCloud';
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
  const identityRef = useRef({ workspaceId, baseUrl });
  useLayoutEffect(() => {
    identityRef.current = { workspaceId, baseUrl };
    return () => { identityRef.current = { workspaceId: null, baseUrl: undefined }; };
  }, [workspaceId, baseUrl]);
  const [stateIdentity, setStateIdentity] = useState({ workspaceId, baseUrl });
  if (stateIdentity.workspaceId !== workspaceId || stateIdentity.baseUrl !== baseUrl) {
    setStateIdentity({ workspaceId, baseUrl });
    setPendingApproval(null);
    setDiff(null);
    setLastEvent(null);
    setStreamStatus(null);
  }

  useEffect(() => {
    if (!workspaceId || !baseUrl) {
      setStreamStatus(null);
      return;
    }
    let active = true;
    const isCurrent = () => active && identityRef.current.workspaceId === workspaceId
      && identityRef.current.baseUrl === baseUrl;
    const stream = connectWorkspaceStream({
      baseUrl,
      workspaceId,
      onStatus: (status) => { if (isCurrent()) setStreamStatus(status); },
      onUnavailable: () => { if (isCurrent()) setStreamStatus('unavailable'); },
      onEvents: (events) => {
        if (!isCurrent()) return;
        const last = events.at(-1);
        if (!last) return;
        setLastEvent(eventText(last) ?? last.kind);
        for (const event of events) {
          const patch = diffFromEvent(event);
          if (patch) setDiff(patch);
          if (event.kind === 'approval_requested') {
            const payload = event.payload;
            const id =
              payload && typeof payload === 'object' && 'id' in payload
              && typeof payload.id === 'string'
                ? payload.id
                : String(event.id);
            setPendingApproval(id);
          }
        }
      },
    });
    return () => {
      active = false;
      stream.close();
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
            disabled
            style={styles.steerButton}
          >
            <Text style={styles.steerButtonText}>Steer</Text>
          </Pressable>
        </View>
      ) : null}
      {workspaceId ? <Text style={styles.hint}>Remote steering is unavailable.</Text> : null}
      {pendingApproval ? (
        <View style={styles.steerRow}>
          <Text style={styles.hint}>Approval {pendingApproval}: remote approval is unavailable.</Text>
          <Pressable
            accessibilityRole="button"
            accessibilityLabel="Approve"
            disabled
            style={styles.steerButton}
          >
            <Text style={styles.steerButtonText}>Approve</Text>
          </Pressable>
          <Pressable
            accessibilityRole="button"
            accessibilityLabel="Deny"
            disabled
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
