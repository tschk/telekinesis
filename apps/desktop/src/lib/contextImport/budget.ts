import {
  MAX_CONTEXT_CHARACTERS,
  prepareContext,
  type ImportPreview,
  type PreparedContext,
} from "./index";

export function prepareEditableContext(
  preview: ImportPreview | null,
  selected: readonly string[],
  input: string,
): { prepared: PreparedContext | null; error: string | null } {
  const budget = Number(input);
  if (
    !input.trim() ||
    !Number.isSafeInteger(budget) ||
    budget < 1000 ||
    budget > MAX_CONTEXT_CHARACTERS
  ) {
    return {
      prepared: null,
      error: `Enter a whole number from 1000 to ${MAX_CONTEXT_CHARACTERS}.`,
    };
  }
  try {
    return {
      prepared: preview
        ? prepareContext(preview, selected, { maxCharacters: budget })
        : null,
      error: null,
    };
  } catch {
    return {
      prepared: null,
      error: "Could not prepare context. Adjust the selection or choose the export again.",
    };
  }
}
