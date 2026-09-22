import { Button } from "@astryxdesign/core/Button";
import * as stylex from "@stylexjs/stylex";
import { useState } from "react";
import { normalizeAiProvider } from "../../ai/models";
import { Icon } from "../../components/Icon";
import { SegmentedControl } from "../../components/SegmentedControl";
import type { AppSettings, WritingPreset } from "../../types";
import {
  DEFAULT_PRESET_TEMPLATE,
  INSTRUCTION_PLACEHOLDER,
  isBuiltInPreset,
  MAX_WRITING_PRESETS,
  newCustomPreset,
  normalizeWritingPreset,
  OUTPUT_RULE_PLACEHOLDER,
  resolveWritingPresets,
  withAddedPreset,
  withMovedPreset,
  withRemovedPreset,
  withResetAllPresets,
  withResetPreset,
  withWritingPreset,
} from "../writing-tools/presets";
import { ModelQueueEditor } from "./ModelQueueEditor";

interface WritingPresetListProps {
  readonly settings: AppSettings;
  readonly save: (patch: Partial<AppSettings>) => Promise<void>;
}

/**
 * Editable Writing Tools presets. The list controls the popup order and which
 * presets appear; each row expands into a form where the easy fields (name,
 * description, "what it should do") and the advanced template and per-preset
 * model priority can be changed. Everything can be reset to the defaults.
 */
export function WritingPresetList({ settings, save }: WritingPresetListProps) {
  const [editingId, setEditingId] = useState<string | null>(null);
  const presets = resolveWritingPresets(settings);
  const atCap = settings.enabledWritingActions.length >= MAX_WRITING_PRESETS;

  return (
    <div {...stylex.props(styles.list)}>
      <ol {...stylex.props(styles.items)}>
        {presets.map((preset, index) => {
          const editing = editingId === preset.id;
          let modelSummary = "Global AI models";
          if (preset.models.length > 0) {
            const noun = preset.models.length === 1 ? "model" : "models";
            modelSummary = `${preset.models.length} ${noun} (custom order)`;
          }
          return (
            <li key={preset.id} {...stylex.props(styles.preset, editing && styles.presetEditing)}>
              <div {...stylex.props(styles.summary)}>
                <span
                  aria-hidden
                  {...stylex.props(styles.position, index === 0 && styles.positionFirst)}
                >
                  {index + 1}
                </span>
                <span aria-hidden {...stylex.props(styles.icon)}>
                  <Icon name={preset.icon} size={16} />
                </span>
                <span {...stylex.props(styles.copy)}>
                  <strong {...stylex.props(styles.copyTitle)}>
                    {preset.label || "Untitled preset"}
                  </strong>
                  <small {...stylex.props(styles.copyMeta)}>
                    {preset.description || modelSummary}
                  </small>
                </span>
                <span {...stylex.props(styles.actions)}>
                  <Button
                    icon={<Icon name="chevron-up" size={14} />}
                    isDisabled={index === 0}
                    isIconOnly
                    label={`Move ${preset.label} up`}
                    onClick={() => void save(withMovedPreset(settings, preset.id, -1))}
                    size="sm"
                    tooltip="Move up"
                  />
                  <Button
                    icon={<Icon name="chevron-down" size={14} />}
                    isDisabled={index === presets.length - 1}
                    isIconOnly
                    label={`Move ${preset.label} down`}
                    onClick={() => void save(withMovedPreset(settings, preset.id, 1))}
                    size="sm"
                    tooltip="Move down"
                  />
                  <Button
                    aria-expanded={editing}
                    icon={<Icon name="pencil" size={14} />}
                    label={editing ? "Close" : "Edit"}
                    onClick={() => setEditingId(editing ? null : preset.id)}
                    size="sm"
                  />
                  <Button
                    icon={<Icon name="close" size={14} />}
                    isDisabled={presets.length <= 1}
                    isIconOnly
                    label={`Remove ${preset.label}`}
                    onClick={() => {
                      if (editing) setEditingId(null);
                      void save(withRemovedPreset(settings, preset.id));
                    }}
                    size="sm"
                    tooltip="Remove from the menu"
                    variant="destructive"
                  />
                </span>
              </div>
              {editing ? (
                <WritingPresetForm
                  onClose={() => setEditingId(null)}
                  preset={preset}
                  save={save}
                  settings={settings}
                />
              ) : null}
            </li>
          );
        })}
      </ol>
      <div {...stylex.props(styles.listFooter)}>
        <Button
          icon={<Icon name="spark" size={14} />}
          isDisabled={atCap}
          label={atCap ? `Up to ${MAX_WRITING_PRESETS} presets` : "Add preset"}
          onClick={() => {
            const preset = newCustomPreset();
            setEditingId(preset.id);
            void save(withAddedPreset(settings, preset));
          }}
          size="sm"
        />
        <Button
          label="Reset all to defaults"
          onClick={() => {
            setEditingId(null);
            void save(withResetAllPresets());
          }}
          size="sm"
        />
      </div>
    </div>
  );
}

interface WritingPresetFormProps {
  readonly preset: WritingPreset;
  readonly settings: AppSettings;
  readonly save: (patch: Partial<AppSettings>) => Promise<void>;
  readonly onClose: () => void;
}

/** Seed a preset's custom priority with its saved list or the global queue. */
function modelsForPriority(current: string[], fallback: string[]): string[] {
  return current.length > 0 ? current : fallback;
}

function WritingPresetForm({ preset, settings, save, onClose }: WritingPresetFormProps) {
  const provider = normalizeAiProvider(settings.aiProvider);
  const [draft, setDraft] = useState<WritingPreset>(preset);
  const [showTemplate, setShowTemplate] = useState(preset.template != null);
  const useGlobalModels = draft.models.length === 0;

  function patch(next: Partial<WritingPreset>) {
    setDraft((current) => ({ ...current, ...next }));
  }

  function commit() {
    const normalized = normalizeWritingPreset(draft);
    if (normalized == null) return;
    void save(withWritingPreset(settings, normalized));
    onClose();
  }

  return (
    <form
      onSubmit={(event) => {
        event.preventDefault();
        commit();
      }}
      {...stylex.props(styles.form)}
    >
      <label {...stylex.props(styles.field)}>
        <span {...stylex.props(styles.fieldHeading)}>Name</span>
        <input
          autoComplete="off"
          maxLength={60}
          onChange={(event) => patch({ label: event.target.value })}
          placeholder="e.g. Make it polite"
          required
          value={draft.label}
          {...stylex.props(styles.input)}
        />
      </label>
      <label {...stylex.props(styles.field)}>
        <span {...stylex.props(styles.fieldHeading)}>Description</span>
        <input
          autoComplete="off"
          maxLength={60}
          onChange={(event) => patch({ description: event.target.value })}
          placeholder="Shown under the name in the menu"
          value={draft.description}
          {...stylex.props(styles.input)}
        />
      </label>
      <label {...stylex.props(styles.field)}>
        <span {...stylex.props(styles.fieldHeading)}>What it should do</span>
        <textarea
          onChange={(event) => patch({ instruction: event.target.value })}
          placeholder="Describe the change, e.g. Make the tone warmer and more conversational."
          required
          rows={3}
          value={draft.instruction}
          {...stylex.props(styles.textarea)}
        />
      </label>

      <div {...stylex.props(styles.field)}>
        <span {...stylex.props(styles.fieldHeading)}>Result</span>
        <SegmentedControl
          ariaLabel="Result behavior"
          onChange={(value) => patch({ replacesSelection: value === "replace" })}
          options={[
            { label: "Replace selection", value: "replace" },
            { label: "Show as result", value: "show" },
          ]}
          value={draft.replacesSelection ? "replace" : "show"}
        />
      </div>

      <div {...stylex.props(styles.field)}>
        <span {...stylex.props(styles.fieldHeading)}>Models</span>
        <SegmentedControl
          ariaLabel="Model priority"
          onChange={(value) =>
            patch({
              models: value === "custom" ? modelsForPriority(draft.models, settings.aiModels) : [],
            })
          }
          options={[
            { label: "Follow global models", value: "global" },
            { label: "Custom priority", value: "custom" },
          ]}
          value={useGlobalModels ? "global" : "custom"}
        />
        {useGlobalModels ? (
          <p {...stylex.props(styles.hint)}>Uses the ordered AI models from the AI settings.</p>
        ) : (
          <ModelQueueEditor
            provider={provider}
            value={draft.models}
            onChange={(models) => patch({ models })}
          />
        )}
      </div>

      <div {...stylex.props(styles.advanced)}>
        <Button
          aria-expanded={showTemplate}
          label={showTemplate ? "Hide advanced prompt" : "Advanced: edit the prompt template"}
          onClick={() => setShowTemplate((value) => !value)}
          size="sm"
          variant="ghost"
          xstyle={styles.textLink}
        />
        {showTemplate ? (
          <div {...stylex.props(styles.field)}>
            <span {...stylex.props(styles.fieldHeading)}>Prompt template</span>
            <textarea
              onChange={(event) => patch({ template: event.target.value })}
              rows={8}
              spellCheck={false}
              value={draft.template ?? DEFAULT_PRESET_TEMPLATE}
              {...stylex.props(styles.textarea, styles.template)}
            />
            <p {...stylex.props(styles.hint)}>
              {INSTRUCTION_PLACEHOLDER} is replaced with "What it should do";{" "}
              {OUTPUT_RULE_PLACEHOLDER} with the result rule. Remove a placeholder to stop
              substituting it.
            </p>
            {draft.template != null ? (
              <Button label="Reset template" onClick={() => patch({ template: null })} size="sm" />
            ) : null}
          </div>
        ) : null}
      </div>

      <div {...stylex.props(styles.footer)}>
        {isBuiltInPreset(preset.id) ? (
          <Button
            label="Reset this preset"
            onClick={() => {
              void save(withResetPreset(settings, preset.id));
              onClose();
            }}
            size="sm"
          />
        ) : (
          <span />
        )}
        <span {...stylex.props(styles.footerActions)}>
          <Button label="Cancel" onClick={onClose} size="sm" />
          <Button
            isDisabled={draft.label.trim() === "" || draft.instruction.trim() === ""}
            label="Save preset"
            size="sm"
            type="submit"
            variant="primary"
          />
        </span>
      </div>
    </form>
  );
}

const styles = stylex.create({
  list: {
    display: "grid",
    gap: "10px",
    padding: "12px",
  },
  items: {
    display: "grid",
    gap: "8px",
    margin: 0,
    padding: 0,
    listStyle: "none",
  },
  preset: {
    overflow: "hidden",
    backgroundColor: "var(--color-background-card)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border)",
    borderRadius: "var(--radius-element)",
  },
  presetEditing: {
    borderColor: "var(--color-accent)",
  },
  summary: {
    display: "flex",
    alignItems: "center",
    gap: "10px",
    padding: "10px 12px",
  },
  position: {
    display: "grid",
    placeItems: "center",
    flexShrink: 0,
    width: "24px",
    height: "24px",
    color: "var(--color-text-secondary)",
    fontSize: "11px",
    fontWeight: 650,
    fontVariantNumeric: "tabular-nums",
    backgroundColor: "var(--color-background-muted)",
    borderRadius: "50%",
  },
  positionFirst: {
    color: "var(--color-on-accent)",
    backgroundColor: "var(--color-accent)",
  },
  icon: {
    display: "grid",
    placeItems: "center",
    flexShrink: 0,
    color: "var(--color-text-secondary)",
  },
  copy: {
    display: "grid",
    flex: 1,
    gap: "2px",
    minWidth: 0,
  },
  copyTitle: {
    fontSize: "13px",
    fontWeight: 620,
  },
  copyMeta: {
    overflow: "hidden",
    color: "var(--color-text-secondary)",
    fontSize: "11px",
    textOverflow: "ellipsis",
    whiteSpace: "nowrap",
  },
  actions: {
    display: "flex",
    flexShrink: 0,
    gap: "4px",
  },
  form: {
    display: "grid",
    gap: "12px",
    padding: "14px",
    backgroundColor: "var(--color-background-muted)",
    borderBlockStartWidth: "1px",
    borderBlockStartStyle: "solid",
    borderBlockStartColor: "var(--color-border)",
  },
  field: {
    display: "grid",
    gap: "5px",
  },
  fieldHeading: {
    color: "var(--color-text-secondary)",
    fontSize: "11px",
    fontWeight: 650,
  },
  input: {
    width: "100%",
    minHeight: "32px",
    padding: "0 10px",
    userSelect: "text",
    color: "var(--color-text-primary)",
    backgroundColor: "var(--color-background-card)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border-emphasized)",
    borderRadius: "var(--radius-element)",
    ":focus": {
      borderColor: "color-mix(in srgb, var(--color-accent) 55%, transparent)",
      outline: "none",
    },
    "::placeholder": {
      color: "var(--color-text-disabled)",
    },
  },
  textarea: {
    width: "100%",
    padding: "8px 10px",
    resize: "vertical",
    userSelect: "text",
    color: "var(--color-text-primary)",
    backgroundColor: "var(--color-background-card)",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "var(--color-border-emphasized)",
    borderRadius: "var(--radius-element)",
    lineHeight: 1.5,
    ":focus": {
      borderColor: "color-mix(in srgb, var(--color-accent) 55%, transparent)",
      outline: "none",
    },
    "::placeholder": {
      color: "var(--color-text-disabled)",
    },
  },
  template: {
    fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
    fontSize: "11.5px",
  },
  hint: {
    margin: 0,
    color: "var(--kivo-text-tertiary)",
    fontSize: "11px",
    lineHeight: 1.4,
  },
  advanced: {
    display: "grid",
    gap: "10px",
  },
  textLink: {
    justifySelf: "start",
    height: "auto",
    padding: 0,
    color: "var(--color-accent)",
    fontSize: "12px",
    fontWeight: 400,
  },
  footer: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: "8px",
  },
  footerActions: {
    display: "flex",
    gap: "7px",
  },
  listFooter: {
    display: "flex",
    justifyContent: "space-between",
    gap: "7px",
  },
});
