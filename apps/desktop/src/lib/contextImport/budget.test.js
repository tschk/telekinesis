import { expect, test } from 'bun:test';
import { prepareEditableContext } from './budget';
import { parseChatExport } from './index';

test('editable budgets reject fractional, transient, nonfinite and out-of-range values locally', () => {
  for (const input of ['1000.5', '', ' ', '-', '1e', 'NaN', 'Infinity', '1e999', '999', '-1000', '32001']) {
    expect(prepareEditableContext(null, [], input)).toEqual({ prepared: null, error: 'Enter a whole number from 1000 to 32000.' });
  }
});

test('clearing and retyping preserves selection and preview for valid bounded preparation', () => {
  const preview = parseChatExport(JSON.stringify({ uuid: 'chat', chat_messages: [{ uuid: 'message', sender: 'human', text: 'Reference text' }] }), { fileName: 'chat.json' });
  const selected = [preview.conversations[0].entries[0].id];
  for (const input of ['12000', '1000.5', '', '1', '100', '1000', '32000']) {
    const result = prepareEditableContext(preview, selected, input);
    if (['12000', '1000', '32000'].includes(input)) {
      expect(result.error).toBeNull();
      expect(result.prepared.includedEntryIds).toEqual(selected);
      expect(result.prepared.characters).toBeLessThanOrEqual(Number(input));
      expect(result.prepared.text).toContain('Reference text');
      expect(result.prepared.text).toContain('chat.json');
    } else expect(result.prepared).toBeNull();
  }
  expect(selected).toHaveLength(1);
  expect(preview.conversations[0].entries[0].text).toBe('Reference text');
});

test('unexpected preparation failures stay local', () => {
  expect(prepareEditableContext({}, ['missing'], '1000')).toEqual({ prepared: null, error: 'Could not prepare context. Adjust the selection or choose the export again.' });
});
