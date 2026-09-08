import { Pressable, StyleSheet, Text, View } from 'react-native';
import { colors } from '../theme';

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
    backgroundColor: colors.errorBg,
    borderColor: colors.errorBorder,
    borderWidth: 1,
    borderRadius: 10,
    padding: 12,
    gap: 8,
  },
  text: {
    color: colors.errorText,
    fontSize: 13,
  },
  retry: {
    alignSelf: 'flex-start',
    paddingVertical: 4,
  },
  retryText: {
    color: colors.accent,
    fontWeight: '600',
    fontSize: 13,
  },
});
