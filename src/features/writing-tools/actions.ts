import type { IconName } from "../../components/Icon";
import type { WritingActionId } from "../../types";

export interface WritingActionDefinition {
  id: WritingActionId;
  label: string;
  description: string;
  icon: IconName;
  resultOnly: boolean;
}

export const WRITING_ACTIONS: WritingActionDefinition[] = [
  { id: "proofread", label: "Proofread", description: "Correct grammar and spelling", icon: "proofread", resultOnly: false },
  { id: "rewrite", label: "Rewrite", description: "Improve clarity and wording", icon: "rewrite", resultOnly: false },
  { id: "concise", label: "Concise", description: "Shorten while preserving meaning", icon: "summarize", resultOnly: false },
  { id: "friendly", label: "Friendly", description: "Make the tone warmer", icon: "spark", resultOnly: false },
  { id: "professional", label: "Professional", description: "Polish the tone", icon: "pencil", resultOnly: false },
  { id: "summarize", label: "Summarize", description: "Create a short overview", icon: "summarize", resultOnly: true },
  { id: "key-points", label: "Key Points", description: "Extract the essentials", icon: "key-points", resultOnly: true },
  { id: "custom", label: "Custom", description: "Describe another change", icon: "pencil", resultOnly: false },
];

export function writingAction(id: WritingActionId) {
  return WRITING_ACTIONS.find((action) => action.id === id) ?? WRITING_ACTIONS[0];
}
