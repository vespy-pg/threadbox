import { invoke } from "@tauri-apps/api/core";
import { disable as disableAutostart, enable as enableAutostart, isEnabled as isAutostartEnabled } from "@tauri-apps/plugin-autostart";
import { readImage } from "@tauri-apps/plugin-clipboard-manager";
import { setMediaRoot } from "./media";
import type { AppSettings, ApiProvider, LanguageModelStatus, ModelStatus, Organization, OrganizationMember, Person, Project, ProjectDocument, ProjectDocumentInput, ProjectLanguage, ProviderProbe, SpeechModelId, Task, TaskInput, TaskPatch } from "./types";

const inTauri = (): boolean => "__TAURI_INTERNALS__" in window;

const demoKey = "threadbox.demo.tasks";

/** Used when the interface runs in a plain browser, where none of the desktop settings apply. */
const browserSettings: AppSettings = {
  welcomeCompleted: false,
  startAtLogin: true,
  quickCaptureShortcut: "CommandOrControl+Shift+Space",
  overdueRemindersEnabled: true,
  overdueIntervalMinutes: 15,
  stickyRemindersEnabled: true,
  tomorrowReminderTime: "08:30",
  clockFormat: "24h",
  audioInputMode: "microphone",
  taskRetentionDays: 7,
  speech: { model: "small", language: "auto", terminologyLanguage: null },
  languageModel: {
    kind: "unset",
    local: { baseUrl: "http://127.0.0.1:11434/v1", model: "", managed: false, command: "", idleTimeoutMinutes: 10 },
    api: { provider: "", baseUrl: "", model: "" },
    agent: { command: "", arguments: [] },
  },
};

function demoTasks(): Task[] {
  try {
    return (JSON.parse(localStorage.getItem(demoKey) ?? "[]") as Task[]).map((task) => ({
      ...task,
      priority: task.priority ?? "mid",
      deletedAt: task.deletedAt ?? null,
      links: task.links ?? [],
      fileAttachments: task.fileAttachments ?? [],
    }));
  } catch {
    return [];
  }
}

function saveDemo(tasks: Task[]): void {
  localStorage.setItem(demoKey, JSON.stringify(tasks));
}

export const api = {
  async listTasks(): Promise<Task[]> {
    return inTauri() ? invoke<Task[]>("list_tasks") : demoTasks();
  },

  async createTask(input: TaskInput): Promise<Task> {
    if (inTauri()) return invoke<Task>("create_task", { input });
    const now = new Date().toISOString();
    const task: Task = {
      id: crypto.randomUUID(),
      title: input.title,
      notes: input.notes ?? "",
      status: input.status ?? "inbox",
      priority: input.priority ?? "mid",
      sourceType: input.sourceType ?? "manual",
      sourceUrl: input.sourceUrl ?? null,
      sourceLabel: input.sourceLabel ?? null,
      sourceAuthor: input.sourceAuthor ?? null,
      sourceExcerpt: input.sourceExcerpt ?? null,
      projectId: input.projectId ?? null,
      dueAt: input.dueAt ?? null,
      remindAt: input.remindAt ?? null,
      screenshots: input.screenshots ?? (input.screenshotDataUrl ? [input.screenshotDataUrl] : []),
      audioAttachments: input.audioAttachments ?? [],
      links: input.links ?? [],
      fileAttachments: input.fileAttachments ?? [],
      createdAt: now,
      updatedAt: now,
      completedAt: null,
      deletedAt: null,
    };
    saveDemo([task, ...demoTasks()]);
    return task;
  },

  async updateTask(patch: TaskPatch): Promise<Task> {
    if (inTauri()) return invoke<Task>("update_task", { patch });
    const tasks = demoTasks();
    const index = tasks.findIndex((task) => task.id === patch.id);
    if (index < 0) throw new Error("Task not found");
    const current = tasks[index];
    const next: Task = {
      ...current,
      ...patch,
      updatedAt: new Date().toISOString(),
      completedAt: patch.status === "done" ? new Date().toISOString() : patch.status ? null : current.completedAt,
    };
    tasks[index] = next;
    saveDemo(tasks);
    return next;
  },

  async deleteTask(id: string): Promise<void> {
    if (inTauri()) return invoke<void>("delete_task", { id });
    const tasks = demoTasks().map((task) => task.id === id ? { ...task, deletedAt: new Date().toISOString(), dueAt: null, remindAt: null } : task);
    saveDemo(tasks);
  },

  async restoreTask(id: string): Promise<Task> {
    if (inTauri()) return invoke<Task>("restore_task", { id });
    const tasks = demoTasks();
    const index = tasks.findIndex((task) => task.id === id);
    if (index < 0) throw new Error("Task not found");
    tasks[index] = { ...tasks[index], status: "inbox", completedAt: null, deletedAt: null, updatedAt: new Date().toISOString() };
    saveDemo(tasks);
    return tasks[index];
  },

  async deleteTasks(ids: string[], permanently: boolean): Promise<void> {
    if (inTauri()) return invoke<void>("delete_tasks", { ids, permanently });
    const selected = new Set(ids);
    if (permanently) saveDemo(demoTasks().filter((task) => !selected.has(task.id)));
    else {
      const deletedAt = new Date().toISOString();
      saveDemo(demoTasks().map((task) => selected.has(task.id) ? { ...task, deletedAt, dueAt: null, remindAt: null } : task));
    }
  },

  async exportBackup(path: string): Promise<void> {
    if (inTauri()) return invoke<void>("export_backup", { path });
    throw new Error("Backup export is only available in the desktop app.");
  },

  async saveDataUrl(path: string, dataUrl: string): Promise<void> {
    if (!inTauri()) throw new Error("Attachment downloads are only available in the desktop app.");
    await invoke<void>("save_data_url", { path, dataUrl });
  },

  async listOrganizations(): Promise<Organization[]> {
    return inTauri() ? invoke<Organization[]>("list_organizations") : [];
  },

  async createOrganization(input: { name: string; notes?: string; contextSharing?: Organization["contextSharing"] }): Promise<Organization> {
    return invoke<Organization>("create_organization", { input: { notes: "", contextSharing: "isolated", ...input } });
  },

  async updateOrganization(patch: { id: string } & Partial<Organization>): Promise<Organization> {
    return invoke<Organization>("update_organization", { patch });
  },

  async deleteOrganization(id: string): Promise<void> {
    await invoke<void>("delete_organization", { id });
  },

  async listProjects(organizationId?: string): Promise<Project[]> {
    return inTauri() ? invoke<Project[]>("list_projects", { organizationId: organizationId ?? null }) : [];
  },

  async createProject(input: { organizationId: string; name: string; parentId?: string | null; description?: string; contextSharing?: Project["contextSharing"]; language?: string | null; terminologyLanguage?: string | null }): Promise<Project> {
    return invoke<Project>("create_project", { input: { parentId: null, description: "", contextSharing: "inherit", language: null, terminologyLanguage: null, ...input } });
  },

  async updateProject(patch: { id: string } & Partial<Project>): Promise<Project> {
    return invoke<Project>("update_project", { patch });
  },

  async deleteProject(id: string): Promise<void> {
    await invoke<void>("delete_project", { id });
  },

  /** The projects a model may draw context from when working on this one. */
  async projectContextScope(projectId: string): Promise<string[]> {
    return invoke<string[]>("project_context_scope", { projectId });
  },

  async listProjectLinks(projectId: string): Promise<Project[]> {
    return invoke<Project[]>("list_project_links", { projectId });
  },

  async linkProjects(projectId: string, linkedProjectId: string): Promise<void> {
    await invoke<void>("link_projects", { projectId, linkedProjectId });
  },

  async unlinkProjects(projectId: string, linkedProjectId: string): Promise<void> {
    await invoke<void>("unlink_projects", { projectId, linkedProjectId });
  },

  async listPeople(): Promise<Person[]> {
    return inTauri() ? invoke<Person[]>("list_people") : [];
  },

  async createPerson(input: { displayName: string; aliases?: string[]; email?: string | null; notes?: string; isSelf?: boolean }): Promise<Person> {
    return invoke<Person>("create_person", { input: { aliases: [], email: null, notes: "", isSelf: false, ...input } });
  },

  async updatePerson(patch: { id: string } & Partial<Person>): Promise<Person> {
    return invoke<Person>("update_person", { patch });
  },

  async deletePerson(id: string): Promise<void> {
    await invoke<void>("delete_person", { id });
  },

  async listOrganizationPeople(organizationId: string): Promise<OrganizationMember[]> {
    return inTauri() ? invoke<OrganizationMember[]>("list_organization_people", { organizationId }) : [];
  },

  async addOrganizationPerson(organizationId: string, personId: string, role?: string | null): Promise<void> {
    await invoke<void>("add_organization_person", { organizationId, personId, role: role ?? null });
  },

  async removeOrganizationPerson(organizationId: string, personId: string): Promise<void> {
    await invoke<void>("remove_organization_person", { organizationId, personId });
  },

  async listProjectDocuments(projectId: string): Promise<ProjectDocument[]> {
    return inTauri() ? invoke<ProjectDocument[]>("list_project_documents", { projectId }) : [];
  },

  async createProjectDocument(input: ProjectDocumentInput): Promise<ProjectDocument> {
    return invoke<ProjectDocument>("create_project_document", { input });
  },

  async updateProjectDocument(patch: { id: string } & Partial<ProjectDocument>): Promise<ProjectDocument> {
    return invoke<ProjectDocument>("update_project_document", { patch });
  },

  async deleteProjectDocument(id: string): Promise<void> {
    await invoke<void>("delete_project_document", { id });
  },

  /** Fetched once so the interface can resolve the relative references stored in the database. */
  async loadMediaRoot(): Promise<void> {
    if (!inTauri()) return;
    setMediaRoot(await invoke<string>("media_root"));
  },

  async speechModels(): Promise<ModelStatus[]> {
    return inTauri() ? invoke<ModelStatus[]>("speech_models") : [];
  },

  async downloadModel(id: SpeechModelId): Promise<ModelStatus> {
    return invoke<ModelStatus>("download_model", { id });
  },

  /** Without a model this is a voice note, which the desktop side always transcribes with the small model. */
  async transcribeWav(wavBase64: string, model?: SpeechModelId, language?: string): Promise<string> {
    return invoke<string>("transcribe_wav", { wavBase64, model: model ?? null, language: language ?? null });
  },

  /** What will answer the next analysis request. Shown before anything is sent. */
  async languageModelStatus(): Promise<LanguageModelStatus> {
    if (inTauri()) return invoke<LanguageModelStatus>("language_model_status");
    return { kind: "unset", summary: "Providers are only configurable in the desktop app.", configured: false, keyPresent: false, providersWithKeys: [] };
  },

  /** Only ever called from the test button; it is the one place that reaches a provider on purpose. */
  async testLanguageModel(): Promise<ProviderProbe> {
    return invoke<ProviderProbe>("test_language_model");
  },

  /** The key goes straight to the operating system keyring and is never read back. */
  async setLanguageModelKey(provider: ApiProvider, key: string): Promise<LanguageModelStatus> {
    return invoke<LanguageModelStatus>("set_language_model_key", { provider, key });
  },

  async deleteLanguageModelKey(provider: ApiProvider): Promise<LanguageModelStatus> {
    return invoke<LanguageModelStatus>("delete_language_model_key", { provider });
  },

  async projectLanguage(projectId: string): Promise<ProjectLanguage> {
    return invoke<ProjectLanguage>("project_language", { projectId });
  },

  async getSettings(): Promise<AppSettings> {
    if (inTauri()) {
      const settings = await invoke<AppSettings>("get_settings");
      try {
        return { ...settings, startAtLogin: await isAutostartEnabled() };
      } catch {
        return settings;
      }
    }
    return browserSettings;
  },

  async updateSettings(settings: AppSettings): Promise<AppSettings> {
    if (!inTauri()) return settings;
    if (settings.startAtLogin) await enableAutostart();
    else await disableAutostart();
    return invoke<AppSettings>("update_settings", { settings });
  },

  async captureScreenshot(): Promise<string> {
    return invoke<string>("capture_screenshot");
  },

  async testReminderSound(): Promise<void> {
    if (inTauri()) await invoke<void>("test_reminder_sound");
  },

  async warmUpAudio(): Promise<void> {
    if (inTauri()) await invoke<void>("warm_up_audio");
  },

  async playRecording(dataUrl: string): Promise<void> {
    if (!inTauri()) throw new Error("Recording playback is only available in the desktop app.");
    await invoke<void>("play_recording", { source: dataUrl });
  },

  async openMedia(path: string): Promise<void> {
    if (!inTauri() || path.startsWith("data:")) return;
    await invoke<void>("open_media", { path });
  },

  async stopRecordingPlayback(): Promise<void> {
    if (inTauri()) await invoke<void>("stop_recording_playback");
  },

  async readClipboardImage(): Promise<string> {
    if (!inTauri()) throw new Error("Image clipboard access is only available in the desktop app.");
    const image = await readImage();
    try {
      const [{ width, height }, rgba] = await Promise.all([image.size(), image.rgba()]);
      const canvas = document.createElement("canvas");
      canvas.width = width;
      canvas.height = height;
      const context = canvas.getContext("2d");
      if (!context) throw new Error("Could not prepare the clipboard image.");
      context.putImageData(new ImageData(new Uint8ClampedArray(rgba), width, height), 0, 0);
      return canvas.toDataURL("image/png");
    } finally {
      await image.close();
    }
  },
};
