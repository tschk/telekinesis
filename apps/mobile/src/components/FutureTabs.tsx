import { StyleSheet, Text, View } from 'react-native';
import { colors } from '../theme';

const FUTURE_TABS = ['Status', 'Steer', 'Diffs', 'Approvals'] as const;

export function FutureTabs() {
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
        Future tabs — Status / Steer / Diffs / Approvals (not wired yet)
      </Text>
    </View>
  );
}

const styles = StyleSheet.create({
  tabs: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: 8,
  },
  tabChip: {
    backgroundColor: colors.chipBg,
    borderColor: colors.chipBorder,
    borderWidth: 1,
    borderRadius: 999,
    paddingHorizontal: 12,
    paddingVertical: 6,
    opacity: 0.7,
  },
  tabText: {
    color: colors.chipText,
    fontSize: 12,
    fontWeight: '600',
  },
  hint: {
    color: colors.hint,
    fontSize: 12,
    marginTop: 8,
  },
});
