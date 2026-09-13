import { Pressable, StyleSheet, Text, TextInput } from 'react-native';
import { Panel } from './Panel';
import { colors, fontSans } from '../theme';

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
    <Panel title="Create workspace">
      <TextInput
        style={styles.input}
        placeholder="Name (optional)"
        placeholderTextColor={colors.tkMuted}
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
        android_ripple={{ color: colors.tkBorder }}
      >
        <Text style={styles.primaryBtnText}>
          {creating ? 'Creating…' : 'Create workspace'}
        </Text>
      </Pressable>
    </Panel>
  );
}

const styles = StyleSheet.create({
  input: {
    ...fontSans,
    backgroundColor: colors.tkBg,
    borderColor: colors.tkBorder,
    borderWidth: 1,
    borderRadius: colors.tkRadiusMd,
    color: colors.tkText,
    paddingHorizontal: colors.tkSpace3,
    paddingVertical: colors.tkSpace2,
    fontSize: 14,
  },
  primaryBtn: {
    backgroundColor: colors.tkAccent,
    borderRadius: colors.tkRadiusMd,
    paddingVertical: colors.tkSpace3,
    alignItems: 'center',
    marginTop: colors.tkSpace1,
  },
  btnDisabled: { opacity: 0.5 },
  primaryBtnText: {
    ...fontSans,
    color: colors.tkAccentFg,
    fontWeight: '700',
    fontSize: 14,
  },
});
