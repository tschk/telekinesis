import type { ReactNode } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { colors, fontSans, tkShadowSm } from '../theme';

type Props = {
  title?: string;
  actions?: ReactNode;
  children: ReactNode;
  accessibilityLabel?: string;
};

export function Panel({ title, actions, children, accessibilityLabel }: Props) {
  return (
    <View style={styles.panel} accessibilityLabel={accessibilityLabel}>
      {(title || actions) && (
        <View style={styles.header}>
          {title ? (
            <Text style={styles.title} accessibilityRole="header">
              {title}
            </Text>
          ) : (
            <View />
          )}
          {actions ? <View style={styles.actions}>{actions}</View> : null}
        </View>
      )}
      <View style={styles.body}>{children}</View>
    </View>
  );
}

const styles = StyleSheet.create({
  panel: {
    backgroundColor: colors.tkSurface,
    borderColor: colors.tkBorder,
    borderWidth: 1,
    borderRadius: colors.tkRadiusLg,
    overflow: 'hidden',
    ...tkShadowSm,
  },
  header: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: colors.tkSpace3,
    paddingHorizontal: colors.tkSpace4,
    paddingVertical: colors.tkSpace3,
    borderBottomWidth: 1,
    borderBottomColor: colors.tkBorder,
    backgroundColor: colors.tkSurface2,
  },
  title: {
    ...fontSans,
    color: colors.tkText,
    fontSize: 14,
    fontWeight: '600',
  },
  actions: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: colors.tkSpace2,
  },
  body: {
    padding: colors.tkSpace4,
    gap: colors.tkSpace2,
  },
});
