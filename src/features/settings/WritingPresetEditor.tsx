import { useState } from "react";
import { Button } from "../../components/Button";
import { Icon } from "../../components/Icon";
import { SegmentedControl } from "../../components/SegmentedControl";
import { ModelQueueEditor } from "./ModelQueueEditor";
import { normalizeAiProvider } from "../../ai/models";
import {
  DEFAULT_PRESET_TEMPLATE,
  INSTRUCTION_PLACEHOLDER,
  MAX_WRITING_PRESETS,
  OUTPUT_RULE_PLACEHOLDER,
  isBuiltInPreset,
  newCustomPreset,
  normalizeWritingPreset,
  resolveWritingPresets,
  withAddedPreset,
  withMovedPreset,
  withRemovedPreset,
  withResetAllPresets,
  withResetPreset,
  withWritingPreset,
} from "../writing-tools/presets";
import type { AppSettings, WritingPreset } from "../../types";

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
    <div className="writing-preset-list">
      <ol className="writing-preset-list__items">
        {presets.map((preset, index) => {
          const editing = editingId === preset.id;
          let modelSummary = "Global AI models";
          if (preset.models.length > 0) {
            const noun = preset.models.length === 1 ? "model" : "models";
            modelSummary = `${preset.models.length} ${noun} (custom order)`;
          }
          return (
            <li className="writing-preset" data-editing={editing} key={preset.id}>
              <div className="writing-preset__summary">
                <span aria-hidden className="writing-preset__position" data-first={index === 0}>{index + 1}</span>
                <span className="writing-preset__icon" aria-hidden><Icon name={preset.icon} size={16} /></span>
                <span className="writing-preset__copy">
                  <strong>{preset.label || "Untitled preset"}</strong>
                  <small>{preset.description || modelSummary}</small>
                </span>
                <span className="writing-preset__actions">
                  <Button
                    aria-label={`Move ${preset.label} up`}
                    compact
                    disabled={index === 0}
                    icon="chevron-up"
                    onClick={() => void save(withMovedPreset(settings, preset.id, -1))}
                    title="Move up"
                  />
                  <Button
                    aria-label={`Move ${preset.label} down`}
                    compact
                    disabled={index === presets.length - 1}
                    icon="chevron-down"
                    onClick={() => void save(withMovedPreset(settings, preset.id, 1))}
                    title="Move down"
                  />
                  <Button
                    aria-expanded={editing}
                    compact
                    icon="pencil"
                    onClick={() => setEditingId(editing ? null : preset.id)}
                  >
                    {editing ? "Close" : "Edit"}
                  </Button>
                  <Button
                    aria-label={`Remove ${preset.label}`}
                    compact
                    disabled={presets.length <= 1}
                    icon="close"
                    onClick={() => {
                      if (editing) setEditingId(null);
                      void save(withRemovedPreset(settings, preset.id));
                    }}
                    title="Remove from the menu"
                    tone="danger"
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
      <div className="writing-preset-list__footer">
        <Button
          compact
          disabled={atCap}
          icon="spark"
          onClick={() => {
            const preset = newCustomPreset();
            setEditingId(preset.id);
            void save(withAddedPreset(settings, preset));
          }}
        >
          {atCap ? `Up to ${MAX_WRITING_PRESETS} presets` : "Add preset"}
        </Button>
        <Button compact onClick={() => { setEditingId(null); void save(withResetAllPresets()); }}>
          Reset all to defaults
        </Button>
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
      className="writing-preset-form"
      onSubmit={(event) => {
        event.preventDefault();
        commit();
      }}
    >
      <label className="writing-preset-form__field">
        <span>Name</span>
        <input
          autoComplete="off"
          maxLength={60}
          onChange={(event) => patch({ label: event.target.value })}
          placeholder="e.g. Make it polite"
          required
          value={draft.label}
        />
      </label>
      <label className="writing-preset-form__field">
        <span>Description</span>
        <input
          autoComplete="off"
          maxLength={60}
          onChange={(event) => patch({ description: event.target.value })}
          placeholder="Shown under the name in the menu"
          value={draft.description}
        />
      </label>
      <label className="writing-preset-form__field">
        <span>What it should do</span>
        <textarea
          onChange={(event) => patch({ instruction: event.target.value })}
          placeholder="Describe the change, e.g. Make the tone warmer and more conversational."
          required
          rows={3}
          value={draft.instruction}
        />
      </label>

      <div className="writing-preset-form__field">
        <span>Result</span>
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

      <div className="writing-preset-form__field">
        <span>Models</span>
        <SegmentedControl
          ariaLabel="Model priority"
          onChange={(value) => patch({ models: value === "custom" ? modelsForPriority(draft.models, settings.aiModels) : [] })}
          options={[
            { label: "Follow global models", value: "global" },
            { label: "Custom priority", value: "custom" },
          ]}
          value={useGlobalModels ? "global" : "custom"}
        />
        {useGlobalModels ? (
          <p className="writing-preset-form__hint">Uses the ordered AI models from the AI settings.</p>
        ) : (
          <ModelQueueEditor
            provider={provider}
            value={draft.models}
            onChange={(models) => patch({ models })}
          />
        )}
      </div>

      <div className="writing-preset-form__advanced">
        <button
          aria-expanded={showTemplate}
          className="text-link"
          onClick={() => setShowTemplate((value) => !value)}
          type="button"
        >
          {showTemplate ? "Hide advanced prompt" : "Advanced: edit the prompt template"}
        </button>
        {showTemplate ? (
          <div className="writing-preset-form__field">
            <span>Prompt template</span>
            <textarea
              className="writing-preset-form__template"
              onChange={(event) => patch({ template: event.target.value })}
              rows={8}
              spellCheck={false}
              value={draft.template ?? DEFAULT_PRESET_TEMPLATE}
            />
            <p className="writing-preset-form__hint">
              {INSTRUCTION_PLACEHOLDER} is replaced with "What it should do"; {OUTPUT_RULE_PLACEHOLDER} with the result rule.
              Remove a placeholder to stop substituting it.
            </p>
            {draft.template != null ? (
              <Button compact onClick={() => patch({ template: null })}>Reset template</Button>
            ) : null}
          </div>
        ) : null}
      </div>

      <div className="writing-preset-form__footer">
        {isBuiltInPreset(preset.id) ? (
          <Button
            compact
            onClick={() => {
              void save(withResetPreset(settings, preset.id));
              onClose();
            }}
          >
            Reset this preset
          </Button>
        ) : (
          <span />
        )}
        <span className="writing-preset-form__footer-actions">
          <Button compact onClick={onClose}>Cancel</Button>
          <Button compact disabled={draft.label.trim() === "" || draft.instruction.trim() === ""} tone="primary" type="submit">
            Save preset
          </Button>
        </span>
      </div>
    </form>
  );
}
