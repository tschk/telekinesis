import { StyleSheet, Text, View } from 'react-native';
import { colors, fontMono } from '../theme';

export type BadgeTone = 'ok' | 'warn' | 'danger' | 'muted';

type Props = {
  tone?: BadgeTone;
  label: string;
};

export function StatusBadge({ tone = 'muted', label }: Props) {
  return (
    <View
      style={[styles.badge, toneStyles[tone]]}
      accessibilityRole="text"
      accessibilityLabel={label}
    >
      <Text style={[styles.label, labelTone[tone]]}>{label}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  badge: {
    alignSelf: 'flex-start',
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: colors.tkSpace2,
    paddingVertical: 2,
    borderRadius: 999,
    borderWidth: 1,
    borderColor: colors.tkBorder,
    backgroundColor: colors.tkSurface2,
  },
  label: {
    ...fontMono,
    fontSize: 11,
    fontWeight: '600',
    color: colors.tkMuted,
  },
});

const toneStyles = StyleSheet.create({
  ok: { borderColor: 'rgba(52, 211, 153, 0.4)' },
  warn: { borderColor: 'rgba(251, 191, 36, 0.4)' },
  danger: { borderColor: 'rgba(248, 113, 113, 0.4)' },
  muted: { borderColor: colors.tkBorder },
});

const labelTone = StyleSheet.create({
  ok: { color: colors.tkSuccess },
  warn: { color: colors.tkWarn },
  danger: { color: colors.tkDanger },
  muted: { color: colors.tkMuted },
});
