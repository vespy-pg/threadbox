import { useCallback, useEffect, useMemo, useState } from "react";
import { ArrowRight, BriefcaseBusiness, CalendarDays, CheckCircle2, Clock3, FileText, FolderKanban, Inbox, LayoutDashboard, Link2, ListTodo, Mail, Mic, Play, Plus, Plug, Square, Settings2, Trash2, UserPlus, Users } from "lucide-react";

import { api } from "./api";
import { ProjectDetail, projectPath, type Workspace } from "./projects";
import type { LanguageModelStatus, Meeting, MeetingAnalysis, MeetingTranscript, Organization, OrganizationMember, Person, ProcessingJob, Project, Task } from "./types";

export type WorkbenchArea = "organization-overview" | "projects" | "people" | "integrations" | "project-overview" | "threads" | "meetings" | "documents";

export function WorkspaceNavigation({ workspace, organization, project, area, inboxCount, projectTaskCounts, onOrganization, onCreateOrganization, onProject, onArea }: { workspace: Workspace; organization: Organization | null; project: Project | null; area: WorkbenchArea; inboxCount: number; projectTaskCounts: Record<string, number>; onOrganization: (id: string | null) => void; onCreateOrganization: () => void; onProject: (id: string | null) => void; onArea: (area: WorkbenchArea) => void }) {
  const projects = organization ? workspace.projects.filter((item) => item.organizationId === organization.id) : [];
  return <>
    <div className="context-switcher">
      <span className="nav-section-label">Organisation</span>
      <div><select value={organization?.id ?? ""} onChange={(event) => onOrganization(event.target.value || null)} aria-label="Current organisation"><option value="">Choose an organisation</option>{workspace.organizations.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select><button className="context-add" title="Add organisation" aria-label="Add organisation" onClick={onCreateOrganization}><Plus size={15} /></button></div>
    </div>
    <nav className="workspace-navigation" aria-label="Workspace navigation">
      <button className={!project && area === "threads" ? "nav-item active" : "nav-item"} onClick={() => { onProject(null); onArea("threads"); }}><Inbox size={17} /><span>Inbox</span>{inboxCount > 0 && <span className="count">{inboxCount}</span>}</button>
      {organization && <>
        <span className="nav-section-label">{organization.name}</span>
        <button className={area === "organization-overview" ? "nav-item active" : "nav-item"} onClick={() => { onProject(null); onArea("organization-overview"); }}><LayoutDashboard size={17} /><span>Overview</span></button>
        <button className={area === "people" ? "nav-item active" : "nav-item"} onClick={() => { onProject(null); onArea("people"); }}><Users size={17} /><span>People</span></button>
        <button className={area === "projects" ? "nav-item active" : "nav-item"} onClick={() => { onProject(null); onArea("projects"); }}><FolderKanban size={17} /><span>Projects</span><span className="count">{projects.length}</span></button>
        <button className={area === "integrations" ? "nav-item active" : "nav-item"} onClick={() => { onProject(null); onArea("integrations"); }}><Plug size={17} /><span>Integrations</span></button>
        <span className="nav-section-label project-label">Projects</span>
        <div className="sidebar-projects">
          {projects.filter((item) => item.parentId === null).map((item) => <SidebarProject key={item.id} project={item} allProjects={projects} selectedId={project?.id ?? null} taskCounts={projectTaskCounts} onSelect={(selected) => { onProject(selected.id); onArea("project-overview"); }} />)}
          {projects.length === 0 && <button className="sidebar-empty-action" onClick={() => onArea("projects")}><Plus size={14} />Create the first project</button>}
        </div>
      </>}
    </nav>
    {project && <nav className="project-navigation" aria-label={`${project.name} navigation`}>
      <span className="nav-section-label">Current project</span>
      <strong className="current-project-name">{project.name}</strong>
      <button className={area === "project-overview" ? "nav-item active" : "nav-item"} onClick={() => onArea("project-overview")}><LayoutDashboard size={17} /><span>Overview</span></button>
      <button className={area === "threads" ? "nav-item active" : "nav-item"} onClick={() => onArea("threads")}><ListTodo size={17} /><span>Threads</span>{(projectTaskCounts[project.id] ?? 0) > 0 && <span className="count">{projectTaskCounts[project.id]}</span>}</button>
      <button className={area === "meetings" ? "nav-item active" : "nav-item"} onClick={() => onArea("meetings")}><CalendarDays size={17} /><span>Meetings</span></button>
      <button className={area === "documents" ? "nav-item active" : "nav-item"} onClick={() => onArea("documents")}><FileText size={17} /><span>Documents</span></button>
    </nav>}
  </>;
}

function SidebarProject({ project, allProjects, selectedId, taskCounts, onSelect, depth = 0 }: { project: Project; allProjects: Project[]; selectedId: string | null; taskCounts: Record<string, number>; onSelect: (project: Project) => void; depth?: number }) {
  const children = allProjects.filter((item) => item.parentId === project.id);
  return <div className="sidebar-project-branch">
    <button className={project.id === selectedId ? "sidebar-project active" : "sidebar-project"} style={{ paddingLeft: `${10 + depth * 13}px` }} onClick={() => onSelect(project)}><span className="project-dot" /><span>{project.name}</span>{(taskCounts[project.id] ?? 0) > 0 && <small>{taskCounts[project.id]}</small>}</button>
    {children.map((child) => <SidebarProject key={child.id} project={child} allProjects={allProjects} selectedId={selectedId} taskCounts={taskCounts} onSelect={onSelect} depth={depth + 1} />)}
  </div>;
}

export function OrganizationOverview({ organization, workspace, tasks, onProject, onPeople }: { organization: Organization; workspace: Workspace; tasks: Task[]; onProject: (project: Project) => void; onPeople: () => void }) {
  const projects = workspace.projects.filter((project) => project.organizationId === organization.id);
  const projectIds = new Set(projects.map((project) => project.id));
  const organizationTasks = tasks.filter((task) => task.projectId && projectIds.has(task.projectId) && !task.deletedAt);
  const activeTasks = organizationTasks.filter((task) => task.status !== "done");
  const overdue = activeTasks.filter((task) => task.dueAt && new Date(task.dueAt).getTime() < Date.now());
  const highPriority = activeTasks.filter((task) => task.priority === "high");
  return <WorkspacePage eyebrow="Organisation overview" title={organization.name} description="The important work across every project, without losing the organisation context.">
    <div className="overview-metrics">
      <Metric icon={FolderKanban} label="Active projects" value={projects.length} />
      <Metric icon={ListTodo} label="Open threads" value={activeTasks.length} />
      <Metric icon={Clock3} label="Overdue" value={overdue.length} tone={overdue.length ? "attention" : "normal"} />
      <Metric icon={Users} label="People" value="Manage" onClick={onPeople} />
    </div>
    <div className="overview-grid">
      <section className="surface-card"><div className="surface-card-heading"><div><p className="eyebrow">Projects</p><h2>Where work is moving</h2></div></div><div className="project-card-list">{projects.map((project) => { const open = activeTasks.filter((task) => task.projectId === project.id); return <button key={project.id} className="project-summary-row" onClick={() => onProject(project)}><span className="project-summary-icon"><BriefcaseBusiness size={17} /></span><span><strong>{project.name}</strong><small>{project.description || "No project description yet"}</small></span><span className="project-summary-count">{open.length} open</span><ArrowRight size={16} /></button>; })}{projects.length === 0 && <EmptyPanel title="No projects yet" text="Create a project to give threads, meetings and documents a clear home." />}</div></section>
      <section className="surface-card"><div className="surface-card-heading"><div><p className="eyebrow">Attention</p><h2>Needs a decision</h2></div></div><div className="attention-list">{overdue.slice(0, 4).map((task) => <div key={task.id} className="attention-row"><Clock3 size={16} /><span><strong>{task.title}</strong><small>Overdue in {projects.find((project) => project.id === task.projectId)?.name}</small></span></div>)}{overdue.length === 0 && highPriority.slice(0, 4).map((task) => <div key={task.id} className="attention-row"><ListTodo size={16} /><span><strong>{task.title}</strong><small>High priority in {projects.find((project) => project.id === task.projectId)?.name}</small></span></div>)}{overdue.length === 0 && highPriority.length === 0 && <EmptyPanel title="Nothing urgent" text="Overdue and high-priority work will surface here." />}</div></section>
    </div>
  </WorkspacePage>;
}

export function ProjectOverview({ project, workspace, tasks, onArea }: { project: Project; workspace: Workspace; tasks: Task[]; onArea: (area: WorkbenchArea) => void }) {
  const projectTasks = tasks.filter((task) => task.projectId === project.id && !task.deletedAt);
  const open = projectTasks.filter((task) => task.status !== "done");
  return <WorkspacePage eyebrow={projectPath(project, workspace)} title={project.name} description={project.description || "Add a short project description so people and the meeting assistant understand what belongs here."}>
    <div className="overview-metrics project-metrics"><Metric icon={ListTodo} label="Open threads" value={open.length} onClick={() => onArea("threads")} /><Metric icon={CheckCircle2} label="Completed" value={projectTasks.filter((task) => task.status === "done").length} /><Metric icon={CalendarDays} label="Meetings" value="Next" onClick={() => onArea("meetings")} /><Metric icon={FileText} label="Documents" value="Open" onClick={() => onArea("documents")} /></div>
    <div className="overview-grid"><section className="surface-card"><div className="surface-card-heading"><div><p className="eyebrow">Threads</p><h2>Current work</h2></div><button className="text-link" onClick={() => onArea("threads")}>View all <ArrowRight size={14} /></button></div>{open.slice(0, 6).map((task) => <div key={task.id} className="work-preview-row"><span className={`priority-dot ${task.priority}`} /><span><strong>{task.title}</strong><small>{task.notes || "No additional context"}</small></span></div>)}{open.length === 0 && <EmptyPanel title="No open threads" text="Capture work here or move an inbox item into this project." />}</section><section className="surface-card next-module-card"><div className="module-icon"><CalendarDays size={22} /></div><p className="eyebrow">Coming next</p><h2>Meetings become work</h2><p>Google Calendar events, recordings, transcripts, decisions and action items will live in this project instead of in a separate tool.</p><button className="secondary-button" onClick={() => onArea("meetings")}>Open meetings</button></section></div>
  </WorkspacePage>;
}

export function PeopleWorkspace({ organization, onError }: { organization: Organization; onError: (message: string) => void }) {
  const [members, setMembers] = useState<OrganizationMember[]>([]);
  const [people, setPeople] = useState<Person[]>([]);
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState("");
  const [email, setEmail] = useState("");
  const [role, setRole] = useState("");
  const load = useCallback(async () => { try { const [nextMembers, nextPeople] = await Promise.all([api.listOrganizationPeople(organization.id), api.listPeople()]); setMembers(nextMembers); setPeople(nextPeople); } catch (reason) { onError(String(reason)); } }, [onError, organization.id]);
  useEffect(() => { void load(); }, [load]);
  async function create() { if (!name.trim()) return; try { const person = await api.createPerson({ displayName: name.trim(), email: email.trim() || null }); await api.addOrganizationPerson(organization.id, person.id, role.trim() || null); setName(""); setEmail(""); setRole(""); setAdding(false); await load(); } catch (reason) { onError(String(reason)); } }
  async function addExisting(personId: string) { if (!personId) return; try { await api.addOrganizationPerson(organization.id, personId); await load(); } catch (reason) { onError(String(reason)); } }
  const available = people.filter((person) => !members.some((member) => member.person.id === person.id));
  return <WorkspacePage eyebrow="Organisation" title="People" description={`Manage the people who work in ${organization.name}. Project assignments can be added independently later.`} action={<button className="primary-button" onClick={() => setAdding(true)}><UserPlus size={16} />Add person</button>}>
    {adding && <section className="inline-create-card"><div><label><span>Name</span><input autoFocus value={name} onChange={(event) => setName(event.target.value)} placeholder="Full name" /></label><label><span>Email</span><input value={email} onChange={(event) => setEmail(event.target.value)} placeholder="name@example.com" /></label><label><span>Role</span><input value={role} onChange={(event) => setRole(event.target.value)} placeholder="Role in this organisation" /></label></div><div className="inline-create-actions"><button className="text-button" onClick={() => setAdding(false)}>Cancel</button><button className="primary-button" disabled={!name.trim()} onClick={() => void create()}>Add to organisation</button></div></section>}
    {available.length > 0 && <label className="existing-person-picker"><span>Add an existing contact</span><select value="" onChange={(event) => void addExisting(event.target.value)}><option value="">Choose a person</option>{available.map((person) => <option key={person.id} value={person.id}>{person.displayName}</option>)}</select></label>}
    <section className="people-grid">{members.map((member) => <PersonCard key={member.person.id} organizationId={organization.id} member={member} onReload={load} onError={onError} />)}{members.length === 0 && !adding && <EmptyPanel title="No people in this organisation" text="Add employees, collaborators and clients here. They can later be assigned to projects, meetings and work." />}</section>
  </WorkspacePage>;
}

function PersonCard({ organizationId, member, onReload, onError }: { organizationId: string; member: OrganizationMember; onReload: () => Promise<void>; onError: (message: string) => void }) {
  const [role, setRole] = useState(member.role ?? "");
  async function saveRole() { if (role === (member.role ?? "")) return; try { await api.addOrganizationPerson(organizationId, member.person.id, role.trim() || null); await onReload(); } catch (reason) { onError(String(reason)); } }
  async function remove() { try { await api.removeOrganizationPerson(organizationId, member.person.id); await onReload(); } catch (reason) { onError(String(reason)); } }
  const initials = member.person.displayName.split(/\s+/).slice(0, 2).map((part) => part[0]).join("").toUpperCase();
  return <article className="person-card"><div className="person-avatar">{initials}</div><div className="person-card-main"><div className="person-card-title"><strong>{member.person.displayName}</strong>{member.person.isSelf && <span>You</span>}</div>{member.person.email && <a href={`mailto:${member.person.email}`}><Mail size={13} />{member.person.email}</a>}<label><span>Organisation role</span><input value={role} onChange={(event) => setRole(event.target.value)} onBlur={() => void saveRole()} placeholder="Add a role" /></label></div><button className="icon-button danger" title="Remove from this organisation" onClick={() => void remove()}><Trash2 size={16} /></button></article>;
}

export function ProjectsWorkspace({ organization, workspace, activeProject, onReload, onProject, onError }: { organization: Organization; workspace: Workspace; activeProject: Project | null; onReload: () => Promise<void>; onProject: (project: Project) => void; onError: (message: string) => void }) {
  const projects = workspace.projects.filter((project) => project.organizationId === organization.id);
  const [selectedId, setSelectedId] = useState<string | null>(activeProject?.id ?? projects[0]?.id ?? null);
  const selected = projects.find((project) => project.id === selectedId) ?? null;
  async function addProject() { const name = window.prompt("Project name"); if (!name?.trim()) return; try { const project = await api.createProject({ organizationId: organization.id, name: name.trim() }); await onReload(); setSelectedId(project.id); onProject(project); } catch (reason) { onError(String(reason)); } }
  return <WorkspacePage eyebrow="Organisation" title="Projects" description={`Projects keep the work, meetings and documents of ${organization.name} in a clear context.`} action={<button className="primary-button" onClick={addProject}><Plus size={16} />New project</button>}><div className="projects-workspace"><aside className="project-directory">{projects.map((project) => <button key={project.id} className={project.id === selectedId ? "active" : ""} onClick={() => { setSelectedId(project.id); onProject(project); }}><FolderKanban size={16} /><span><strong>{project.name}</strong><small>{project.description || "No description"}</small></span></button>)}{projects.length === 0 && <EmptyPanel title="No projects yet" text="Create the first project for this organisation." />}</aside><section className="project-editor">{selected ? <ProjectDetail key={selected.id} project={selected} workspace={workspace} onReload={onReload} onError={onError} /> : <div className="project-editor-empty"><Settings2 size={24} /><h2>Select a project</h2><p>Its context, language and documents will appear here.</p></div>}</section></div></WorkspacePage>;
}

export function MeetingsWorkspace({ project, organization, workspace, activeRecording, revision, onStart, onStop, onActiveMeetingUpdate, onTasksChanged, onIntegrations, onError }: { project: Project; organization: Organization; workspace: Workspace; activeRecording: Meeting | null; revision: number; onStart: (meeting: Meeting) => Promise<void>; onStop: () => Promise<void>; onActiveMeetingUpdate: (meeting: Meeting) => void; onTasksChanged: () => Promise<void>; onIntegrations: () => void; onError: (message: string) => void }) {
  const [meetings, setMeetings] = useState<Meeting[]>([]);
  const [creating, setCreating] = useState(false);
  const [title, setTitle] = useState("");
  const [scheduledStart, setScheduledStart] = useState("");
  const [busyId, setBusyId] = useState<string | null>(null);
  const [expandedTranscriptId, setExpandedTranscriptId] = useState<string | null>(null);
  const projects = workspace.projects.filter((item) => item.organizationId === organization.id);
  const load = useCallback(async () => {
    try { setMeetings(await api.listMeetings(project.id)); }
    catch (reason) { onError(String(reason)); }
  }, [onError, project.id]);
  useEffect(() => { void load(); }, [load, revision]);

  async function create(startNow: boolean) {
    if (!title.trim()) return;
    setBusyId("new");
    try {
      const meeting = await api.createMeeting({ projectId: project.id, title: title.trim(), scheduledStart: scheduledStart ? new Date(scheduledStart).toISOString() : null });
      setTitle(""); setScheduledStart(""); setCreating(false);
      if (startNow) await onStart(meeting);
      await load();
    } catch (reason) { onError(String(reason)); }
    finally { setBusyId(null); }
  }

  async function update(meeting: Meeting, patch: { title?: string; projectId?: string | null; scheduledStart?: string | null }) {
    setBusyId(meeting.id);
    try {
      const updated = await api.updateMeeting({ id: meeting.id, ...patch });
      if (activeRecording?.id === updated.id) onActiveMeetingUpdate(updated);
      await load();
    } catch (reason) { onError(String(reason)); }
    finally { setBusyId(null); }
  }

  async function remove(meeting: Meeting) {
    if (!window.confirm(`Delete ${meeting.title}?`)) return;
    setBusyId(meeting.id);
    try { await api.deleteMeeting(meeting.id); await load(); }
    catch (reason) { onError(String(reason)); }
    finally { setBusyId(null); }
  }

  async function start(meeting: Meeting) {
    setBusyId(meeting.id);
    try { await onStart(meeting); await load(); }
    catch (reason) { onError(String(reason)); }
    finally { setBusyId(null); }
  }

  return <WorkspacePage eyebrow={`${organization.name} / ${project.name}`} title="Meetings" description="Plan, capture and review the conversations that create work." action={<button className="primary-button" onClick={() => setCreating(true)}><Plus size={16} />New meeting</button>}>
    {creating && <section className="meeting-create-card"><div className="meeting-create-fields"><label><span>Meeting title</span><input autoFocus value={title} onChange={(event) => setTitle(event.target.value)} placeholder="Weekly project sync" /></label><label><span>Scheduled start</span><input type="datetime-local" value={scheduledStart} onChange={(event) => setScheduledStart(event.target.value)} /></label></div><p><Mic size={14} />Recording captures your microphone on the left channel and the system audio on the right. Ctrl/Cmd+Shift+M starts immediately in the current project.</p><div className="inline-create-actions"><button className="text-button" onClick={() => setCreating(false)}>Cancel</button><button className="secondary-button" disabled={!title.trim() || busyId === "new"} onClick={() => void create(false)}>Save for later</button><button className="primary-button recording-action" disabled={!title.trim() || busyId === "new" || Boolean(activeRecording)} onClick={() => void create(true)}><Mic size={15} />Save and record</button></div></section>}
    <div className="meeting-list">{meetings.map((meeting) => <article key={meeting.id} className={`meeting-card ${meeting.status === "recording" ? "recording" : ""}`}><div className={`meeting-status-icon ${meeting.status}`} >{meeting.status === "recording" ? <Mic size={18} /> : meeting.status === "recorded" ? <CheckCircle2 size={18} /> : <CalendarDays size={18} />}</div><div className="meeting-card-main"><div className="meeting-card-heading"><input aria-label="Meeting title" defaultValue={meeting.title} onBlur={(event) => { const next = event.target.value.trim(); if (next && next !== meeting.title) void update(meeting, { title: next }); }} /><span className={`status-pill ${meeting.status}`}>{meeting.status}</span></div><div className="meeting-meta"><span><Clock3 size={13} />{formatMeetingDate(meeting.scheduledStart ?? meeting.startedAt ?? meeting.createdAt)}</span>{meeting.durationSeconds !== null && <span>{formatMeetingDuration(meeting.durationSeconds)}</span>}<label><span>Project</span><select value={meeting.projectId ?? ""} disabled={busyId === meeting.id} onChange={(event) => void update(meeting, { projectId: event.target.value || null })}><option value="">Unassigned</option>{projects.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select></label></div><p className="meeting-track-note">{meeting.recordingPath ? "Stereo source saved: microphone left, system audio right." : meeting.status === "recording" ? "Both audio tracks are being captured now." : "Ready for two-track recording."}</p>{expandedTranscriptId === meeting.id && <MeetingTranscriptPanel meeting={meeting} onTasksChanged={onTasksChanged} onError={onError} />}</div><div className="meeting-actions">{meeting.status === "planned" && <button className="primary-button recording-action" disabled={Boolean(activeRecording) || busyId === meeting.id} onClick={() => void start(meeting)}><Mic size={15} />Record</button>}{meeting.status === "recording" && activeRecording?.id === meeting.id && <button className="stop-meeting-button" onClick={() => void onStop()}><Square size={14} />Stop</button>}{meeting.recordingPath && <button className="secondary-button" onClick={() => void api.playRecording(meeting.recordingPath!).catch((reason) => onError(String(reason)))}><Play size={14} />Play</button>}{meeting.recordingPath && <button className="secondary-button" onClick={() => setExpandedTranscriptId((current) => current === meeting.id ? null : meeting.id)}><FileText size={14} />Transcript</button>}<button className="icon-button danger" aria-label="Delete meeting" disabled={meeting.status === "recording" || busyId === meeting.id} onClick={() => void remove(meeting)}><Trash2 size={16} /></button></div></article>)}{meetings.length === 0 && !creating && <section className="empty-module compact"><div className="module-icon"><CalendarDays size={25} /></div><h2>No meetings in this project</h2><p>Create one now or connect Google Calendar when the integration becomes available.</p><button className="secondary-button" onClick={onIntegrations}><Plug size={15} />Open integrations</button></section>}</div>
  </WorkspacePage>;
}

export function MeetingRecordingBanner({ meeting, project, onStop }: { meeting: Meeting; project: Project | null; onStop: () => Promise<void> }) {
  const [seconds, setSeconds] = useState(() => elapsedRecordingSeconds(meeting.startedAt));
  const [stopping, setStopping] = useState(false);
  useEffect(() => { const timer = window.setInterval(() => setSeconds(elapsedRecordingSeconds(meeting.startedAt)), 1_000); return () => window.clearInterval(timer); }, [meeting.startedAt]);
  return <aside className="meeting-recording-banner" aria-live="polite"><span className="recording-pulse" /><div><strong>Recording {meeting.title}</strong><small>{project?.name ?? "Unassigned"} - microphone + system audio - {formatMeetingDuration(seconds)}</small></div><button disabled={stopping} onClick={() => { setStopping(true); void onStop().finally(() => setStopping(false)); }}><Square size={14} />{stopping ? "Saving..." : "Stop and save"}</button></aside>;
}

function MeetingTranscriptPanel({ meeting, onTasksChanged, onError }: { meeting: Meeting; onTasksChanged: () => Promise<void>; onError: (message: string) => void }) {
  const [transcript, setTranscript] = useState<MeetingTranscript | null>(null);
  const [jobs, setJobs] = useState<ProcessingJob[]>([]);
  const [analysis, setAnalysis] = useState<MeetingAnalysis | null>(null);
  const [analysisJobs, setAnalysisJobs] = useState<ProcessingJob[]>([]);
  const [modelStatus, setModelStatus] = useState<LanguageModelStatus | null>(null);
  const [processing, setProcessing] = useState(false);
  const [analysing, setAnalysing] = useState(false);

  const load = useCallback(async () => {
    try {
      const [nextTranscript, nextJobs, nextAnalysis, nextAnalysisJobs, nextModelStatus] = await Promise.all([
        api.meetingTranscript(meeting.id),
        api.meetingTranscriptionJobs(meeting.id),
        api.meetingAnalysis(meeting.id),
        api.meetingAnalysisJobs(meeting.id),
        api.languageModelStatus(),
      ]);
      setTranscript(nextTranscript);
      setJobs(nextJobs);
      setAnalysis(nextAnalysis);
      setAnalysisJobs(nextAnalysisJobs);
      setModelStatus(nextModelStatus);
    } catch (reason) {
      onError(String(reason));
    }
  }, [meeting.id, onError]);

  useEffect(() => { void load(); }, [load]);
  useEffect(() => {
    if (![...jobs, ...analysisJobs].some((job) => job.status === "queued" || job.status === "running")) return;
    const timer = window.setInterval(() => void load(), 2_000);
    return () => window.clearInterval(timer);
  }, [analysisJobs, jobs, load]);

  async function transcribe() {
    setProcessing(true);
    try {
      setTranscript(await api.transcribeMeeting(meeting.id));
      await load();
    } catch (reason) {
      onError(String(reason));
      await load();
    } finally {
      setProcessing(false);
    }
  }

  async function analyse() {
    setAnalysing(true);
    try {
      setAnalysis(await api.analyseMeeting(meeting.id));
      await onTasksChanged();
      await load();
    } catch (reason) {
      onError(String(reason));
      await load();
    } finally {
      setAnalysing(false);
    }
  }

  const latestJob = jobs[0] ?? null;
  const latestAnalysisJob = analysisJobs[0] ?? null;
  return <section className="meeting-transcript">
    <div className="meeting-transcript-heading"><div><strong>Source transcript</strong><small>{transcript ? `Microphone: ${transcript.microphoneLanguage} - system: ${transcript.systemLanguage} - ${transcript.modelId}` : "Timestamped microphone and system channels are stored separately."}</small></div>{!processing && latestJob?.status !== "running" && <button className="secondary-button" onClick={() => void transcribe()}>{transcript ? "Transcribe again" : latestJob?.status === "failed" ? "Retry transcription" : "Transcribe recording"}</button>}{(processing || latestJob?.status === "running" || latestJob?.status === "queued") && <span className="transcription-state">Processing...</span>}</div>
    {latestJob?.status === "failed" && <p className="transcription-error">{latestJob.error}</p>}
    {transcript && <div className="transcript-segments">{transcript.segments.map((segment) => <div className={`transcript-segment ${segment.channel}`} key={segment.id}><button title="Play from this timestamp" onClick={() => void api.playRecording(meeting.recordingPath!, segment.startMs / 1_000).catch((reason) => onError(String(reason)))}>{formatTranscriptTime(segment.startMs)}</button><span>{segment.channel === "microphone" ? "You" : "Others"}</span><p>{segment.text}</p></div>)}</div>}
    {!transcript && latestJob?.status !== "failed" && !processing && <p className="transcript-empty">No transcript yet. Threadbox will use the configured meeting model and project language.</p>}
    {transcript && <section className="meeting-analysis"><div className="meeting-analysis-heading"><div><strong>Meeting assistant</strong><small>{analysis ? `${analysis.provider} - ${analysis.model} - ${analysis.promptVersion}` : "Notes, decisions, your action items and moments addressed to you."}</small><small className="analysis-provider-summary">{modelStatus?.summary}</small></div>{!analysing && latestAnalysisJob?.status !== "running" && <button className="primary-button" disabled={!modelStatus?.configured} onClick={() => void analyse()}>{analysis ? "Analyse again" : latestAnalysisJob?.status === "failed" ? "Retry analysis" : "Analyse meeting"}</button>}{(analysing || latestAnalysisJob?.status === "running" || latestAnalysisJob?.status === "queued") && <span className="transcription-state">Analysing...</span>}</div>{latestAnalysisJob?.status === "failed" && <p className="transcription-error">{latestAnalysisJob.error}</p>}{analysis && <><div className="analysis-notes">{analysis.notes}</div><div className="analysis-items">{analysis.items.map((item) => <article className={`analysis-item ${item.kind}`} key={item.id}><span>{analysisItemLabel(item.kind)}</span><div><strong>{item.title}</strong><p>{item.text}</p><button className="analysis-timestamp" onClick={() => void api.playRecording(meeting.recordingPath!, item.startMs / 1_000).catch((reason) => onError(String(reason)))}>{formatTranscriptTime(item.startMs)}</button>{item.taskId && <small>Thread created</small>}</div></article>)}</div></>}</section>}
  </section>;
}

function analysisItemLabel(kind: MeetingAnalysis["items"][number]["kind"]): string {
  return { decision: "Decision", action_item: "Your action", addressed: "Addressed", term_explanation: "Explained" }[kind];
}

function elapsedRecordingSeconds(startedAt: string | null): number {
  if (!startedAt) return 0;
  return Math.max(0, Math.floor((Date.now() - new Date(startedAt).getTime()) / 1_000));
}

function formatMeetingDuration(seconds: number): string {
  const rounded = Math.max(0, Math.floor(seconds));
  return `${Math.floor(rounded / 60)}:${String(rounded % 60).padStart(2, "0")}`;
}

function formatTranscriptTime(milliseconds: number): string {
  const totalSeconds = Math.floor(milliseconds / 1_000);
  const minutes = Math.floor(totalSeconds / 60);
  return `${minutes}:${String(totalSeconds % 60).padStart(2, "0")}`;
}

function formatMeetingDate(value: string): string {
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(new Date(value));
}

export function IntegrationsWorkspace({ organization }: { organization: Organization }) {
  return <WorkspacePage eyebrow="Organisation" title="Integrations" description={`Connections available to ${organization.name}. Each integration must clearly state what it reads and changes.`}><section className="integration-card"><div className="integration-logo"><CalendarDays size={23} /></div><div><p className="eyebrow">Calendar</p><h2>Google Calendar</h2><p>Use existing events as the meeting schedule. Threadbox will not create a second calendar.</p></div><span className="status-pill planned">Planned next</span></section><section className="integration-card muted"><div className="integration-logo"><Link2 size={23} /></div><div><p className="eyebrow">Extensible workspace</p><h2>More connections can follow</h2><p>Calls, work distribution and future services will use the same clear integration surface.</p></div><span className="status-pill">Later</span></section></WorkspacePage>;
}

export function DocumentsWorkspace({ project, workspace, onReload, onError }: { project: Project; workspace: Workspace; onReload: () => Promise<void>; onError: (message: string) => void }) {
  return <WorkspacePage eyebrow={projectPath(project, workspace)} title="Documents" description="Reference material for people and for the assistant working in this project."><section className="surface-card document-workspace-card"><ProjectDetail project={project} workspace={workspace} onReload={onReload} onError={onError} /></section></WorkspacePage>;
}

function WorkspacePage({ eyebrow, title, description, action, children }: { eyebrow: string; title: string; description: string; action?: React.ReactNode; children: React.ReactNode }) {
  return <section className="workspace-page"><header className="workspace-page-header"><div><p className="eyebrow">{eyebrow}</p><h1>{title}</h1><p>{description}</p></div>{action}</header><div className="workspace-page-content">{children}</div></section>;
}

function Metric({ icon: Icon, label, value, tone = "normal", onClick }: { icon: typeof LayoutDashboard; label: string; value: number | string; tone?: "normal" | "attention"; onClick?: () => void }) {
  const content = <><span className={`metric-icon ${tone}`}><Icon size={18} /></span><span><small>{label}</small><strong>{value}</strong></span>{onClick && <ArrowRight size={15} />}</>;
  return onClick ? <button className="metric-card" onClick={onClick}>{content}</button> : <div className="metric-card">{content}</div>;
}

function EmptyPanel({ title, text }: { title: string; text: string }) {
  return <div className="empty-panel"><strong>{title}</strong><p>{text}</p></div>;
}
