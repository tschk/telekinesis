import { Pressable, StyleSheet, Text, View } from 'react-native';
import { colors, fontSans } from '../theme';

type Props = {
  message: string;
  onRetry?: () => void;
};

export function ErrorBanner({ message, onRetry }: Props) {
  return (
    <View
      style={styles.banner}
      accessibilityRole="alert"
      accessibilityLiveRegion="assertive"
      accessibilityLabel={`Error: ${message}`}
    >
      <Text style={styles.text}>{message}</Text>
      {onRetry ? (
        <Pressable
          onPress={onRetry}
          accessibilityRole="button"
          accessibilityLabel="Retry loading cloud data"
          hitSlop={8}
          style={styles.retry}
        >
          <Text style={styles.retryText}>Retry</Text>
        </Pressable>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  banner: {
    backgroundColor: 'rgba(248, 113, 113, 0.12)',
    borderColor: 'rgba(248, 113, 113, 0.4)',
    borderWidth: 1,
    borderRadius: colors.tkRadiusMd,
    padding: colors.tkSpace3,
    gap: colors.tkSpace2,
  },
  text: {
    ...fontSans,
    color: colors.tkDanger,
    fontSize: 13,
  },
  retry: {
    alignSelf: 'flex-start',
    paddingVertical: colors.tkSpace1,
    paddingHorizontal: colors.tkSpace2,
    borderRadius: colors.tkRadiusMd,
    borderWidth: 1,
    borderColor: colors.tkBorder,
  },
  retryText: {
    ...fontSans,
    color: colors.tkAccent,
    fontWeight: '600',
    fontSize: 13,
  },
});
