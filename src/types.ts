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
export type SpeechProvider = "local" | "openai";

export interface SpeechSettings {
  /** Preselected for a new meeting job. Each queued job keeps its own provider. */
  provider: SpeechProvider;
  /** Used for meetings. Voice notes always use the small model so capture stays fast. */
  model: SpeechModelId;
  /** "auto" for detection per recording, or a language code. */
  language: string;
  terminologyLanguage: string | null;
  /** Timestamp-capable cloud model used for meeting recordings. */
  cloudModel: "whisper-1";
}

export interface SpeechCloudStatus {
  provider: "openai";
  model: string;
  configured: boolean;
  keyPresent: boolean;
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
  googleOauthClientId: string;
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

export type MeetingStatus = "planned" | "recording" | "recorded";

export interface Meeting {
  id: string;
  projectId: string | null;
  title: string;
  status: MeetingStatus;
  scheduledStart: string | null;
  startedAt: string | null;
  endedAt: string | null;
  /** Stereo WAV: microphone on the left, system audio on the right. */
  recordingPath: string | null;
  durationSeconds: number | null;
  createdAt: string;
  updatedAt: string;
  deletedAt: string | null;
}

export interface MeetingInput {
  projectId?: string | null;
  title: string;
  scheduledStart?: string | null;
}

export type ProcessingJobStatus = "queued" | "running" | "completed" | "failed";

export interface ProcessingJob {
  id: string;
  meetingId: string;
  status: ProcessingJobStatus;
  attempts: number;
  error: string | null;
  createdAt: string;
  startedAt: string | null;
  completedAt: string | null;
  updatedAt: string;
}

export interface TranscriptSegment {
  id: string;
  channel: "microphone" | "system";
  startMs: number;
  endMs: number;
  text: string;
  originalText: string;
  sequence: number;
}

export interface MeetingTranscript {
  id: string;
  meetingId: string;
  microphoneLanguage: string;
  systemLanguage: string;
  modelId: string;
  promptVersion: string;
  createdAt: string;
  updatedAt: string;
  segments: TranscriptSegment[];
}

export type MeetingAnalysisItemKind = "decision" | "action_item" | "addressed" | "term_explanation";

export interface MeetingAnalysisItem {
  id: string;
  kind: MeetingAnalysisItemKind;
  title: string;
  text: string;
  startMs: number;
  endMs: number;
  taskId: string | null;
}

export interface MeetingAnalysis {
  id: string;
  meetingId: string;
  notes: string;
  provider: string;
  model: string;
  promptVersion: string;
  createdAt: string;
  updatedAt: string;
  items: MeetingAnalysisItem[];
}

export interface VocabularySet {
  id: string;
  name: string;
  alwaysActive: boolean;
  projectIds: string[];
  termCount: number;
  createdAt: string;
  updatedAt: string;
  deletedAt: string | null;
}

export interface VocabularySetInput {
  name: string;
  alwaysActive?: boolean;
  projectIds?: string[];
}

export interface VocabularyTerm {
  id: string;
  setId: string;
  canonicalForm: string;
  expansion: string | null;
  definition: string | null;
  language: string;
  variants: string[];
  priority: number;
  createdAt: string;
  updatedAt: string;
  deletedAt: string | null;
}

export interface VocabularyTermInput {
  setId: string;
  canonicalForm: string;
  expansion?: string | null;
  definition?: string | null;
  language?: string;
  variants?: string[];
  priority?: number;
}

export interface VocabularyCandidate {
  text: string;
  occurrences: number;
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

export interface OrganizationMember {
  role: string | null;
  person: Person;
}

export type IntegrationCapabilityId = "calendar_read" | "calendar_write" | "calendar_free_busy" | "mail_metadata_read" | "mail_content_read" | "mail_send";

export interface IntegrationConnection {
  id: string;
  organizationId: string;
  provider: string;
  accountIdentifier: string;
  displayName: string;
  status: "connected" | "degraded" | "disconnected";
  createdAt: string;
  updatedAt: string;
  deletedAt: string | null;
}

export interface IntegrationCapability {
  connectionId: string;
  capability: IntegrationCapabilityId;
  status: "granted" | "revoked";
  providerScope: string | null;
  grantedAt: string | null;
  revokedAt: string | null;
  updatedAt: string;
}

export interface IntegrationSnapshot {
  connection: IntegrationConnection;
  capabilities: IntegrationCapability[];
}

export interface ExternalCalendar {
  id: string;
  summary: string;
  primary: boolean;
  accessRole: string;
  timeZone: string | null;
}

export interface ExternalCalendarEvent {
  id: string;
  calendarId: string;
  summary: string;
  description: string;
  start: string;
  end: string;
  allDay: boolean;
  timeZone: string | null;
  htmlLink: string | null;
  conferenceLink: string | null;
  attendees: string[];
  etag: string | null;
}

export interface CalendarEventDraft {
  organizationId: string;
  projectId: string | null;
  connectionId: string;
  calendarId: string;
  summary: string;
  description: string;
  start: string;
  end: string;
  timeZone: string;
  attendees: string[];
  addGoogleMeet: boolean;
}

export interface FindTimeInput {
  connectionId: string;
  calendarIds: string[];
  timeMin: string;
  timeMax: string;
  durationMinutes: number;
  bufferMinutes: number;
  timeZone: string;
  workdayStart: string;
  workdayEnd: string;
}

export interface AvailableSlot {
  start: string;
  end: string;
}
