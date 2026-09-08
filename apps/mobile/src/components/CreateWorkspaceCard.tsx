import { Pressable, StyleSheet, Text, TextInput, View } from 'react-native';
import { colors } from '../theme';

type Props = {
  name: string;
  onChangeName: (value: string) => void;
  creating: boolean;
  onCreate: () => void;
};

export function CreateWorkspaceCard({
  name,
  onChangeName,
  creating,
  onCreate,
}: Props) {
  return (
    <View style={styles.card}>
      <Text style={styles.cardTitle} accessibilityRole="header">
        Create workspace
      </Text>
      <TextInput
        style={styles.input}
        placeholder="Name (optional)"
        placeholderTextColor={colors.hint}
        value={name}
        onChangeText={onChangeName}
        editable={!creating}
        autoCapitalize="none"
        autoCorrect={false}
        returnKeyType="done"
        onSubmitEditing={onCreate}
        accessibilityLabel="Workspace name, optional"
        accessibilityHint="Leave blank to let tk-cloud assign a name"
      />
      <Pressable
        style={[styles.primaryBtn, creating && styles.btnDisabled]}
        onPress={onCreate}
        disabled={creating}
        accessibilityRole="button"
        accessibilityLabel={creating ? 'Creating workspace' : 'Create workspace'}
        accessibilityState={{ disabled: creating, busy: creating }}
        android_ripple={{ color: '#134e4a' }}
      >
        <Text style={styles.primaryBtnText}>
          {creating ? 'Creating…' : 'Create workspace'}
        </Text>
      </Pressable>
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
  cardTitle: {
    color: colors.textSecondary,
    fontSize: 16,
    fontWeight: '600',
  },
  input: {
    backgroundColor: colors.bg,
    borderColor: colors.inputBorder,
    borderWidth: 1,
    borderRadius: 8,
    color: colors.text,
    paddingHorizontal: 12,
    paddingVertical: 10,
    fontSize: 15,
  },
  primaryBtn: {
    backgroundColor: colors.primary,
    borderRadius: 8,
    paddingVertical: 12,
    alignItems: 'center',
  },
  btnDisabled: { opacity: 0.6 },
  primaryBtnText: {
    color: colors.primaryText,
    fontWeight: '700',
    fontSize: 15,
  },
});
