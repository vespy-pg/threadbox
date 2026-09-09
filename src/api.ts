import { invoke } from "@tauri-apps/api/core";
import { disable as disableAutostart, enable as enableAutostart, isEnabled as isAutostartEnabled } from "@tauri-apps/plugin-autostart";
import { readImage } from "@tauri-apps/plugin-clipboard-manager";
import type { AppSettings, ModelStatus, Task, TaskInput, TaskPatch } from "./types";

const inTauri = (): boolean => "__TAURI_INTERNALS__" in window;

const demoKey = "threadbox.demo.tasks";

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

  async modelStatus(): Promise<ModelStatus> {
    if (inTauri()) return invoke<ModelStatus>("model_status");
    return { installed: false, path: "", sizeBytes: null };
  },

  async downloadModel(): Promise<ModelStatus> {
    return invoke<ModelStatus>("download_model");
  },

  async transcribeWav(wavBase64: string): Promise<string> {
    return invoke<string>("transcribe_wav", { wavBase64 });
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
    return { startAtLogin: true, quickCaptureShortcut: "CommandOrControl+Shift+Space", overdueRemindersEnabled: true, overdueIntervalMinutes: 15, stickyRemindersEnabled: true, tomorrowReminderTime: "08:30", clockFormat: "24h", audioInputMode: "microphone", taskRetentionDays: 7 };
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
