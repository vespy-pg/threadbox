import { useCallback, useEffect, useState } from "react";
import { Check, Download, KeyRound, LoaderCircle, ShieldCheck, Trash2, Zap } from "lucide-react";

import { api } from "./api";
import type { ApiModelSettings, ApiProvider, AppSettings, LanguageModelKind, LanguageModelSettings, LanguageModelStatus, LocalModelSettings, ModelStatus, ProviderProbe, SpeechModelId, SpeechSettings } from "./types";

/** Enough of the common cases to choose from, with detection first because it is the default. */
export const speechLanguages: Array<{ value: string; label: string }> = [
  { value: "auto", label: "Detect per recording" },
  { value: "pl", label: "Polish" },
  { value: "en", label: "English" },
  { value: "de", label: "German" },
  { value: "fr", label: "French" },
  { value: "es", label: "Spanish" },
  { value: "it", label: "Italian" },
  { value: "nl", label: "Dutch" },
  { value: "pt", label: "Portuguese" },
  { value: "cs", label: "Czech" },
  { value: "uk", label: "Ukrainian" },
  { value: "sv", label: "Swedish" },
];

const apiProviders: Array<{ value: ApiProvider; label: string; suggestion: string }> = [
  { value: "anthropic", label: "Anthropic", suggestion: "claude-opus-5" },
  { value: "openai", label: "OpenAI", suggestion: "gpt-5" },
  { value: "openrouter", label: "OpenRouter", suggestion: "anthropic/claude-opus-5" },
  { value: "compatible", label: "Any OpenAI-compatible endpoint", suggestion: "" },
];

function gigabytes(bytes: number): string {
  return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
}

/** Keeps a text field editable while only writing the settings once the user leaves it. */
function useDraft<T>(value: T): [T, (next: T) => void] {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  return [draft, setDraft];
}

export function SpeechProviderSettings({ settings, onChange, onError }: { settings: SpeechSettings; onChange: (patch: Partial<SpeechSettings>) => void; onError: (message: string) => void }) {
  const [models, setModels] = useState<ModelStatus[]>([]);
  const [downloading, setDownloading] = useState<SpeechModelId | null>(null);

  const reload = useCallback(async () => {
    try {
      setModels(await api.speechModels());
    } catch (reason) {
      onError(String(reason));
    }
  }, [onError]);

  useEffect(() => { void reload(); }, [reload]);

  async function download(id: SpeechModelId) {
    try {
      setDownloading(id);
      await api.downloadModel(id);
      await reload();
    } catch (reason) {
      onError(String(reason));
    } finally {
      setDownloading(null);
    }
  }

  const selected = models.find((model) => model.id === settings.model);

  return <>
    <label className="setting-select"><span>Model for meetings</span><select value={settings.model} onChange={(event) => onChange({ model: event.target.value as SpeechModelId })}>{models.map((model) => <option key={model.id} value={model.id}>{model.label}{model.installed ? "" : ` - not downloaded, ${gigabytes(model.approximateBytes)}`}</option>)}</select></label>
    {selected && <p className="setting-note">{selected.note}</p>}
    <div className="model-list">
      {models.map((model) => <div className="model-row" key={model.id}>
        <div><strong>{model.label}</strong><small>{model.installed ? `Downloaded, ${gigabytes(model.sizeBytes ?? model.approximateBytes)}` : `${gigabytes(model.approximateBytes)} to download`}</small></div>
        {model.installed ? <span className="model-ready"><Check size={14} />Ready</span> : <button type="button" className="secondary-button" disabled={downloading !== null} onClick={() => void download(model.id)}>{downloading === model.id ? <LoaderCircle size={14} className="spin" /> : <Download size={14} />}{downloading === model.id ? "Downloading..." : "Download"}</button>}
      </div>)}
    </div>
    <p className="setting-note">Voice notes always use the small model, so capturing a thought stays fast whatever is chosen here.</p>
    <label className="setting-select"><span>Spoken language</span><select value={settings.language} onChange={(event) => onChange({ language: event.target.value })}>{speechLanguages.map((language) => <option key={language.value} value={language.value}>{language.label}</option>)}</select></label>
    <label className="setting-select"><span>Terminology language</span><select value={settings.terminologyLanguage ?? ""} onChange={(event) => onChange({ terminologyLanguage: event.target.value || null })}><option value="">Same as spoken</option>{speechLanguages.filter((language) => language.value !== "auto").map((language) => <option key={language.value} value={language.value}>{language.label}</option>)}</select></label>
    <p className="setting-note">Set this when meetings are held in one language about terminology written in another. A project can override both.</p>
  </>;
}

export function LanguageModelProviderSettings({ settings, onChange, onError }: { settings: LanguageModelSettings; onChange: (patch: Partial<LanguageModelSettings>) => void; onError: (message: string) => void }) {
  const [status, setStatus] = useState<LanguageModelStatus | null>(null);
  const [probe, setProbe] = useState<ProviderProbe | null>(null);
  const [testing, setTesting] = useState(false);
  const [key, setKey] = useState("");

  const reload = useCallback(async () => {
    try {
      setStatus(await api.languageModelStatus());
    } catch (reason) {
      onError(String(reason));
    }
  }, [onError]);

  useEffect(() => { void reload(); }, [reload, settings]);

  async function test() {
    try {
      setTesting(true);
      setProbe(await api.testLanguageModel());
    } catch (reason) {
      onError(String(reason));
    } finally {
      setTesting(false);
    }
  }

  async function saveKey() {
    const provider = settings.api.provider;
    if (!provider) return;
    try {
      setStatus(await api.setLanguageModelKey(provider, key));
      setKey("");
    } catch (reason) {
      onError(String(reason));
    }
  }

  async function removeKey() {
    const provider = settings.api.provider;
    if (!provider) return;
    try {
      setStatus(await api.deleteLanguageModelKey(provider));
    } catch (reason) {
      onError(String(reason));
    }
  }

  return <>
    <label className="setting-select"><span>Analysis runs on</span><select value={settings.kind} onChange={(event) => onChange({ kind: event.target.value as LanguageModelKind })}>
      <option value="unset">Nothing yet - decide later</option>
      <option value="local">A model on this computer</option>
      <option value="api">An external provider, with my API key</option>
      <option value="agent">A command line tool I already use</option>
    </select></label>

    {settings.kind === "local" && <LocalFields settings={settings.local} onChange={(local) => onChange({ local: { ...settings.local, ...local } })} />}
    {settings.kind === "api" && <ApiFields settings={settings.api} providersWithKeys={status?.providersWithKeys ?? []} keyDraft={key} onKeyDraft={setKey} onSaveKey={() => void saveKey()} onRemoveKey={() => void removeKey()} onChange={(patch) => onChange({ api: { ...settings.api, ...patch } })} />}
    {settings.kind === "agent" && <AgentFields command={settings.agent.command} argumentList={settings.agent.arguments} onChange={(patch) => onChange({ agent: { ...settings.agent, ...patch } })} />}

    {status && <p className={`provider-summary ${status.configured ? "ready" : ""}`}><ShieldCheck size={14} />{status.summary}</p>}
    {settings.kind !== "unset" && <div className="provider-test">
      <button type="button" className="secondary-button" disabled={testing || !status?.configured} onClick={() => void test()}>{testing ? <LoaderCircle size={14} className="spin" /> : <Zap size={14} />}{testing ? "Testing..." : "Test this provider"}</button>
      {probe && <span className={probe.reachable ? "probe-ok" : "probe-failed"}>{probe.detail}</span>}
    </div>}
  </>;
}

function LocalFields({ settings, onChange }: { settings: LocalModelSettings; onChange: (patch: Partial<LocalModelSettings>) => void }) {
  const [baseUrl, setBaseUrl] = useDraft(settings.baseUrl);
  const [model, setModel] = useDraft(settings.model);
  const [command, setCommand] = useDraft(settings.command);

  return <div className="provider-fields">
    <label className="setting-select"><span>Endpoint</span><input value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} onBlur={() => onChange({ baseUrl })} placeholder="http://127.0.0.1:11434/v1" /></label>
    <label className="setting-select"><span>Model</span><input value={model} onChange={(event) => setModel(event.target.value)} onBlur={() => onChange({ model })} placeholder="qwen2.5:14b" /></label>
    <label className="toggle-row"><span>Threadbox starts and stops the server</span><input type="checkbox" checked={settings.managed} onChange={(event) => onChange({ managed: event.target.checked })} /></label>
    <p className="setting-note">Leave this off if you already run the server yourself, for example as a system service. Threadbox will then use it and never shut it down.</p>
    {settings.managed && <label className="setting-select"><span>Start command</span><input value={command} onChange={(event) => setCommand(event.target.value)} onBlur={() => onChange({ command })} placeholder="llama-server --port 11434 -m /path/to/model.gguf" /></label>}
    <label className="setting-select"><span>Unload after</span><select value={settings.idleTimeoutMinutes} onChange={(event) => onChange({ idleTimeoutMinutes: Number(event.target.value) })}>{[5, 10, 20, 60].map((minutes) => <option key={minutes} value={minutes}>{minutes} minutes idle</option>)}<option value={0}>Never - keep it loaded</option></select></label>
    <p className="setting-note">A loaded model holds several gigabytes. Unloading it returns all of that to the system, at the cost of the loading time on the next request.</p>
  </div>;
}

function ApiFields({ settings, providersWithKeys, keyDraft, onKeyDraft, onSaveKey, onRemoveKey, onChange }: { settings: ApiModelSettings; providersWithKeys: string[]; keyDraft: string; onKeyDraft: (value: string) => void; onSaveKey: () => void; onRemoveKey: () => void; onChange: (patch: Partial<ApiModelSettings>) => void }) {
  const [model, setModel] = useDraft(settings.model);
  const [baseUrl, setBaseUrl] = useDraft(settings.baseUrl);
  const chosen = apiProviders.find((provider) => provider.value === settings.provider);
  const keyPresent = providersWithKeys.includes(settings.provider);

  return <div className="provider-fields">
    <label className="setting-select"><span>Provider</span><select value={settings.provider} onChange={(event) => onChange({ provider: event.target.value as ApiProvider })}><option value="">Choose a provider</option>{apiProviders.map((provider) => <option key={provider.value} value={provider.value}>{provider.label}{providersWithKeys.includes(provider.value) ? " - key stored" : ""}</option>)}</select></label>
    {settings.provider === "compatible" && <label className="setting-select"><span>Endpoint</span><input value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} onBlur={() => onChange({ baseUrl })} placeholder="https://models.example.com/v1" /></label>}
    {settings.provider && <label className="setting-select"><span>Model</span><input value={model} onChange={(event) => setModel(event.target.value)} onBlur={() => onChange({ model })} placeholder={chosen?.suggestion || "model name"} /></label>}
    {settings.provider && <div className="key-row">
      <label className="setting-select"><span>API key</span><input type="password" value={keyDraft} onChange={(event) => onKeyDraft(event.target.value)} placeholder={keyPresent ? "A key is stored" : "Paste your key"} autoComplete="off" /></label>
      <div className="key-actions">
        <button type="button" className="secondary-button" disabled={!keyDraft.trim()} onClick={onSaveKey}><KeyRound size={14} />Save key</button>
        {keyPresent && <button type="button" className="secondary-button" onClick={onRemoveKey}><Trash2 size={14} />Remove key</button>}
      </div>
    </div>}
    <p className="setting-note">The key is kept in your operating system keyring, not in Threadbox's settings file, and is never shown again. Removing it deletes it.</p>
  </div>;
}

function AgentFields({ command, argumentList, onChange }: { command: string; argumentList: string[]; onChange: (patch: { command?: string; arguments?: string[] }) => void }) {
  const [draftCommand, setDraftCommand] = useDraft(command);
  const [draftArguments, setDraftArguments] = useDraft(argumentList.join(" "));

  return <div className="provider-fields">
    <label className="setting-select"><span>Command</span><input value={draftCommand} onChange={(event) => setDraftCommand(event.target.value)} onBlur={() => onChange({ command: draftCommand.trim() })} placeholder="claude" /></label>
    <label className="setting-select"><span>Arguments</span><input value={draftArguments} onChange={(event) => setDraftArguments(event.target.value)} onBlur={() => onChange({ arguments: draftArguments.split(" ").map((value) => value.trim()).filter(Boolean) })} placeholder="-p" /></label>
    <p className="setting-note">Threadbox runs the tool you already signed in to and reads its output. Your credentials stay inside that tool.</p>
  </div>;
}

/** Both provider choices together, for the first run. */
export function ProviderSetup({ settings, onChange, onError }: { settings: AppSettings; onChange: (patch: Partial<AppSettings>) => void; onError: (message: string) => void }) {
  return <div className="provider-setup">
    <section>
      <h3>Speech recognition</h3>
      <p>Audio never leaves this computer. Pick a model now or download one later from Settings.</p>
      <SpeechProviderSettings settings={settings.speech} onChange={(patch) => onChange({ speech: { ...settings.speech, ...patch } })} onError={onError} />
    </section>
    <section>
      <h3>Analysis</h3>
      <p>Writing meeting notes and pulling out action items needs a language model. Nothing is sent anywhere until you choose one.</p>
      <LanguageModelProviderSettings settings={settings.languageModel} onChange={(patch) => onChange({ languageModel: { ...settings.languageModel, ...patch } })} onError={onError} />
    </section>
  </div>;
}
