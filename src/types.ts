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
  projectId: string | null;
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
  projectId?: string | null;
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

/** One downloadable speech model, with whether it is already on this machine. */
export interface ModelStatus {
  id: SpeechModelId;
  label: string;
  note: string;
  installed: boolean;
  path: string;
  sizeBytes: number | null;
  approximateBytes: number;
}

export type SpeechModelId = "small" | "medium" | "large";

export interface SpeechSettings {
  /** Used for meetings. Voice notes always use the small model so capture stays fast. */
  model: SpeechModelId;
  /** "auto" for detection per recording, or a language code. */
  language: string;
  terminologyLanguage: string | null;
}

export type LanguageModelKind = "unset" | "local" | "api" | "agent";
export type ApiProvider = "anthropic" | "openai" | "openrouter" | "compatible";

export interface LocalModelSettings {
  baseUrl: string;
  model: string;
  /** Whether Threadbox starts the server itself, rather than using one already running. */
  managed: boolean;
  command: string;
  /** Minutes of inactivity after which a server Threadbox started is stopped. Zero keeps it loaded. */
  idleTimeoutMinutes: number;
}

export interface ApiModelSettings {
  provider: ApiProvider | "";
  /** Only used by "compatible"; the others have fixed endpoints. */
  baseUrl: string;
  model: string;
}

export interface AgentModelSettings {
  command: string;
  arguments: string[];
}

export interface LanguageModelSettings {
  kind: LanguageModelKind;
  local: LocalModelSettings;
  api: ApiModelSettings;
  agent: AgentModelSettings;
}

/** What will answer the next analysis request. Never carries the API key. */
export interface LanguageModelStatus {
  kind: LanguageModelKind;
  summary: string;
  configured: boolean;
  keyPresent: boolean;
  providersWithKeys: string[];
}

export interface ProviderProbe {
  reachable: boolean;
  detail: string;
  models: string[];
}

/** A project's recognition language with every fallback applied. */
export interface ProjectLanguage {
  language: string;
  terminologyLanguage: string;
  /** The project the answer came from, or null when it came from the global setting. */
  inheritedFrom: string | null;
}

export interface AppSettings {
  welcomeCompleted: boolean;
  startAtLogin: boolean;
  quickCaptureShortcut: string;
  overdueRemindersEnabled: boolean;
  overdueIntervalMinutes: number;
  stickyRemindersEnabled: boolean;
  tomorrowReminderTime: string;
  clockFormat: ClockFormat;
  audioInputMode: AudioInputMode;
  taskRetentionDays: number;
  speech: SpeechSettings;
  languageModel: LanguageModelSettings;
}

/** Whether a project or organisation lends its material as context to its neighbours. */
export type ContextSharing = "inherit" | "isolated" | "shared";

export interface Organization {
  id: string;
  name: string;
  notes: string;
  contextSharing: Exclude<ContextSharing, "inherit">;
  createdAt: string;
  updatedAt: string;
  deletedAt: string | null;
}

export interface Project {
  id: string;
  organizationId: string;
  parentId: string | null;
  name: string;
  description: string;
  contextSharing: ContextSharing;
  /** Unset means take the nearest ancestor's language, then the global setting. */
  language: string | null;
  terminologyLanguage: string | null;
  createdAt: string;
  updatedAt: string;
  deletedAt: string | null;
}

export type DocumentKind = "note" | "link" | "file";

export interface ProjectDocument {
  id: string;
  projectId: string;
  kind: DocumentKind;
  title: string;
  body: string;
  url: string | null;
  /** Relative to the media root, resolved through `mediaSource` before it can be displayed. */
  mediaPath: string | null;
  mimeType: string | null;
  sizeBytes: number | null;
  createdAt: string;
  updatedAt: string;
  deletedAt: string | null;
}

export interface ProjectDocumentInput {
  projectId: string;
  kind: DocumentKind;
  title?: string;
  body?: string;
  url?: string | null;
  dataUrl?: string | null;
  fileName?: string | null;
  mimeType?: string | null;
}

export interface Person {
  id: string;
  displayName: string;
  aliases: string[];
  email: string | null;
  notes: string;
  isSelf: boolean;
  createdAt: string;
  updatedAt: string;
  deletedAt: string | null;
}
