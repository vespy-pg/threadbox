import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AlertTriangle,
  ArrowDown,
  Bell,
  BellRing,
  Camera,
  Check,
  CheckCircle2,
  Download,
  ExternalLink,
  FilePlus2,
  Inbox,
  Link2,
  LoaderCircle,
  Menu,
  Mic,
  MicOff,
  Minus,
  MoreHorizontal,
  Paperclip,
  Play,
  Plus,
  Search,
  Settings,
  SlidersHorizontal,
  Square,
  Trash2,
  RotateCcw,
  X,
  Zap,
} from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
import { save } from "@tauri-apps/plugin-dialog";
import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import { openUrl } from "@tauri-apps/plugin-opener";
import { type RecorderResult, WavRecorder } from "./audio";
import { FileAttachmentList, fileToDataUrl, filesToAttachments } from "./attachments";
import { api } from "./api";
import { DateTimePicker } from "./DateTimePicker";
import { formatDueDate, taskMatchesView, toDateTimeLocal } from "./date";
import { mediaSource } from "./media";
import { ProjectSelect, projectPath, useWorkspace, type Workspace } from "./projects";
import { LanguageModelProviderSettings, ProviderSetup, SpeechProviderSettings } from "./providers";
import { DocumentsWorkspace, IntegrationsWorkspace, MeetingsWorkspace, OrganizationOverview, PeopleWorkspace, ProjectOverview, ProjectsWorkspace, WorkspaceNavigation, type WorkbenchArea } from "./workbench";
import type { AppSettings, AudioAttachment, AudioInputMode, FileAttachment, SourceType, Task, TaskInput, TaskPriority, TaskView } from "./types";
import packageJson from "../package.json";

const viewItems: Array<{ id: TaskView; label: string; icon: typeof Inbox }> = [
  { id: "inbox", label: "Open", icon: Inbox },
  { id: "high", label: "High", icon: AlertTriangle },
  { id: "mid", label: "Mid", icon: Minus },
  { id: "low", label: "Low", icon: ArrowDown },
  { id: "done", label: "Completed", icon: CheckCircle2 },
  { id: "deleted", label: "Deleted", icon: Trash2 },
];

const viewTitles: Record<TaskView, string> = {
  inbox: "Open threads",
  high: "High priority",
  mid: "Mid priority",
  low: "Low priority",
  done: "Completed",
  deleted: "Deleted",
};

const inTauri = (): boolean => "__TAURI_INTERNALS__" in window;
const defaultSettings: AppSettings = { welcomeCompleted: false, startAtLogin: true, quickCaptureShortcut: "CommandOrControl+Shift+Space", overdueRemindersEnabled: true, overdueIntervalMinutes: 15, stickyRemindersEnabled: true, tomorrowReminderTime: "08:30", clockFormat: "24h", audioInputMode: "microphone", taskRetentionDays: 7, speech: { model: "small", language: "auto", terminologyLanguage: null }, languageModel: { kind: "unset", local: { baseUrl: "http://127.0.0.1:11434/v1", model: "", managed: false, command: "", idleTimeoutMinutes: 10 }, api: { provider: "", baseUrl: "", model: "" }, agent: { command: "", arguments: [] } } };
const pageSize = 20;
export type ReminderPreset = "15m" | "1h" | "3h" | "6h" | "24h" | "tomorrow";
const reminderPresets: Array<{ value: ReminderPreset; label: string }> = [
  { value: "15m", label: "15 min" },
  { value: "1h", label: "1h" },
  { value: "3h", label: "3h" },
  { value: "6h", label: "6h" },
  { value: "24h", label: "24h" },
  { value: "tomorrow", label: "Tomorrow" },
];
interface AudioTranscription {
  text: string;
  note: string;
}
let shortcutOperation: Promise<void> = Promise.resolve();

function queueShortcutOperation(operation: () => Promise<void>): void {
  shortcutOperation = shortcutOperation.then(operation, operation);
}

export default function App() {
  const [tasks, setTasks] = useState<Task[]>([]);
  const [view, setView] = useState<TaskView>("inbox");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [composerOpen, setComposerOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [area, setArea] = useState<WorkbenchArea>("organization-overview");
  const [activeOrganizationId, setActiveOrganizationId] = useState<string | null>(null);
  const [activeProjectId, setActiveProjectId] = useState<string | null>(null);
  const [welcomeOpen, setWelcomeOpen] = useState(false);
  const [remindersOpen, setRemindersOpen] = useState(false);
  const [stickyReminderOpen, setStickyReminderOpen] = useState(false);
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [settings, setSettings] = useState<AppSettings>(defaultSettings);
  const [transcribingAudioIds, setTranscribingAudioIds] = useState<string[]>([]);
  const [processingTaskCounts, setProcessingTaskCounts] = useState<Record<string, number>>({});
  const [tasksLoaded, setTasksLoaded] = useState(false);
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  const [page, setPage] = useState(1);
  const [listMenuView, setListMenuView] = useState<TaskView | null>(null);
  const [undoCompletion, setUndoCompletion] = useState<{ id: string; previousStatus: Task["status"] } | null>(null);
  const [undoDeletion, setUndoDeletion] = useState<{ id: string; title: string } | null>(null);
  const checkedInitialReminders = useRef(false);
  const checkedInitialWelcome = useRef(false);
  const initializedWorkspaceContext = useRef(false);
  const listMenuRef = useRef<HTMLDivElement>(null);
  const undoTimerRef = useRef<number | null>(null);
  const undoDeletionTimerRef = useRef<number | null>(null);

  const { workspace, reload: reloadWorkspace } = useWorkspace(setError);
  const activeOrganization = workspace.organizations.find((organization) => organization.id === activeOrganizationId) ?? null;
  const activeProject = workspace.projects.find((project) => project.id === activeProjectId) ?? null;

  useEffect(() => {
    if (initializedWorkspaceContext.current || workspace.organizations.length === 0) return;
    initializedWorkspaceContext.current = true;
    setActiveOrganizationId(workspace.organizations[0].id);
    setArea("organization-overview");
  }, [workspace.organizations]);

  useEffect(() => {
    if (activeProject && activeProject.organizationId !== activeOrganizationId) setActiveProjectId(null);
  }, [activeOrganizationId, activeProject]);

  const setAudioTranscribing = useCallback((id: string, active: boolean) => {
    setTranscribingAudioIds((current) => active
      ? current.includes(id) ? current : [...current, id]
      : current.filter((currentId) => currentId !== id));
  }, []);

  const setTaskProcessing = useCallback((id: string, active: boolean) => {
    setProcessingTaskCounts((current) => {
      const nextCount = Math.max(0, (current[id] ?? 0) + (active ? 1 : -1));
      const next = { ...current };
      if (nextCount === 0) delete next[id];
      else next[id] = nextCount;
      return next;
    });
  }, []);

  const refresh = useCallback(async () => {
    try {
      setTasks(await api.listTasks());
      setTasksLoaded(true);
    } catch (reason) {
      setError(String(reason));
    }
  }, []);

  useEffect(() => {
    // Stored media is relative to the media root, so it has to be known before anything is drawn.
    void api.loadMediaRoot().catch(() => undefined).then(refresh);
    const poll = window.setInterval(refresh, 30_000);
    const refreshOnFocus = () => void refresh();
    window.addEventListener("focus", refreshOnFocus);
    return () => {
      window.clearInterval(poll);
      window.removeEventListener("focus", refreshOnFocus);
    };
  }, [refresh]);

  useEffect(() => {
    void api.getSettings().then(setSettings).catch((reason) => setError(String(reason))).finally(() => setSettingsLoaded(true));
  }, []);

  useEffect(() => {
    if (!settingsLoaded || checkedInitialWelcome.current) return;
    checkedInitialWelcome.current = true;
    setWelcomeOpen(!settings.welcomeCompleted);
  }, [settings.welcomeCompleted, settingsLoaded]);

  useEffect(() => {
    const warmup = window.setTimeout(() => void api.warmUpAudio().catch(() => undefined), 350);
    return () => window.clearTimeout(warmup);
  }, []);

  useEffect(() => {
    if (!inTauri()) return;
    let disposed = false;
    const stopListening: Array<() => void> = [];
    void Promise.all([
      listen("open-reminders", () => setRemindersOpen(true)),
      listen("show-sticky-reminder", async () => {
        await refresh();
        if (!disposed) setStickyReminderOpen(true);
      }),
    ]).then((unlisteners) => {
      if (disposed) unlisteners.forEach((unlisten) => unlisten());
      else stopListening.push(...unlisteners);
    });
    return () => {
      disposed = true;
      stopListening.forEach((unlisten) => unlisten());
    };
  }, [refresh]);

  useEffect(() => {
    if (!inTauri()) return;
    let active = true;
    const shortcut = settings.quickCaptureShortcut;
    queueShortcutOperation(async () => {
      if (!active) return;
      try {
        await unregister(shortcut);
      } catch {
        // The shortcut is normally absent on first launch.
      }
      if (!active) return;
      try {
        await register(shortcut, async () => {
          const appWindow = getCurrentWindow();
          await appWindow.show();
          await appWindow.unminimize();
          await appWindow.setAlwaysOnTop(true);
          await appWindow.setFocus();
          if (active) setComposerOpen(true);
          window.setTimeout(() => void appWindow.setAlwaysOnTop(false), 1_200);
        });
      } catch (reason) {
        if (active) setError(`Could not register the quick capture shortcut: ${String(reason)}`);
      }
    });
    return () => {
      active = false;
      queueShortcutOperation(async () => {
        try {
          await unregister(shortcut);
        } catch {
          // The shortcut may already have been released.
        }
      });
    };
  }, [settings.quickCaptureShortcut]);

  useEffect(() => {
    if (!inTauri()) return;
    void isPermissionGranted().then(async (granted) => {
      if (!granted) await requestPermission();
    });
  }, []);

  const selected = tasks.find((task) => task.id === selectedId) ?? null;
  const dueReminders = useMemo(() => dueReminderTasks(tasks), [tasks]);
  const contextTasks = useMemo(() => tasks.filter((task) => activeProject ? task.projectId === activeProject.id : task.projectId === null), [activeProject, tasks]);
  const viewTasks = useMemo(() => sortTasksForView(contextTasks.filter((task) => taskMatchesView(task, view))), [contextTasks, view]);
  const projectTaskCounts = useMemo(() => tasks.reduce<Record<string, number>>((counts, task) => { if (task.projectId && !task.deletedAt && task.status !== "done") counts[task.projectId] = (counts[task.projectId] ?? 0) + 1; return counts; }, {}), [tasks]);
  const filtered = useMemo(() => {
    const query = search.trim().toLocaleLowerCase();
    return viewTasks
      .filter((task) => !query || [task.title, task.notes, task.sourceExcerpt, task.sourceAuthor, task.sourceLabel].some((value) => value?.toLocaleLowerCase().includes(query)));
  }, [viewTasks, search]);
  const pageCount = Math.max(1, Math.ceil(filtered.length / pageSize));
  const visibleTasks = filtered.slice((page - 1) * pageSize, page * pageSize);

  useEffect(() => setPage(1), [activeProjectId, search, view]);
  useEffect(() => setPage((current) => Math.min(current, pageCount)), [pageCount]);

  useEffect(() => {
    if (!listMenuView) return;
    const close = (event: MouseEvent) => {
      if (!listMenuRef.current?.contains(event.target as Node)) setListMenuView(null);
    };
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [listMenuView]);

  useEffect(() => () => {
    if (undoTimerRef.current !== null) window.clearTimeout(undoTimerRef.current);
    if (undoDeletionTimerRef.current !== null) window.clearTimeout(undoDeletionTimerRef.current);
  }, []);

  async function createTask(input: TaskInput): Promise<Task> {
    const task = await api.createTask(input);
    await refresh();
    setSelectedId(task.id);
    setComposerOpen(false);
    return task;
  }

  async function createOrganization() {
    const name = window.prompt("Organisation name");
    if (!name?.trim()) return;
    try {
      const organization = await api.createOrganization({ name: name.trim() });
      await reloadWorkspace();
      setActiveOrganizationId(organization.id);
      setActiveProjectId(null);
      setArea("projects");
    } catch (reason) {
      setError(String(reason));
    }
  }

  function openTask(task: Task) {
    const project = task.projectId ? workspace.projects.find((item) => item.id === task.projectId) : null;
    setActiveProjectId(project?.id ?? null);
    setActiveOrganizationId(project?.organizationId ?? activeOrganizationId);
    setArea("threads");
    setView(task.deletedAt ? "deleted" : task.status === "done" ? "done" : "inbox");
    setSelectedId(task.id);
  }

  async function updateTask(id: string, patch: Partial<TaskInput>) {
    const updated = await api.updateTask({ id, ...patch });
    setTasks((current) => current.map((task) => task.id === id ? updated : task));
  }

  async function toggleTaskCompletion(task: Task) {
    if (task.status === "done") {
      await updateTask(task.id, { status: "todo" });
      return;
    }
    await updateTask(task.id, { status: "done" });
    if (undoTimerRef.current !== null) window.clearTimeout(undoTimerRef.current);
    setUndoCompletion({ id: task.id, previousStatus: task.status });
    undoTimerRef.current = window.setTimeout(() => {
      setUndoCompletion(null);
      undoTimerRef.current = null;
    }, 5_000);
  }

  async function undoCompletedTask() {
    if (!undoCompletion) return;
    if (undoTimerRef.current !== null) window.clearTimeout(undoTimerRef.current);
    await updateTask(undoCompletion.id, { status: undoCompletion.previousStatus });
    setUndoCompletion(null);
    undoTimerRef.current = null;
  }

  async function removeTask(id: string, permanently = false) {
    if (permanently && !window.confirm("Permanently delete this task? This cannot be undone.")) return;
    if (permanently) await api.deleteTasks([id], true);
    else {
      const removedTask = tasks.find((task) => task.id === id);
      await api.deleteTask(id);
      if (undoDeletionTimerRef.current !== null) window.clearTimeout(undoDeletionTimerRef.current);
      setUndoDeletion({ id, title: removedTask?.title ?? "Task" });
      undoDeletionTimerRef.current = window.setTimeout(() => {
        setUndoDeletion(null);
        undoDeletionTimerRef.current = null;
      }, 5_000);
    }
    setSelectedId(null);
    await refresh();
  }

  async function undoDeletedTask() {
    if (!undoDeletion) return;
    if (undoDeletionTimerRef.current !== null) window.clearTimeout(undoDeletionTimerRef.current);
    await restoreTask(undoDeletion.id);
    setUndoDeletion(null);
    undoDeletionTimerRef.current = null;
  }

  async function restoreTask(id: string) {
    const restored = await api.restoreTask(id);
    setTasks((current) => current.map((task) => task.id === id ? restored : task));
    setSelectedId(restored.id);
    setView("inbox");
  }

  async function removeAllInView(targetView: TaskView) {
    const targetTasks = sortTasksForView(contextTasks.filter((task) => taskMatchesView(task, targetView)));
    if (targetTasks.length === 0) return;
    const permanently = targetView === "deleted";
    const action = permanently ? "permanently delete" : "remove";
    setListMenuView(null);
    if (!window.confirm(`${action[0].toUpperCase()}${action.slice(1)} all ${targetTasks.length} tasks in ${viewTitles[targetView]}?`)) return;
    await api.deleteTasks(targetTasks.map((task) => task.id), permanently);
    setSelectedId(null);
    await refresh();
  }

  const releaseStickyReminder = useCallback(async () => {
    setStickyReminderOpen(false);
    if (inTauri()) await getCurrentWindow().setAlwaysOnTop(false);
  }, []);

  useEffect(() => {
    if (stickyReminderOpen && dueReminders.length === 0) void releaseStickyReminder();
  }, [dueReminders.length, releaseStickyReminder, stickyReminderOpen]);

  useEffect(() => {
    if (!tasksLoaded || !settingsLoaded || checkedInitialReminders.current) return;
    checkedInitialReminders.current = true;
    if (!settings.overdueRemindersEnabled || !settings.stickyRemindersEnabled || dueReminders.length === 0) return;
    setStickyReminderOpen(true);
    if (inTauri()) void getCurrentWindow().setAlwaysOnTop(true);
  }, [dueReminders.length, settings.overdueRemindersEnabled, settings.stickyRemindersEnabled, settingsLoaded, tasksLoaded]);

  return (
    <main className="app-shell">
      <aside className={`sidebar ${sidebarOpen ? "sidebar-open" : ""}`}>
        <div className="brand"><span className="brand-mark">T</span><span>Threadbox</span></div>
        <button className="primary-button capture-button" onClick={() => setComposerOpen(true)}><Plus size={18} />Quick capture <kbd>{shortcutLabel(settings.quickCaptureShortcut)}</kbd></button>
        <WorkspaceNavigation workspace={workspace} organization={activeOrganization} project={activeProject} area={area} inboxCount={tasks.filter((task) => task.projectId === null && !task.deletedAt && task.status !== "done").length} projectTaskCounts={projectTaskCounts} onOrganization={(id) => { setActiveOrganizationId(id); setActiveProjectId(null); setArea(id ? "organization-overview" : "threads"); setSidebarOpen(false); }} onCreateOrganization={() => void createOrganization()} onProject={(id) => { setActiveProjectId(id); const project = workspace.projects.find((item) => item.id === id); if (project) setActiveOrganizationId(project.organizationId); setSidebarOpen(false); }} onArea={(next) => { setArea(next); setSelectedId(null); setSidebarOpen(false); }} />
        <button className="nav-item reminder-link" onClick={() => setRemindersOpen(true)}><Bell size={17} /><span>Reminders</span>{reminderTasks(tasks).length > 0 && <span className="count">{reminderTasks(tasks).length}</span>}</button>
        <button className="nav-item settings-link" onClick={() => setSettingsOpen(true)}><Settings size={17} /><span>Settings</span></button>
        <span className="app-version">Threadbox {packageJson.version}</span>
      </aside>
      <section className="workspace-main">
        <button className="icon-button mobile-menu workspace-mobile-menu" aria-label="Open navigation" onClick={() => setSidebarOpen((value) => !value)}><Menu size={20} /></button>
        {area === "threads" && <div className="thread-workspace"><section className="task-column"><header className="column-header"><button className="icon-button mobile-menu" aria-label="Open navigation" onClick={() => setSidebarOpen((value) => !value)}><Menu size={20} /></button><div className="list-title-menu"><p className="eyebrow">{activeProject ? projectPath(activeProject, workspace) : "Unassigned work"}</p><h1>{activeProject ? "Threads" : "Inbox"}</h1></div><button className="icon-button" aria-label="Create thread" onClick={() => setComposerOpen(true)}><Plus /></button></header><div className="thread-filters">{viewItems.map(({ id, label, icon: Icon }) => { const count = contextTasks.filter((task) => taskMatchesView(task, id)).length; return <div key={id} className="thread-filter-menu" ref={listMenuView === id ? listMenuRef : undefined}><button className={view === id ? "thread-filter active" : "thread-filter"} onClick={() => { setView(id); setListMenuView(null); }} onContextMenu={(event) => { event.preventDefault(); setListMenuView(id); }}><Icon size={14} />{label}{count > 0 && <span>{count}</span>}</button>{listMenuView === id && <div className="list-context-menu"><button disabled={count === 0} onClick={() => void removeAllInView(id)}><Trash2 size={14} />{id === "deleted" ? "Delete all permanently" : "Clear this list"}</button></div>}</div>; })}</div><label className="search-box"><Search size={17} /><input value={search} onChange={(event) => setSearch(event.target.value)} placeholder="Search threads" /></label><div className="task-list">{visibleTasks.map((task) => <TaskRow key={task.id} task={task} projectLabel={taskProjectLabel(task, workspace)} selected={task.id === selectedId} processing={Boolean(processingTaskCounts[task.id])} onSelect={() => setSelectedId(task.id)} onToggle={() => toggleTaskCompletion(task)} onRestore={() => restoreTask(task.id)} />)}{filtered.length === 0 && <EmptyState view={view} retentionDays={settings.taskRetentionDays} onCreate={() => setComposerOpen(true)} />}</div>{filtered.length > pageSize && <Pagination page={page} pageCount={pageCount} itemCount={filtered.length} onPage={setPage} />}</section><section className={`details-column ${selected ? "details-visible" : ""}`}>{selected ? <TaskDetails key={selected.id} task={selected} workspace={workspace} clockFormat={settings.clockFormat} audioInputMode={settings.audioInputMode} tomorrowReminderTime={settings.tomorrowReminderTime} transcribingAudioIds={transcribingAudioIds} onAudioTranscribing={setAudioTranscribing} onTaskProcessing={setTaskProcessing} onClose={() => setSelectedId(null)} onUpdate={(patch) => updateTask(selected.id, patch)} onRestore={() => restoreTask(selected.id)} onDelete={() => removeTask(selected.id, Boolean(selected.deletedAt))} onError={setError} /> : <div className="details-placeholder"><div className="placeholder-icon"><Check size={24} /></div><h2>Pick a thread</h2><p>Its task, context, reminders and attachments will appear here.</p></div>}</section></div>}
        {area === "organization-overview" && activeOrganization && <OrganizationOverview organization={activeOrganization} workspace={workspace} tasks={tasks} onProject={(project) => { setActiveProjectId(project.id); setArea("project-overview"); }} onPeople={() => setArea("people")} />}
        {area === "project-overview" && activeProject && <ProjectOverview project={activeProject} workspace={workspace} tasks={tasks} onArea={setArea} />}
        {area === "people" && activeOrganization && <PeopleWorkspace organization={activeOrganization} onError={setError} />}
        {area === "projects" && activeOrganization && <ProjectsWorkspace organization={activeOrganization} workspace={workspace} activeProject={activeProject} onReload={reloadWorkspace} onProject={(project) => setActiveProjectId(project.id)} onError={setError} />}
        {area === "meetings" && activeProject && activeOrganization && <MeetingsWorkspace project={activeProject} organization={activeOrganization} onIntegrations={() => { setActiveProjectId(null); setArea("integrations"); }} />}
        {area === "documents" && activeProject && <DocumentsWorkspace project={activeProject} workspace={workspace} onReload={reloadWorkspace} onError={setError} />}
        {area === "integrations" && activeOrganization && <IntegrationsWorkspace organization={activeOrganization} />}
        {area !== "threads" && !activeOrganization && <section className="workspace-page workspace-start"><div className="module-icon"><Plus size={24} /></div><p className="eyebrow">Start with context</p><h1>Create your first organisation</h1><p>An organisation keeps its projects, people, meetings and documents together without mixing client contexts.</p><button className="primary-button" onClick={() => void createOrganization()}>Create organisation</button></section>}
      </section>

      {composerOpen && <Composer workspace={workspace} defaultProjectId={activeProject?.id ?? null} clockFormat={settings.clockFormat} audioInputMode={settings.audioInputMode} tomorrowReminderTime={settings.tomorrowReminderTime} transcribingAudioIds={transcribingAudioIds} onAudioTranscribing={setAudioTranscribing} onTaskProcessing={setTaskProcessing} onClose={() => setComposerOpen(false)} onSubmit={createTask} onTaskUpdate={updateTask} onError={setError} />}
      {settingsOpen && <SettingsDialog settings={settings} onSettings={setSettings} onClose={() => { setSettingsOpen(false); if (!settings.welcomeCompleted) setWelcomeOpen(true); }} onError={setError} />}
      {welcomeOpen && <WelcomeDialog shortcut={settings.quickCaptureShortcut} settings={settings} onSettings={setSettings} onError={setError} onComplete={() => { void api.updateSettings({ ...settings, welcomeCompleted: true }).then((updated) => { setSettings(updated); setWelcomeOpen(false); }).catch((reason) => setError(String(reason))); }} />}
      {remindersOpen && <ReminderCenter tasks={reminderTasks(tasks)} tomorrowReminderTime={settings.tomorrowReminderTime} onClose={() => setRemindersOpen(false)} onOpen={(task) => { openTask(task); setRemindersOpen(false); }} onDone={(task) => updateTask(task.id, { status: "done" })} onSnooze={(task, preset) => updateTask(task.id, { remindAt: reminderDate(preset, settings.tomorrowReminderTime).toISOString() })} onSnoozeAll={(items, preset) => Promise.all(items.map((task) => updateTask(task.id, { remindAt: reminderDate(preset, settings.tomorrowReminderTime).toISOString() }))).then(() => undefined)} />}
      {stickyReminderOpen && dueReminders.length > 0 && <StickyReminder tasks={dueReminders} tomorrowReminderTime={settings.tomorrowReminderTime} onOpen={(task) => { openTask(task); void releaseStickyReminder(); }} onDone={(task) => updateTask(task.id, { status: "done" })} onSnooze={(task, preset) => updateTask(task.id, { remindAt: reminderDate(preset, settings.tomorrowReminderTime).toISOString() })} onSnoozeAll={(items, preset) => Promise.all(items.map((task) => updateTask(task.id, { remindAt: reminderDate(preset, settings.tomorrowReminderTime).toISOString() }))).then(() => undefined)} />}
      {error && <div className="toast error-toast"><span>{error}</span><button onClick={() => setError(null)}><X size={16} /></button></div>}
      {undoCompletion && <div className="toast undo-toast"><span>Task completed</span><button onClick={() => void undoCompletedTask()}>Undo</button></div>}
      {undoDeletion && <div className="toast undo-toast"><span>{undoDeletion.title} deleted</span><button onClick={() => void undoDeletedTask()}>Undo</button></div>}
    </main>
  );
}

function TaskRow({ task, projectLabel, selected, processing, onSelect, onToggle, onRestore }: { task: Task; projectLabel: string | null; selected: boolean; processing: boolean; onSelect: () => void; onToggle: () => void; onRestore: () => void }) {
  const preview = task.notes.replace(/\s+/g, " ").trim();
  return <article className={`task-row priority-${task.priority} ${selected ? "selected" : ""} ${processing ? "processing" : ""}`} onClick={onSelect}>
    {task.deletedAt ? <button className="restore-task-button" title="Restore task" aria-label="Restore task" onClick={(event) => { event.stopPropagation(); onRestore(); }}><RotateCcw size={15} /></button> : <input className="task-checkbox" type="checkbox" checked={task.status === "done"} aria-label={task.status === "done" ? "Reopen task" : "Complete task"} onClick={(event) => event.stopPropagation()} onChange={onToggle} />}
    <div className="task-row-body"><h3>{task.title}</h3>{preview && <p className="task-preview">{preview}</p>}<div className="task-meta"><span className={`priority-badge ${task.priority}`}>{task.priority}</span>{projectLabel && <span className="project-badge" title={projectLabel}>{projectLabel.split(" / ").slice(-1)[0]}</span>}{task.sourceType !== "manual" && <span className={`source-badge ${task.sourceType}`}>{task.sourceType}</span>}<span className={task.dueAt && new Date(task.dueAt) < new Date() ? "overdue" : ""}>{task.deletedAt ? `Deleted ${formatDueDate(task.deletedAt)}` : formatDueDate(task.dueAt)}</span></div></div>
    <div className="task-row-media">{processing && <span className="task-processing" title="Background processing"><LoaderCircle size={15} /></span>}{(task.screenshots.length > 0 || task.audioAttachments.length > 0 || task.fileAttachments.length > 0) && <Paperclip size={15} className="muted-icon" />}</div>
  </article>;
}

function Pagination({ page, pageCount, itemCount, onPage }: { page: number; pageCount: number; itemCount: number; onPage: (page: number) => void }) {
  return <nav className="pagination" aria-label="Task pages"><button type="button" disabled={page === 1} onClick={() => onPage(page - 1)}>Previous</button><span>{page} / {pageCount} ({itemCount})</span><button type="button" disabled={page === pageCount} onClick={() => onPage(page + 1)}>Next</button></nav>;
}

function EmptyState({ view, retentionDays, onCreate }: { view: TaskView; retentionDays: number; onCreate: () => void }) {
  return <div className="empty-state"><div className="empty-orbit"><CheckCircle2 size={28} /></div><h2>{view === "done" ? "Nothing completed yet" : view === "deleted" ? "Deleted is empty" : "You are clear here"}</h2><p>{view === "deleted" ? `Removed tasks and their media are deleted permanently after ${retentionDays} days.` : "Capture a thought, message or follow-up before it slips away."}</p>{view !== "deleted" && <button className="secondary-button" onClick={onCreate}><Plus size={16} />Add a task</button>}</div>;
}

function LinkEditor({ links, onAdd, onRemove, onError }: { links: string[]; onAdd: (link: string) => void; onRemove: (link: string) => void; onError: (error: string) => void }) {
  const [input, setInput] = useState("");

  function addLink() {
    if (!input.trim()) return;
    try {
      const link = normalizeLink(input);
      onAdd(link);
      setInput("");
    } catch (reason) {
      onError(String(reason));
    }
  }

  function openLink(event: React.MouseEvent<HTMLAnchorElement>, link: string) {
    if (!inTauri()) return;
    event.preventDefault();
    void openUrl(link).catch((reason) => onError(String(reason)));
  }

  return <section className="link-editor"><label><span>Links</span><div className="link-entry"><div className="input-with-icon"><Link2 size={16} /><input type="url" value={input} onChange={(event) => setInput(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter") { event.preventDefault(); addLink(); } }} placeholder="Paste a URL" /></div><button type="button" className="secondary-button" onClick={addLink} disabled={!input.trim()}><Plus size={15} />Add</button></div></label>{links.length > 0 && <div className="link-list">{links.map((link) => <div className="link-item" key={link}><a className="link-open" href={link} target="_blank" rel="noreferrer" onClick={(event) => openLink(event, link)} title={link}><ExternalLink size={14} /><span>{link}</span></a><button type="button" className="remove-attachment" title="Remove link" onClick={() => onRemove(link)}><X size={14} /></button></div>)}</div>}</section>;
}

function Composer({ workspace, defaultProjectId, clockFormat, audioInputMode, tomorrowReminderTime, transcribingAudioIds, onAudioTranscribing, onTaskProcessing, onClose, onSubmit, onTaskUpdate, onError }: { workspace: Workspace; defaultProjectId: string | null; clockFormat: AppSettings["clockFormat"]; audioInputMode: AudioInputMode; tomorrowReminderTime: string; transcribingAudioIds: string[]; onAudioTranscribing: (id: string, active: boolean) => void; onTaskProcessing: (id: string, active: boolean) => void; onClose: () => void; onSubmit: (input: TaskInput) => Promise<Task>; onTaskUpdate: (id: string, patch: Partial<TaskInput>) => Promise<void>; onError: (error: string) => void }) {
  const [title, setTitle] = useState("");
  const [projectId, setProjectId] = useState<string | null>(defaultProjectId);
  const [notes, setNotes] = useState("");
  const [dueAt, setDueAt] = useState("");
  const [sourceType, setSourceType] = useState<SourceType>("manual");
  const [links, setLinks] = useState<string[]>([]);
  const [screenshots, setScreenshots] = useState<string[]>([]);
  const [audioAttachments, setAudioAttachments] = useState<AudioAttachment[]>([]);
  const [fileAttachments, setFileAttachments] = useState<FileAttachment[]>([]);
  const [recording, setRecording] = useState(false);
  const [recordingBusy, setRecordingBusy] = useState(false);
  const [selectedAudioInput, setSelectedAudioInput] = useState<AudioInputMode>(audioInputMode);
  const recorder = useRef<WavRecorder | null>(null);
  const transcriptionJobs = useRef<Array<Promise<AudioTranscription | null>>>([]);
  const titleRef = useRef<HTMLInputElement>(null);
  const fileInput = useRef<HTMLInputElement>(null);

  useEffect(() => titleRef.current?.focus(), []);

  useEffect(() => {
    const handlePaste = (event: ClipboardEvent) => {
      const image = Array.from(event.clipboardData?.items ?? []).find((item) => item.type.startsWith("image/"));
      const file = image?.getAsFile() ?? Array.from(event.clipboardData?.files ?? []).find((item) => item.type.startsWith("image/"));
      if (file) {
        event.preventDefault();
        void fileToDataUrl(file).then((dataUrl) => setScreenshots((current) => [...current, dataUrl])).catch((reason) => onError(String(reason)));
        return;
      }
      void api.readClipboardImage().then((dataUrl) => setScreenshots((current) => [...current, dataUrl])).catch(() => undefined);
    };
    window.addEventListener("paste", handlePaste, true);
    return () => window.removeEventListener("paste", handlePaste, true);
  }, [onError]);

  function startTranscription(result: RecorderResult, attachment: AudioAttachment): void {
    const recordedAt = new Date(attachment.createdAt);
    onAudioTranscribing(attachment.id, true);
    const job = api.transcribeWav(result.base64)
        .then((transcript) => {
          const text = transcript.trim();
          if (!text) return null;
          const transcription = { text, note: timestampedTranscript(text, recordedAt) };
          setNotes((current) => appendTranscriptNotes(current, [transcription.note]));
          setTitle((current) => isGeneratedTaskTitle(current) ? transcriptTitle(text) : current);
          return transcription;
        })
        .catch((reason) => { onError(String(reason)); return null; })
        .finally(() => onAudioTranscribing(attachment.id, false));
    transcriptionJobs.current.push(job);
  }

  async function finishRecording(): Promise<AudioAttachment> {
    if (!recorder.current) throw new Error("No audio recording is active.");
    const result = await recorder.current.stop();
    setRecording(false);
    recorder.current = null;
    const attachment = recordingAttachment(result);
    setAudioAttachments((current) => [...current, attachment]);
    startTranscription(result, attachment);
    return attachment;
  }

  async function finishRecordingForTask(activeRecorder: WavRecorder, taskId: string): Promise<void> {
    let attachment: AudioAttachment | null = null;
    onTaskProcessing(taskId, true);
    try {
      const result = await activeRecorder.stop();
      attachment = recordingAttachment(result);
      onAudioTranscribing(attachment.id, true);

      const latestTask = (await api.listTasks()).find((item) => item.id === taskId);
      if (!latestTask) throw new Error("The new task could not be found after recording.");
      await onTaskUpdate(taskId, { audioAttachments: [...latestTask.audioAttachments, attachment] });

      const text = (await api.transcribeWav(result.base64)).trim();
      if (!text) return;
      const currentTask = (await api.listTasks()).find((item) => item.id === taskId);
      if (!currentTask) return;
      const nextTitle = isGeneratedTaskTitle(currentTask.title) ? transcriptTitle(text) : currentTask.title;
      const nextNotes = appendTranscriptNotes(currentTask.notes, [timestampedTranscript(text, new Date(attachment.createdAt))]);
      await onTaskUpdate(taskId, { title: nextTitle, notes: nextNotes });
    } catch (reason) {
      onError(String(reason));
    } finally {
      if (attachment) onAudioTranscribing(attachment.id, false);
      onTaskProcessing(taskId, false);
    }
  }

  async function toggleRecording() {
    if (recordingBusy) return;
    setRecordingBusy(true);
    try {
      if (!recording) {
        setRecording(true);
        recorder.current = new WavRecorder();
        await recorder.current.start(selectedAudioInput);
      } else {
        await finishRecording();
      }
    } catch (reason) {
      setRecording(false);
      onError(String(reason));
    } finally {
      setRecordingBusy(false);
    }
  }

  async function takeScreenshot() {
    const appWindow = getCurrentWindow();
    try {
      await appWindow.hide();
      const screenshot = await api.captureScreenshot();
      setScreenshots((current) => [...current, screenshot]);
    } catch (reason) {
      if (!String(reason).toLocaleLowerCase().includes("cancel")) onError(String(reason));
    } finally {
      await appWindow.show();
      await appWindow.unminimize();
      await appWindow.setAlwaysOnTop(true);
      await appWindow.setFocus();
      window.setTimeout(() => void appWindow.setAlwaysOnTop(false), 1_200);
    }
  }

  async function addFiles(files: FileList | null) {
    if (!files?.length) return;
    try {
      const added = await filesToAttachments(files);
      setFileAttachments((current) => [...current, ...added]);
    } catch (reason) {
      onError(String(reason));
    } finally {
      if (fileInput.current) fileInput.current.value = "";
    }
  }

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    if (recordingBusy) return;
    setRecordingBusy(true);
    let activeRecorder: WavRecorder | null = null;
    try {
      if (recording) {
        activeRecorder = recorder.current;
        recorder.current = null;
        setRecording(false);
      }
      const due = dueAt ? new Date(dueAt).toISOString() : null;
      const baseNotes = notes.trim();
      const submittedTitle = taskTitle(title, baseNotes);
      const task = await onSubmit({ title: submittedTitle, notes: baseNotes, status: "inbox", projectId, sourceType, sourceUrl: null, links, dueAt: due, remindAt: due, screenshots, audioAttachments, fileAttachments });
      if (activeRecorder) {
        void finishRecordingForTask(activeRecorder, task.id);
        activeRecorder = null;
      }
      const jobs = [...transcriptionJobs.current];
      if (jobs.length > 0) {
        onTaskProcessing(task.id, true);
        void Promise.all(jobs).then(async (transcripts) => {
          const additions = transcripts.filter((transcript): transcript is AudioTranscription => transcript !== null);
          if (additions.length > 0) {
            const latestTask = (await api.listTasks()).find((item) => item.id === task.id);
            const currentTitle = latestTask?.title ?? submittedTitle;
            const currentNotes = latestTask?.notes ?? baseNotes;
            const nextTitle = isGeneratedTaskTitle(currentTitle) ? transcriptTitle(additions[0].text) : currentTitle;
            const nextNotes = appendTranscriptNotes(currentNotes, additions.map((transcript) => transcript.note));
            if (nextTitle !== currentTitle || nextNotes !== currentNotes) {
              await onTaskUpdate(task.id, { title: nextTitle, notes: nextNotes });
            }
          }
        }).catch((reason) => onError(String(reason))).finally(() => onTaskProcessing(task.id, false));
      }
    } catch (reason) {
      onError(String(reason));
      if (activeRecorder) void activeRecorder.stop().catch(() => undefined);
    } finally {
      setRecordingBusy(false);
    }
  }

  return <div className="modal-backdrop" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
    <form className="composer" onSubmit={submit}>
      <div className="dialog-header"><div><p className="eyebrow">Capture</p><h2>Add to Threadbox</h2></div><button type="button" className="icon-button" onClick={onClose}><X /></button></div>
      <div className="title-capture"><input ref={titleRef} value={title} onChange={(event) => setTitle(event.target.value)} placeholder="Task title (optional)" /><button type="button" className="record-button camera-button" title="Take a system screenshot" onClick={takeScreenshot} disabled={recording || recordingBusy}><Camera /></button><button type="button" className={`record-button ${recording ? "recording" : ""}`} title={recording ? "Stop recording" : "Record an audio note"} onClick={toggleRecording} disabled={recordingBusy}>{recording ? <MicOff /> : <Mic />}</button></div>
      <AudioInputSelect value={selectedAudioInput} disabled={recording || recordingBusy} onChange={setSelectedAudioInput} />
      {transcribingAudioIds.length > 0 && <p className="field-help">Transcribing audio in the background. You can add the task now.</p>}
      <div className="notes-capture"><textarea value={notes} onChange={(event) => setNotes(event.target.value)} placeholder="Notes or context" rows={3} /></div>
      <div className="form-grid"><label><span>Due and remind</span><DateTimePicker value={dueAt} onChange={setDueAt} clockFormat={clockFormat} /></label><label><span>Source</span><select value={sourceType} onChange={(event) => setSourceType(event.target.value as SourceType)}><option value="manual">Manual</option><option value="slack">Slack</option><option value="gmail">Gmail</option><option value="whatsapp">WhatsApp</option><option value="web">Web</option></select></label></div>
      {workspace.projects.length > 0 && <label className="composer-project"><span>Project</span><ProjectSelect value={projectId} workspace={workspace} onChange={setProjectId} /></label>}
      <ReminderPresetButtons onChoose={(preset) => setDueAt(toLocalDateTime(reminderDate(preset, tomorrowReminderTime)))} />
      <LinkEditor links={links} onAdd={(link) => setLinks((current) => current.includes(link) ? current : [...current, link])} onRemove={(link) => setLinks((current) => current.filter((item) => item !== link))} onError={onError} />
      <div className={`drop-zone ${screenshots.length ? "has-image" : ""}`} tabIndex={0}>{screenshots.length ? <div className="screenshot-grid">{screenshots.map((screenshot, index) => <div className="screenshot-thumb" key={`${screenshot.slice(-24)}-${index}`}><img src={mediaSource(screenshot)} alt={`Screenshot ${index + 1}`} /><button type="button" className="download-image" title="Download screenshot" onClick={() => void downloadDataUrl(`screenshot-${index + 1}.png`, screenshot, onError)}><Download size={14} /></button><button type="button" className="remove-image" onClick={() => setScreenshots((current) => current.filter((_, itemIndex) => itemIndex !== index))}><X size={15} /></button></div>)}</div> : <><Paperclip size={19} /><span>Paste one or more screenshots anywhere in this window</span></>}</div>
      <div className="composer-file-action"><button type="button" className="secondary-button" onClick={() => fileInput.current?.click()}><FilePlus2 size={15} />Add file</button><span>Any file type, up to 5 MB per file</span><input ref={fileInput} className="hidden-file-input" type="file" multiple onChange={(event) => void addFiles(event.target.files)} /></div>
      {fileAttachments.length > 0 && <FileAttachmentList attachments={fileAttachments} onOpen={() => onError("Add the task before opening an attachment in a system application.")} onDownload={(attachment) => void downloadDataUrl(attachment.name, attachment.dataUrl, onError)} onRemove={(id) => setFileAttachments((current) => current.filter((attachment) => attachment.id !== id))} />}
      {audioAttachments.length > 0 && <AttachmentAudioList attachments={audioAttachments} transcribingIds={transcribingAudioIds} onError={onError} onDownload={(attachment, index) => void downloadDataUrl(`recording-${index + 1}.wav`, attachment.dataUrl, onError)} onRemove={(id) => setAudioAttachments((current) => current.filter((attachment) => attachment.id !== id))} />}
      <div className="dialog-actions"><button type="button" className="text-button" onClick={onClose}>Cancel</button><button className="primary-button" disabled={recordingBusy}><Plus size={17} />{recording ? "Stop and add task" : "Add task"}</button></div>
    </form>
  </div>;
}

function TaskDetails({ task, workspace, clockFormat, audioInputMode, tomorrowReminderTime, transcribingAudioIds, onAudioTranscribing, onTaskProcessing, onClose, onUpdate, onRestore, onDelete, onError }: { task: Task; workspace: Workspace; clockFormat: AppSettings["clockFormat"]; audioInputMode: AudioInputMode; tomorrowReminderTime: string; transcribingAudioIds: string[]; onAudioTranscribing: (id: string, active: boolean) => void; onTaskProcessing: (id: string, active: boolean) => void; onClose: () => void; onUpdate: (patch: Partial<TaskInput>) => Promise<void>; onRestore: () => Promise<void>; onDelete: () => Promise<void>; onError: (error: string) => void }) {
  const [title, setTitle] = useState(task.title);
  const [notes, setNotes] = useState(task.notes);
  const [priority, setPriority] = useState<TaskPriority>(task.priority);
  const [dueAt, setDueAt] = useState(toDateTimeLocal(task.dueAt));
  const [sourceType, setSourceType] = useState(task.sourceType);
  const [sourceUrl, setSourceUrl] = useState(task.sourceUrl ?? "");
  const [screenshots, setScreenshots] = useState(task.screenshots);
  const [audioAttachments, setAudioAttachments] = useState(task.audioAttachments);
  const [links, setLinks] = useState(task.links);
  const [fileAttachments, setFileAttachments] = useState(task.fileAttachments);
  const [recording, setRecording] = useState(false);
  const [recordingBusy, setRecordingBusy] = useState(false);
  const [selectedAudioInput, setSelectedAudioInput] = useState<AudioInputMode>(audioInputMode);
  const [editVersion, setEditVersion] = useState(0);
  const [saveState, setSaveState] = useState<"idle" | "saving" | "saved" | "error">("idle");
  const [moreOpen, setMoreOpen] = useState(false);
  const activeTaskId = useRef(task.id);
  const latestEditVersion = useRef(0);
  const savedEditVersion = useRef(0);
  const onUpdateRef = useRef(onUpdate);
  const onErrorRef = useRef(onError);
  const latestPatch = useRef<Partial<TaskInput>>({});
  const fileInput = useRef<HTMLInputElement>(null);
  const recorder = useRef<WavRecorder | null>(null);
  const moreActionsRef = useRef<HTMLDivElement>(null);
  latestPatch.current = {
    title: taskTitle(title, notes),
    notes,
    priority,
    dueAt: dueAt ? new Date(dueAt).toISOString() : null,
    remindAt: dueAt ? new Date(dueAt).toISOString() : null,
    sourceType,
    sourceUrl: sourceUrl.trim() || null,
    screenshots,
    audioAttachments,
    links,
    fileAttachments,
  };

  useEffect(() => {
    onUpdateRef.current = onUpdate;
    onErrorRef.current = onError;
  }, [onError, onUpdate]);

  useEffect(() => {
    activeTaskId.current = task.id;
    setTitle(task.title);
    setNotes(task.notes);
    setPriority(task.priority);
    setDueAt(toDateTimeLocal(task.dueAt));
    setSourceType(task.sourceType);
    setSourceUrl(task.sourceUrl ?? "");
    setScreenshots(task.screenshots);
    setAudioAttachments(task.audioAttachments);
    setLinks(task.links);
    setFileAttachments(task.fileAttachments);
    setEditVersion(0);
    latestEditVersion.current = 0;
    savedEditVersion.current = 0;
    setSaveState("idle");
  }, [task.id]);

  useEffect(() => {
    if (activeTaskId.current !== task.id || saveState === "saving" || saveState === "error") return;
    setTitle(task.title);
    setNotes(task.notes);
    setPriority(task.priority);
    setDueAt(toDateTimeLocal(task.dueAt));
    setSourceType(task.sourceType);
    setSourceUrl(task.sourceUrl ?? "");
    setScreenshots(task.screenshots);
    setAudioAttachments(task.audioAttachments);
    setLinks(task.links);
    setFileAttachments(task.fileAttachments);
    setEditVersion(0);
    latestEditVersion.current = 0;
    savedEditVersion.current = 0;
  }, [saveState, task.id, task.updatedAt]);

  useEffect(() => () => {
    const version = latestEditVersion.current;
    if (version <= savedEditVersion.current) return;
    void onUpdateRef.current(latestPatch.current).catch((reason) => onErrorRef.current(String(reason)));
  }, []);

  useEffect(() => {
    if (editVersion === 0) return;
    const version = editVersion;
    const taskId = task.id;
    const patch = latestPatch.current;
    setSaveState("saving");
    const timeout = window.setTimeout(() => {
      void onUpdateRef.current(patch).then(() => {
        savedEditVersion.current = Math.max(savedEditVersion.current, version);
        if (activeTaskId.current === taskId && version === latestEditVersion.current) setSaveState("saved");
      }).catch((reason) => {
        if (activeTaskId.current === taskId) setSaveState("error");
        onErrorRef.current(String(reason));
      });
    }, 500);
    return () => window.clearTimeout(timeout);
  }, [audioAttachments, dueAt, editVersion, fileAttachments, links, notes, priority, screenshots, sourceType, sourceUrl, task.id, title]);

  useEffect(() => {
    const handlePaste = (event: ClipboardEvent) => {
      if (document.querySelector(".composer")) return;
      const image = Array.from(event.clipboardData?.items ?? []).find((item) => item.type.startsWith("image/"));
      const file = image?.getAsFile() ?? Array.from(event.clipboardData?.files ?? []).find((item) => item.type.startsWith("image/"));
      if (!file) return;
      event.preventDefault();
      void fileToDataUrl(file).then((dataUrl) => {
        setScreenshots((current) => [...current, dataUrl]);
        markChanged();
      }).catch((reason) => onError(String(reason)));
    };
    window.addEventListener("paste", handlePaste, true);
    return () => window.removeEventListener("paste", handlePaste, true);
  }, [onError]);

  useEffect(() => {
    if (!moreOpen) return;
    const closeMenu = (event: MouseEvent) => {
      if (!moreActionsRef.current?.contains(event.target as Node)) setMoreOpen(false);
    };
    const closeMenuOnEscape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setMoreOpen(false);
    };
    document.addEventListener("mousedown", closeMenu);
    document.addEventListener("keydown", closeMenuOnEscape);
    return () => {
      document.removeEventListener("mousedown", closeMenu);
      document.removeEventListener("keydown", closeMenuOnEscape);
    };
  }, [moreOpen]);

  function markChanged() {
    setEditVersion((current) => {
      latestEditVersion.current = current + 1;
      return current + 1;
    });
  }

  function change<T>(setter: (value: T) => void, value: T) {
    setter(value);
    markChanged();
  }

  async function takeScreenshot() {
    const appWindow = getCurrentWindow();
    try {
      await appWindow.hide();
      const screenshot = await api.captureScreenshot();
      setScreenshots((current) => [...current, screenshot]);
      markChanged();
    } catch (reason) {
      if (!String(reason).toLocaleLowerCase().includes("cancel")) onError(String(reason));
    } finally {
      await appWindow.show();
      await appWindow.unminimize();
      await appWindow.setAlwaysOnTop(true);
      await appWindow.setFocus();
      window.setTimeout(() => void appWindow.setAlwaysOnTop(false), 1_200);
    }
  }

  async function toggleRecording() {
    if (recordingBusy) return;
    setRecordingBusy(true);
    if (!recording) {
      setRecording(true);
      try {
        recorder.current = new WavRecorder();
        await recorder.current.start(selectedAudioInput);
      } catch (reason) {
        setRecording(false);
        onError(String(reason));
      } finally {
        setRecordingBusy(false);
      }
      return;
    }
    let backgroundStarted = false;
    try {
      onTaskProcessing(task.id, true);
      backgroundStarted = true;
      const result = await recorder.current!.stop();
      const attachment = recordingAttachment(result);
      const nextAudio = [...audioAttachments, attachment];
      setAudioAttachments(nextAudio);
      await onUpdateRef.current({ audioAttachments: nextAudio });
      const recordedAt = new Date();
      onAudioTranscribing(attachment.id, true);
      void api.transcribeWav(result.base64).then(async (transcript) => {
        const text = transcript.trim();
        if (!text) return;
        const latestTask = (await api.listTasks()).find((item) => item.id === task.id);
        if (!latestTask) return;
        const nextNotes = appendTranscriptNotes(latestTask.notes, [timestampedTranscript(text, recordedAt)]);
        const nextTitle = isGeneratedTaskTitle(latestTask.title) ? transcriptTitle(text) : latestTask.title;
        await onUpdateRef.current({ title: nextTitle, notes: nextNotes });
        if (activeTaskId.current === task.id) {
          setTitle(nextTitle);
          setNotes(nextNotes);
        }
      }).catch((reason) => onErrorRef.current(String(reason))).finally(() => {
        onAudioTranscribing(attachment.id, false);
        onTaskProcessing(task.id, false);
      });
      backgroundStarted = false;
    } catch (reason) {
      onError(String(reason));
    } finally {
      if (backgroundStarted) onTaskProcessing(task.id, false);
      setRecording(false);
      setRecordingBusy(false);
    }
  }

  async function addFiles(files: FileList | null) {
    if (!files?.length) return;
    try {
      const added = await filesToAttachments(files);
      setFileAttachments((current) => [...current, ...added]);
      markChanged();
    } catch (reason) {
      onError(String(reason));
    } finally {
      if (fileInput.current) fileInput.current.value = "";
    }
  }

  function removeScreenshot(index: number) {
    setScreenshots((current) => current.filter((_, itemIndex) => itemIndex !== index));
    markChanged();
  }

  return <div className="details-content">
    <input className="details-title" value={title} onChange={(event) => change(setTitle, event.target.value)} />
    <div className="status-actions"><button className="icon-button close-details" onClick={onClose}><X /></button><div className="status-switcher priority-switcher">{(["high", "mid", "low"] as TaskPriority[]).map((value) => <button key={value} className={priority === value ? `active ${value}` : value} onClick={() => change(setPriority, value)}>{value}</button>)}</div><span className="task-created">Created {formatCreatedAt(task.createdAt)}</span><div className="toolbar-spacer" />{task.deletedAt ? <button className="restore-detail-button" title="Restore task" onClick={() => void onRestore()}><RotateCcw size={16} />Restore</button> : null}<button className="icon-button danger" title={task.deletedAt ? "Delete permanently" : "Delete task"} onClick={onDelete}><Trash2 size={18} /></button><div className="more-actions" ref={moreActionsRef}><button className="icon-button" title="More actions" onClick={() => setMoreOpen((current) => !current)}><MoreHorizontal size={19} /></button>{moreOpen && <div className="more-menu">{!task.deletedAt && <><button onClick={() => { void onUpdate({ status: "done" }); setMoreOpen(false); }}>Mark completed</button><button onClick={() => { const remindAt = new Date(Date.now() + 15 * 60_000).toISOString(); change(setDueAt, toDateTimeLocal(remindAt)); setMoreOpen(false); }}>Remind in 15 minutes</button><button disabled={!dueAt} onClick={() => { change(setDueAt, ""); setMoreOpen(false); }}>Clear due date</button></>}{sourceUrl && <button onClick={() => { void openUrl(sourceUrl); setMoreOpen(false); }}>Open source</button>}</div>}</div></div>
    <section className="detail-section"><label><span>Project</span><ProjectSelect value={task.projectId} workspace={workspace} onChange={(projectId) => void onUpdate({ projectId })} /></label><p className="field-help">A task without a project stays in the inbox. The project decides which material a model may read when it works on this task.</p></section>
    <section className="detail-section"><label><span>Due and remind</span><DateTimePicker value={dueAt} onChange={(value) => change(setDueAt, value)} clockFormat={clockFormat} /></label></section>
    <ReminderPresetButtons onChoose={(preset) => change(setDueAt, toLocalDateTime(reminderDate(preset, tomorrowReminderTime)))} />
    <section className="detail-section"><label><span>Notes</span><div className="notes-capture details-notes"><textarea rows={6} value={notes} onChange={(event) => change(setNotes, event.target.value)} placeholder="Add useful context" /></div></label></section>
    <section className="detail-section details-source-fields"><label><span>Source</span><select value={sourceType} onChange={(event) => change(setSourceType, event.target.value as SourceType)}><option value="manual">Manual</option><option value="slack">Slack</option><option value="gmail">Gmail</option><option value="whatsapp">WhatsApp</option><option value="web">Web</option></select></label></section>
    <LinkEditor links={[...new Set([...(sourceUrl ? [sourceUrl] : []), ...links])]} onAdd={(link) => { if (link !== sourceUrl && !links.includes(link)) change(setLinks, [...links, link]); }} onRemove={(link) => { if (link === sourceUrl) change(setSourceUrl, ""); else change(setLinks, links.filter((item) => item !== link)); }} onError={onError} />
    {(task.sourceLabel || task.sourceAuthor || task.sourceExcerpt) && <section className="source-card"><div className="source-card-header"><div><p className="eyebrow">Source message</p><h3>{task.sourceLabel ?? sourceType}</h3></div>{sourceUrl && <button className="secondary-button" onClick={() => openUrl(sourceUrl)}><ExternalLink size={15} />Open source</button>}</div>{task.sourceAuthor && <p className="source-author">{task.sourceAuthor}</p>}{task.sourceExcerpt && <blockquote>{task.sourceExcerpt}</blockquote>}</section>}
    <section className="detail-section"><div className="media-heading"><div className="media-label-row"><p className="field-label">Media</p><AudioInputSelect compact value={selectedAudioInput} disabled={recording || recordingBusy} onChange={setSelectedAudioInput} /></div><div className="media-actions"><button type="button" className="secondary-button" onClick={takeScreenshot} disabled={recording || recordingBusy}><Camera size={15} />Screenshot</button><button type="button" className="secondary-button" onClick={() => fileInput.current?.click()}><FilePlus2 size={15} />Add file</button><button type="button" className={`secondary-button media-record-button ${recording ? "recording" : ""}`} onClick={toggleRecording} disabled={recordingBusy}>{recording ? <MicOff size={15} /> : <Mic size={15} />}{recording ? "Stop recording" : "Add audio"}</button><input ref={fileInput} className="hidden-file-input" type="file" multiple onChange={(event) => void addFiles(event.target.files)} /></div></div>{transcribingAudioIds.length > 0 && <p className="field-help">Transcribing audio in the background...</p>}{screenshots.length > 0 && <div className="detail-screenshot-grid">{screenshots.map((screenshot, index) => <div className="detail-screenshot-item" key={`${screenshot.slice(-24)}-${index}`}><button type="button" className="screenshot-preview" onClick={() => void api.openMedia(screenshot).catch((reason) => onError(String(reason)))} title="Open in system application"><img className="detail-screenshot" src={mediaSource(screenshot)} alt={`Task context ${index + 1}`} /></button><button type="button" className="download-image detail-download-image" title="Download screenshot" onClick={() => void downloadDataUrl(`screenshot-${index + 1}.png`, screenshot, onError)}><Download size={14} /></button><button type="button" className="remove-image detail-remove-image" title="Remove screenshot" onClick={() => removeScreenshot(index)}><Trash2 size={15} /></button></div>)}</div>}{fileAttachments.length > 0 && <FileAttachmentList attachments={fileAttachments} onOpen={(attachment) => void api.openMedia(attachment.dataUrl).catch((reason) => onError(String(reason)))} onDownload={(attachment) => void downloadDataUrl(attachment.name, attachment.dataUrl, onError)} onRemove={(id) => { setFileAttachments((current) => current.filter((attachment) => attachment.id !== id)); markChanged(); }} />}{audioAttachments.length > 0 && <AttachmentAudioList attachments={audioAttachments} transcribingIds={transcribingAudioIds} onError={onError} onDownload={(attachment, index) => void downloadDataUrl(`recording-${index + 1}.wav`, attachment.dataUrl, onError)} onRemove={(id) => { setAudioAttachments((current) => current.filter((attachment) => attachment.id !== id)); markChanged(); }} />}</section>
  </div>;
}

function WelcomeDialog({ shortcut, settings, onSettings, onComplete, onError }: { shortcut: string; settings: AppSettings; onSettings: (settings: AppSettings) => void; onComplete: () => void; onError: (message: string) => void }) {
  const [step, setStep] = useState<"intro" | "providers">("intro");

  // The provider choice is offered once, here, because neither speech model size nor the language
  // model has a default that suits everyone. Skipping it is allowed: nothing breaks until analysis
  // is asked for.
  async function changeSettings(patch: Partial<AppSettings>) {
    try {
      onSettings(await api.updateSettings({ ...settings, ...patch }));
    } catch (reason) {
      onError(String(reason));
    }
  }

  if (step === "providers") {
    return <div className="modal-backdrop welcome-backdrop"><section className="welcome-dialog welcome-providers" role="dialog" aria-modal="true" aria-labelledby="welcome-providers-title">
      <div className="welcome-hero"><div className="welcome-mark"><SlidersHorizontal size={28} /></div><p className="eyebrow">One choice before you start</p><h2 id="welcome-providers-title">Who does the listening and the thinking</h2><p>Recognition always happens on this computer. Analysis is your decision, and nothing is sent anywhere until you make it.</p></div>
      <ProviderSetup settings={settings} onChange={(patch) => void changeSettings(patch)} onError={onError} />
      <div className="welcome-footer"><p><SlidersHorizontal size={15} /> All of this can be changed later in Settings.</p><div className="welcome-actions"><button className="secondary-button" onClick={() => setStep("intro")}>Back</button><button className="primary-button" onClick={onComplete}>Start using Threadbox</button></div></div>
    </section></div>;
  }

  return <div className="modal-backdrop welcome-backdrop"><section className="welcome-dialog" role="dialog" aria-modal="true" aria-labelledby="welcome-title">
    <div className="welcome-hero"><div className="welcome-mark"><Zap size={28} /></div><p className="eyebrow">Welcome to Threadbox</p><h2 id="welcome-title">Catch it now. Remember it later.</h2><p>Turn a passing message, thought, screenshot or voice note into a task before it disappears.</p></div>
    <div className="welcome-flow">
      <article><span><Zap size={19} /></span><div><strong>Capture in seconds</strong><p>Press <kbd>{shortcutLabel(shortcut)}</kbd> anywhere, then type, speak or attach context.</p></div></article>
      <article><span><Paperclip size={19} /></span><div><strong>Keep the useful context</strong><p>Links, screenshots, recordings and files stay beside the task.</p></div></article>
      <article><span><BellRing size={19} /></span><div><strong>Let Threadbox bring it back</strong><p>Set a reminder and get a visible nudge when it is time to act.</p></div></article>
    </div>
    <div className="welcome-footer"><p><SlidersHorizontal size={15} /> Shortcuts, audio, reminders and retention can all be adjusted in Settings.</p><button className="primary-button" onClick={() => setStep("providers")}>Next</button></div>
  </section></div>;
}

function SettingsDialog({ settings, onSettings, onClose, onError }: { settings: AppSettings; onSettings: (settings: AppSettings) => void; onClose: () => void; onError: (error: string) => void }) {
  async function backup() {
    try {
      const path = await save({ defaultPath: `threadbox-backup-${new Date().toISOString().slice(0, 10)}.zip`, filters: [{ name: "Threadbox backup", extensions: ["zip"] }] });
      if (path) await api.exportBackup(path);
    } catch (reason) { onError(String(reason)); }
  }

  async function changeSettings(patch: Partial<AppSettings>) {
    try {
      onSettings(await api.updateSettings({ ...settings, ...patch }));
    } catch (reason) {
      onError(String(reason));
    }
  }

  return <div className="modal-backdrop"><div className="settings-dialog"><div className="dialog-header"><div><p className="eyebrow">Threadbox</p><h2>Settings</h2></div><button className="icon-button" onClick={onClose}><X /></button></div>
    <section className="settings-section"><h3>Startup</h3><p>Keep global capture and reminders available after you log in.</p><label className="toggle-row"><span>Start Threadbox when I log in</span><input type="checkbox" checked={settings.startAtLogin} onChange={(event) => changeSettings({ startAtLogin: event.target.checked })} /></label><p className="setting-note">Automatic launches stay hidden in the system tray. Opening Threadbox yourself still shows the main window.</p></section>
    <section className="settings-section"><h3>Local speech recognition</h3><p>Audio never leaves this computer. Larger models are markedly more accurate on meetings and markedly slower, and the right trade depends on this machine.</p><SpeechProviderSettings settings={settings.speech} onChange={(patch) => void changeSettings({ speech: { ...settings.speech, ...patch } })} onError={onError} /><label className="setting-select"><span>Default audio input</span><select value={settings.audioInputMode} onChange={(event) => changeSettings({ audioInputMode: event.target.value as AudioInputMode })}><option value="microphone">Microphone</option><option value="system">System audio</option></select></label><p className="setting-note">System audio records the default PipeWire or PulseAudio monitor, including calls and other computer sounds.</p></section>
    <section className="settings-section"><h3>Language model for analysis</h3><p>Meeting notes, decisions and action items are written by a language model. Which one is your choice, and it can change at any time without losing anything already written.</p><LanguageModelProviderSettings settings={settings.languageModel} onChange={(patch) => void changeSettings({ languageModel: { ...settings.languageModel, ...patch } })} onError={onError} /></section>
    <section className="settings-section"><h3>Overdue reminders</h3><p>Repeat a clearly audible notification until the task is completed or reminders are disabled.</p><label className="toggle-row"><span>Repeat overdue reminders</span><input type="checkbox" checked={settings.overdueRemindersEnabled} onChange={(event) => changeSettings({ overdueRemindersEnabled: event.target.checked })} /></label><label className="toggle-row"><span>Show a sticky reminder above other apps</span><input type="checkbox" checked={settings.stickyRemindersEnabled} disabled={!settings.overdueRemindersEnabled} onChange={(event) => changeSettings({ stickyRemindersEnabled: event.target.checked })} /></label><label className="setting-select"><span>Repeat every</span><select value={settings.overdueIntervalMinutes} disabled={!settings.overdueRemindersEnabled} onChange={(event) => changeSettings({ overdueIntervalMinutes: Number(event.target.value) })}>{[5, 10, 15, 30, 60].map((minutes) => <option value={minutes} key={minutes}>{minutes} minutes</option>)}</select></label><button className="secondary-button test-reminder" onClick={() => void api.testReminderSound().catch((reason) => onError(String(reason)))}>Test reminder sound</button></section>
    <section className="settings-section"><h3>Reminder presets</h3><p>Tomorrow uses this local time when scheduling or snoozing a reminder.</p><label className="setting-select"><span>Tomorrow reminder time</span><input type="time" value={settings.tomorrowReminderTime} onChange={(event) => changeSettings({ tomorrowReminderTime: event.target.value })} /></label></section>
    <section className="settings-section"><h3>Task retention</h3><p>Completed and deleted tasks are permanently removed with all attached media after this period.</p><label className="setting-select"><span>Keep tasks for</span><select value={settings.taskRetentionDays} onChange={(event) => changeSettings({ taskRetentionDays: Number(event.target.value) })}>{[1, 3, 7, 14, 30, 60, 90, 180, 365].map((days) => <option value={days} key={days}>{days} {days === 1 ? "day" : "days"}</option>)}</select></label></section>
    <section className="settings-section"><h3>Quick capture</h3><p>The shortcut brings Threadbox above other applications and opens a new task.</p><label className="setting-select"><span>Keyboard shortcut</span><select value={settings.quickCaptureShortcut} onChange={(event) => changeSettings({ quickCaptureShortcut: event.target.value })}><option value="CommandOrControl+Shift+Space">Ctrl Shift Space</option><option value="CommandOrControl+Alt+Space">Ctrl Alt Space</option><option value="CommandOrControl+Shift+A">Ctrl Shift A</option><option value="CommandOrControl+Alt+T">Ctrl Alt T</option></select></label></section>
    <section className="settings-section"><h3>Date and time</h3><p>Choose how hours are displayed in task dates and the clock picker.</p><label className="setting-select"><span>Clock format</span><select value={settings.clockFormat} onChange={(event) => changeSettings({ clockFormat: event.target.value as AppSettings["clockFormat"] })}><option value="24h">24-hour clock</option><option value="12h">12-hour clock</option></select></label></section>
    <section className="settings-section"><h3>Welcome guide</h3><p>Show the short introduction again after this Settings window is closed.</p><button className="secondary-button" disabled={!settings.welcomeCompleted} onClick={() => void changeSettings({ welcomeCompleted: false })}>Show welcome guide again</button>{!settings.welcomeCompleted && <p className="setting-confirmation"><Check size={14} />Ready. Close Settings to open the welcome guide.</p>}</section>
    <section className="settings-section"><h3>Backup</h3><p>Export tasks, message references and attachments to a portable JSON file.</p><button className="secondary-button" onClick={backup}>Export backup</button></section>
  </div></div>;
}

function ReminderCenter({ tasks, tomorrowReminderTime, onClose, onOpen, onDone, onSnooze, onSnoozeAll }: { tasks: Task[]; tomorrowReminderTime: string; onClose: () => void; onOpen: (task: Task) => void; onDone: (task: Task) => Promise<void>; onSnooze: (task: Task, preset: ReminderPreset) => Promise<void>; onSnoozeAll: (tasks: Task[], preset: ReminderPreset) => Promise<void> }) {
  const [page, setPage] = useState(1);
  const pageCount = Math.max(1, Math.ceil(tasks.length / pageSize));
  const visibleTasks = tasks.slice((page - 1) * pageSize, page * pageSize);
  useEffect(() => setPage((current) => Math.min(current, pageCount)), [pageCount]);
  return <div className="modal-backdrop" onMouseDown={(event) => event.target === event.currentTarget && onClose()}><section className="reminder-center">
    <div className="dialog-header"><div><p className="eyebrow">Threadbox</p><h2>Reminder center</h2></div><button className="icon-button" onClick={onClose}><X /></button></div>
    {tasks.length === 0 ? <div className="reminder-empty"><Bell size={28} /><h3>No scheduled tasks</h3><p>Tasks with a due date will appear here.</p></div> : <><SnoozeAll tasks={tasks} tomorrowReminderTime={tomorrowReminderTime} onSnoozeAll={onSnoozeAll} /><div className="reminder-list">{visibleTasks.map((task) => {
      const overdue = Boolean(task.dueAt && new Date(task.dueAt) <= new Date());
      return <article className={`reminder-item ${overdue ? "overdue-item" : ""}`} key={task.id}><button className="reminder-title" onClick={() => onOpen(task)}><strong>{task.title}</strong><span>{formatDueDate(task.remindAt ?? task.dueAt)}</span></button><div className="reminder-actions"><ReminderPresetButtons compact tomorrowReminderTime={tomorrowReminderTime} onChoose={(preset) => void onSnooze(task, preset)} /><button className="done-reminder" onClick={() => void onDone(task)}>Done</button></div></article>;
    })}</div>{tasks.length > pageSize && <Pagination page={page} pageCount={pageCount} itemCount={tasks.length} onPage={setPage} />}</>}
  </section></div>;
}

function StickyReminder({ tasks, tomorrowReminderTime, onOpen, onDone, onSnooze, onSnoozeAll }: { tasks: Task[]; tomorrowReminderTime: string; onOpen: (task: Task) => void; onDone: (task: Task) => Promise<void>; onSnooze: (task: Task, preset: ReminderPreset) => Promise<void>; onSnoozeAll: (tasks: Task[], preset: ReminderPreset) => Promise<void> }) {
  return <div className="modal-backdrop sticky-reminder-backdrop"><section className="reminder-center sticky-reminder" role="alertdialog" aria-modal="true" aria-label="Overdue Threadbox reminders">
    <div className="sticky-reminder-header"><Bell size={19} /><strong>{tasks.length === 1 ? "Threadbox reminder" : `${tasks.length} Threadbox reminders`}</strong></div>
    <div className="reminder-list">{tasks.slice(0, 5).map((task) => <article className="reminder-item overdue-item" key={task.id}>
      <button className="reminder-title sticky-reminder-title" onClick={() => onOpen(task)}><strong>{task.title}</strong><span>{formatDueDate(task.remindAt ?? task.dueAt)}</span></button>
      <div className="sticky-task-actions"><button onClick={() => void onSnooze(task, "15m")}>Snooze 15 min</button><button onClick={() => onOpen(task)}>Open</button><button className="done-reminder" onClick={() => void onDone(task)}>Done</button></div>
    </article>)}{tasks.length > 5 && <p className="additional-reminders">{tasks.length - 5} more reminders will appear as you handle these.</p>}</div>
    <SnoozeAll tasks={tasks} tomorrowReminderTime={tomorrowReminderTime} onSnoozeAll={onSnoozeAll} />
  </section></div>;
}

function SnoozeAll({ tasks, tomorrowReminderTime, onSnoozeAll }: { tasks: Task[]; tomorrowReminderTime: string; onSnoozeAll: (tasks: Task[], preset: ReminderPreset) => Promise<void> }) {
  return <div className="snooze-all"><strong>Snooze all</strong><ReminderPresetButtons compact tomorrowReminderTime={tomorrowReminderTime} onChoose={(preset) => void onSnoozeAll(tasks, preset)} /></div>;
}

function ReminderPresetButtons({ compact = false, tomorrowReminderTime, onChoose }: { compact?: boolean; tomorrowReminderTime?: string; onChoose: (preset: ReminderPreset) => void }) {
  return <div className={`reminder-presets ${compact ? "compact" : ""}`} aria-label="Reminder presets">{reminderPresets.map((preset) => <button type="button" key={preset.value} onClick={() => onChoose(preset.value)}>{preset.value === "tomorrow" && tomorrowReminderTime ? `Tomorrow ${tomorrowReminderTime}` : preset.label}</button>)}</div>;
}

function reminderTasks(tasks: Task[]): Task[] {
  return tasks
    .filter((task) => !task.deletedAt && task.status !== "done" && Boolean(task.remindAt ?? task.dueAt))
    .sort((left, right) => new Date(left.remindAt ?? left.dueAt!).getTime() - new Date(right.remindAt ?? right.dueAt!).getTime());
}

function dueReminderTasks(tasks: Task[]): Task[] {
  const now = Date.now();
  return reminderTasks(tasks).filter((task) => new Date(task.remindAt ?? task.dueAt!).getTime() <= now);
}

/** The project a task is filed under, spelled out from the organisation down. */
function taskProjectLabel(task: Task, workspace: Workspace): string | null {
  if (!task.projectId) return null;
  const project = workspace.projects.find((item) => item.id === task.projectId);
  return project ? projectPath(project, workspace) : null;
}

export function normalizeLink(value: string): string {
  const input = value.trim();
  const candidate = /^[a-z][a-z0-9+.-]*:\/\//i.test(input) ? input : `https://${input}`;
  const url = new URL(candidate);
  if (url.protocol !== "http:" && url.protocol !== "https:") throw new Error("Only HTTP and HTTPS links are supported.");
  return url.toString();
}

function recordingAttachment(recording: RecorderResult): AudioAttachment {
  return {
    id: crypto.randomUUID(),
    dataUrl: `data:audio/wav;base64,${recording.base64}`,
    durationSeconds: recording.durationSeconds,
    createdAt: new Date().toISOString(),
  };
}

function AudioInputSelect({ value, disabled, compact = false, onChange }: { value: AudioInputMode; disabled: boolean; compact?: boolean; onChange: (mode: AudioInputMode) => void }) {
  return <label className={`audio-input-select ${compact ? "compact" : ""}`}><span>Audio input</span><select value={value} disabled={disabled} onChange={(event) => onChange(event.target.value as AudioInputMode)}><option value="microphone">Microphone</option><option value="system">System audio</option></select></label>;
}

async function downloadDataUrl(name: string, dataUrl: string, onError: (message: string) => void): Promise<void> {
  try {
    const path = await save({ defaultPath: name });
    if (path) await api.saveDataUrl(path, dataUrl);
  } catch (reason) {
    onError(String(reason));
  }
}

export function taskTitle(title: string, notes: string): string {
  const trimmed = title.trim();
  if (trimmed && trimmed.toLocaleLowerCase() !== "new task") return trimmed;
  return notes.trim() ? transcriptTitle(notes) : "New task";
}

export function sortTasksForView(tasks: Task[]): Task[] {
  const priorityOrder: Record<TaskPriority, number> = { high: 0, mid: 1, low: 2 };
  return [...tasks].sort((left, right) => {
    const priorityDifference = priorityOrder[left.priority] - priorityOrder[right.priority];
    if (priorityDifference !== 0) return priorityDifference;
    return new Date(right.createdAt).getTime() - new Date(left.createdAt).getTime();
  });
}

export function isGeneratedTaskTitle(title: string): boolean {
  const normalized = title.trim().toLocaleLowerCase();
  return !normalized || normalized === "new task";
}

export function appendTranscriptNotes(notes: string, transcriptNotes: string[]): string {
  return transcriptNotes.reduce((current, transcriptNote) => {
    if (!transcriptNote || current.includes(transcriptNote)) return current;
    return [current, transcriptNote].filter(Boolean).join("\n\n");
  }, notes);
}

export function formatCreatedAt(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "at an unknown time";
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(date);
}

function AttachmentAudioList({ attachments, transcribingIds = [], onDownload, onRemove, onError }: { attachments: AudioAttachment[]; transcribingIds?: string[]; onDownload: (attachment: AudioAttachment, index: number) => void; onRemove: (id: string) => void | Promise<void>; onError: (message: string) => void }) {
  const [playingId, setPlayingId] = useState<string | null>(null);

  async function play(attachment: AudioAttachment) {
    if (playingId === attachment.id) {
      await api.stopRecordingPlayback();
      setPlayingId(null);
      return;
    }
    try {
      setPlayingId(attachment.id);
      await api.playRecording(attachment.dataUrl);
    } catch (reason) {
      onError(String(reason));
    } finally {
      setPlayingId(null);
    }
  }

  return <div className="audio-attachments">{attachments.map((attachment, index) => <div className="audio-attachment" key={attachment.id}>
    <div className="audio-attachment-label"><span>Recording {index + 1} ({formatDuration(attachment.durationSeconds)})</span>{transcribingIds.includes(attachment.id) && <span className="audio-transcribing" role="status"><LoaderCircle size={13} />Transcribing...</span>}</div>
    <button type="button" className="play-attachment" disabled={playingId !== null && playingId !== attachment.id} onClick={() => void play(attachment)}>{playingId === attachment.id ? <Square size={12} fill="currentColor" /> : <Play size={14} />}{playingId === attachment.id ? "Stop" : "Play"}</button>
    <button type="button" className="remove-attachment download-attachment" title="Download recording" onClick={() => onDownload(attachment, index)}><Download size={14} /></button>
    <button type="button" className="remove-attachment" title="Remove recording" onClick={() => void onRemove(attachment.id)}><X size={15} /></button>
  </div>)}</div>;
}

function formatDuration(seconds: number): string {
  const rounded = Math.max(0, Math.round(seconds));
  return `${Math.floor(rounded / 60)}:${String(rounded % 60).padStart(2, "0")}`;
}

export function timestampedTranscript(transcript: string, recordedAt: Date): string {
  const part = (value: number) => String(value).padStart(2, "0");
  const timestamp = `${recordedAt.getFullYear()}-${part(recordedAt.getMonth() + 1)}-${part(recordedAt.getDate())} ${part(recordedAt.getHours())}:${part(recordedAt.getMinutes())}`;
  return `[Audio transcript ${timestamp}]\n${transcript.trim()}`;
}

export function transcriptTitle(transcript: string, maximumLength = 50): string {
  const normalized = transcript.replace(/\s+/g, " ").trim();
  if (normalized.length <= maximumLength) return normalized;
  const candidate = normalized.slice(0, maximumLength + 1);
  const wordBoundary = candidate.lastIndexOf(" ");
  const end = wordBoundary > 0 ? wordBoundary : maximumLength;
  return `${normalized.slice(0, end).trimEnd()}...`;
}

export function reminderDate(preset: ReminderPreset, tomorrowTime: string, now = new Date()): Date {
  if (preset === "tomorrow") {
    const [hour = 8, minute = 30] = tomorrowTime.split(":").map(Number);
    return new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1, hour, minute, 0, 0);
  }
  const minutes: Record<Exclude<ReminderPreset, "tomorrow">, number> = { "15m": 15, "1h": 60, "3h": 180, "6h": 360, "24h": 1_440 };
  return new Date(now.getTime() + minutes[preset] * 60_000);
}

function toLocalDateTime(date: Date): string {
  const part = (value: number) => String(value).padStart(2, "0");
  return `${date.getFullYear()}-${part(date.getMonth() + 1)}-${part(date.getDate())}T${part(date.getHours())}:${part(date.getMinutes())}`;
}

function shortcutLabel(shortcut: string): string {
  return shortcut.replace("CommandOrControl", "Ctrl").replaceAll("+", " ");
}
