export type TaskStatus = "inbox" | "todo" | "waiting" | "done";
export type TaskPriority = "high" | "mid" | "low";
export type SourceType = "manual" | "slack" | "gmail" | "whatsapp" | "web";
export type ClockFormat = "12h" | "24h";
export type AudioInputMode = "microphone" | "system";

export interface AudioAttachment {
  id: string;
  dataUrl: string;
  durationSeconds: number;
  createdAt: string;
}

export interface FileAttachment {
  id: string;
  name: string;
  mimeType: string;
  dataUrl: string;
  sizeBytes: number;
  createdAt: string;
}

export interface Task {
  id: string;
  title: string;
  notes: string;
  status: TaskStatus;
  priority: TaskPriority;
  sourceType: SourceType;
  sourceUrl: string | null;
  sourceLabel: string | null;
  sourceAuthor: string | null;
  sourceExcerpt: string | null;
  dueAt: string | null;
  remindAt: string | null;
  screenshots: string[];
  audioAttachments: AudioAttachment[];
  links: string[];
  fileAttachments: FileAttachment[];
  createdAt: string;
  updatedAt: string;
  completedAt: string | null;
  deletedAt: string | null;
}

export interface TaskInput {
  title: string;
  notes?: string;
  status?: TaskStatus;
  priority?: TaskPriority;
  sourceType?: SourceType;
  sourceUrl?: string | null;
  sourceLabel?: string | null;
  sourceAuthor?: string | null;
  sourceExcerpt?: string | null;
  dueAt?: string | null;
  remindAt?: string | null;
  screenshotDataUrl?: string | null;
  screenshots?: string[];
  audioAttachments?: AudioAttachment[];
  links?: string[];
  fileAttachments?: FileAttachment[];
}

export interface TaskPatch extends Partial<TaskInput> {
  id: string;
}

export type TaskView = "inbox" | "high" | "mid" | "low" | "done" | "deleted";

export interface ModelStatus {
  installed: boolean;
  path: string;
  sizeBytes: number | null;
}

export interface AppSettings {
  startAtLogin: boolean;
  quickCaptureShortcut: string;
  overdueRemindersEnabled: boolean;
  overdueIntervalMinutes: number;
  stickyRemindersEnabled: boolean;
  tomorrowReminderTime: string;
  clockFormat: ClockFormat;
  audioInputMode: AudioInputMode;
  taskRetentionDays: number;
}
