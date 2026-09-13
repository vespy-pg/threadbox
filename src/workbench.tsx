import { useCallback, useEffect, useMemo, useState } from "react";
import { ArrowLeft, ArrowRight, BookOpen, BriefcaseBusiness, CalendarDays, CheckCircle2, ChevronDown, ChevronRight, Clock3, FileText, FolderKanban, Inbox, LayoutDashboard, ListTodo, Mail, MessageSquareText, Mic, Paperclip, Phone, Play, Plus, Plug, Send, Settings2, Square, StickyNote, Trash2, UserPlus, UserRoundCheck, Users, X } from "lucide-react";

import { api } from "./api";
import { ProjectDetail, projectPath, type Workspace } from "./projects";
import type { AppSettings, AvailableSlot, CalendarEventDraft, ExternalCalendar, ExternalCalendarEvent, FindTimeInput, IntegrationCapabilityId, IntegrationSnapshot, LanguageModelStatus, Meeting, MeetingAnalysis, MeetingTranscript, Organization, OrganizationMember, Person, ProcessingJob, Project, SpeechCloudStatus, SpeechProvider, Task, TranscriptSegment, VocabularyCandidate, VocabularySet, VocabularyTerm } from "./types";

export type WorkbenchArea = "organization-overview" | "projects" | "people" | "integrations" | "project-overview" | "threads" | "meetings" | "documents" | "communication" | "vocabulary";

const projectSections: Array<{ area: WorkbenchArea; label: string; icon: typeof LayoutDashboard }> = [
  { area: "project-overview", label: "Overview", icon: LayoutDashboard },
  { area: "threads", label: "Threads", icon: ListTodo },
  { area: "meetings", label: "Meetings", icon: CalendarDays },
  { area: "documents", label: "Documents", icon: FileText },
  { area: "communication", label: "Communication", icon: MessageSquareText },
  { area: "vocabulary", label: "Vocabulary", icon: BookOpen },
];

const areaLabels: Record<WorkbenchArea, string> = {
  "organization-overview": "Overview", projects: "Projects", people: "People", integrations: "Integrations",
  "project-overview": "Overview", threads: "Threads", meetings: "Meetings", documents: "Documents",
  communication: "Communication", vocabulary: "Vocabulary",
};

export function WorkspaceNavigation({ workspace, organization, project, area, inboxCount, projectTaskCounts, onOrganization, onCreateOrganization, onProject, onArea }: { workspace: Workspace; organization: Organization | null; project: Project | null; area: WorkbenchArea; inboxCount: number; projectTaskCounts: Record<string, number>; onOrganization: (id: string | null) => void; onCreateOrganization: () => void; onProject: (id: string | null) => void; onArea: (area: WorkbenchArea) => void }) {
  const projects = organization ? workspace.projects.filter((item) => item.organizationId === organization.id) : [];
  return <>
    <div className="context-switcher">
      <span className="nav-section-label">Organisation</span>
      <div><label className="organization-select"><select value={organization?.id ?? ""} onChange={(event) => onOrganization(event.target.value || null)} aria-label="Current organisation"><option value="">Choose an organisation</option>{workspace.organizations.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select><ChevronDown size={14} /></label><button className="context-add" title="Add organisation" aria-label="Add organisation" onClick={onCreateOrganization}><Plus size={15} /></button></div>
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
          {projects.filter((item) => item.parentId === null).map((item) => <SidebarProject key={item.id} project={item} allProjects={projects} selectedId={project?.id ?? null} area={area} taskCounts={projectTaskCounts} onSelect={(selected) => { onProject(selected.id); onArea("project-overview"); }} onArea={onArea} />)}
          {projects.length === 0 && <button className="sidebar-empty-action" onClick={() => onArea("projects")}><Plus size={14} />Create the first project</button>}
        </div>
      </>}
    </nav>
  </>;
}

function SidebarProject({ project, allProjects, selectedId, area, taskCounts, onSelect, onArea, depth = 0 }: { project: Project; allProjects: Project[]; selectedId: string | null; area: WorkbenchArea; taskCounts: Record<string, number>; onSelect: (project: Project) => void; onArea: (area: WorkbenchArea) => void; depth?: number }) {
  const children = allProjects.filter((item) => item.parentId === project.id);
  const selected = project.id === selectedId;
  return <div className="sidebar-project-branch">
    <button className={selected ? "sidebar-project active" : "sidebar-project"} style={{ paddingLeft: `${10 + depth * 13}px` }} onClick={() => onSelect(project)}>{children.length > 0 ? <ChevronDown className="project-chevron" size={11} /> : <span className="project-dot" />}<span>{project.name}</span>{(taskCounts[project.id] ?? 0) > 0 && <small>{taskCounts[project.id]}</small>}</button>
    {selected && <div className="sidebar-project-modules" style={{ marginLeft: `${19 + depth * 13}px` }}>{projectSections.map(({ area: section, label, icon: Icon }) => <button key={section} className={area === section ? "active" : ""} onClick={() => onArea(section)}><Icon size={14} /><span>{label}</span>{section === "threads" && (taskCounts[project.id] ?? 0) > 0 && <small>{taskCounts[project.id]}</small>}</button>)}</div>}
    {children.map((child) => <SidebarProject key={child.id} project={child} allProjects={allProjects} selectedId={selectedId} area={area} taskCounts={taskCounts} onSelect={onSelect} onArea={onArea} depth={depth + 1} />)}
  </div>;
}

export function WorkspaceContextBar({ workspace, organization, project, area, onInbox, onOrganization, onProjects, onProject, onArea }: { workspace: Workspace; organization: Organization | null; project: Project | null; area: WorkbenchArea; onInbox: () => void; onOrganization: () => void; onProjects: () => void; onProject: (project: Project) => void; onArea: (area: WorkbenchArea) => void }) {
  const projectChain: Project[] = [];
  let cursor = project;
  while (cursor) {
    projectChain.unshift(cursor);
    const parentId = cursor.parentId;
    cursor = parentId ? workspace.projects.find((item) => item.id === parentId) ?? null : null;
  }
  const inbox = !project && area === "threads";
  const parentProject = project?.parentId ? workspace.projects.find((item) => item.id === project.parentId) ?? null : null;
  const back = inbox ? null : project
    ? area === "project-overview" ? parentProject ? () => onProject(parentProject) : onProjects : () => onArea("project-overview")
    : organization && area !== "organization-overview" ? onOrganization : organization ? onInbox : null;
  const backLabel = project ? area === "project-overview" ? parentProject ? `Back to ${parentProject.name}` : "Back to projects" : `Back to ${project.name}` : organization && area !== "organization-overview" ? `Back to ${organization.name}` : "Back to inbox";
  return <header className="workspace-context-bar">
    {back && <button className="context-back" onClick={back} aria-label={backLabel} title={backLabel}><ArrowLeft size={15} /><span>{backLabel}</span></button>}
    <nav className="breadcrumbs" aria-label="Breadcrumb">
      {inbox && <span aria-current="page">Inbox</span>}
      {!inbox && !organization && <span aria-current="page">Workspace</span>}
      {!inbox && organization && <><button onClick={onOrganization}>{organization.name}</button>{project && <><ChevronRight size={12} /><button onClick={onProjects}>Projects</button>{projectChain.map((item) => <span className="breadcrumb-part" key={item.id}><ChevronRight size={12} /><button onClick={() => onProject(item)}>{item.name}</button></span>)}</>}<ChevronRight size={12} /><span aria-current="page">{areaLabels[area]}</span></>}
    </nav>
  </header>;
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

export function ProjectOverview({ project, workspace, tasks, onArea, onCapture }: { project: Project; workspace: Workspace; tasks: Task[]; onArea: (area: WorkbenchArea) => void; onCapture: () => void }) {
  const projectTasks = tasks.filter((task) => task.projectId === project.id && !task.deletedAt);
  const open = projectTasks.filter((task) => task.status !== "done");
  return <WorkspacePage eyebrow={projectPath(project, workspace)} title={project.name} description={project.description || "Add a short project description so people and the meeting assistant understand what belongs here."}>
    <div className="overview-metrics project-metrics"><Metric icon={ListTodo} label="Open threads" value={open.length} onClick={() => onArea("threads")} /><Metric icon={CheckCircle2} label="Completed" value={projectTasks.filter((task) => task.status === "done").length} /><Metric icon={CalendarDays} label="Meetings" value="Open" onClick={() => onArea("meetings")} /><Metric icon={FileText} label="Documents" value="Open" onClick={() => onArea("documents")} /></div>
    <section className="project-actions-section"><div className="surface-card-heading"><div><p className="eyebrow">Start from the outcome</p><h2>What needs to happen?</h2></div><small>Every action stays in this project's context.</small></div><div className="project-action-grid">
      <ProjectAction icon={StickyNote} title="Capture an agreement" text="Save a loose note, decision or piece of context before it disappears." status="Ready" onClick={onCapture} />
      <ProjectAction icon={ListTodo} title="Create project work" text="Turn an outcome into a thread with context, priority and a reminder." status="Ready" onClick={onCapture} />
      <ProjectAction icon={UserRoundCheck} title="Delegate work" text="Choose a responsible person, give clear instructions and track acknowledgement." status="Preview" onClick={() => onArea("communication")} />
      <ProjectAction icon={CalendarDays} title="Plan or record a meeting" text="Keep the recording, transcript, decisions and follow-up work together." status="Ready" onClick={() => onArea("meetings")} />
      <ProjectAction icon={Paperclip} title="Add source material" text="Attach a note, link or file that explains the work and its constraints." status="Ready" onClick={() => onArea("documents")} />
      <ProjectAction icon={Send} title="Send an update or question" text="Prepare instructions, information, questions or clarification for a chosen channel." status="Preview" onClick={() => onArea("communication")} />
      <ProjectAction icon={Phone} title="Make a call" text="Prepare and later place a phone, WhatsApp or workspace call with a clear purpose." status="Preview" onClick={() => onArea("communication")} />
    </div></section>
    <div className="overview-grid"><section className="surface-card"><div className="surface-card-heading"><div><p className="eyebrow">Threads</p><h2>Current work</h2></div><button className="text-link" onClick={() => onArea("threads")}>View all <ArrowRight size={14} /></button></div>{open.slice(0, 6).map((task) => <div key={task.id} className="work-preview-row"><span className={`priority-dot ${task.priority}`} /><span><strong>{task.title}</strong><small>{task.notes || "No additional context"}</small></span></div>)}{open.length === 0 && <EmptyPanel title="No open threads" text="Capture work here or move an inbox item into this project." />}</section><section className="surface-card project-memory-card"><p className="eyebrow">One project memory</p><h2>Context follows the action</h2><p>A meeting, message, call or delegated task can use the same people, documents, vocabulary and earlier decisions. The channel changes, but the project context does not.</p><div className="context-flow"><span>Intent</span><ChevronRight size={13} /><span>Context</span><ChevronRight size={13} /><span>Action</span><ChevronRight size={13} /><span>Outcome</span></div></section></div>
  </WorkspacePage>;
}

function ProjectAction({ icon: Icon, title, text, status, onClick }: { icon: typeof LayoutDashboard; title: string; text: string; status: "Ready" | "Preview"; onClick: () => void }) {
  return <button className="project-action" onClick={onClick}><span className="project-action-icon"><Icon size={18} /></span><span><strong>{title}</strong><small>{text}</small></span><span className={`status-pill ${status === "Ready" ? "ready" : "planned"}`}>{status}</span><ArrowRight size={15} /></button>;
}

export function CommunicationWorkspace({ project, workspace, onIntegrations }: { project: Project; workspace: Workspace; onIntegrations: () => void }) {
  const [channel, setChannel] = useState("Email");
  const [intent, setIntent] = useState("Project update");
  const [draft, setDraft] = useState("");
  const channels = [{ name: "Email", icon: Mail }, { name: "WhatsApp", icon: MessageSquareText }, { name: "Slack", icon: MessageSquareText }, { name: "Phone call", icon: Phone }, { name: "Facebook", icon: Send }];
  return <WorkspacePage eyebrow={projectPath(project, workspace)} title="Communication" description="Prepare project communication by intent and recipient. Sending and calling remain disabled until the matching organisation integration is connected.">
    <div className="communication-layout"><section className="surface-card communication-composer"><div className="surface-card-heading"><div><p className="eyebrow">Action preview</p><h2>Prepare communication</h2></div><span className="status-pill planned">Not connected</span></div>
      <label><span>Purpose</span><select value={intent} onChange={(event) => setIntent(event.target.value)}><option>Project update</option><option>Instruction</option><option>Request</option><option>Question</option><option>Clarification</option><option>Decision confirmation</option></select></label>
      <div className="communication-field"><span className="field-label">Channel</span><div className="channel-picker">{channels.map(({ name, icon: Icon }) => <button key={name} className={channel === name ? "active" : ""} onClick={() => setChannel(name)}><Icon size={15} />{name}</button>)}</div></div>
      <label><span>Recipient</span><input placeholder="Choose a person or enter an address after the integration is connected" disabled /></label>
      <label><span>Message or call brief</span><textarea value={draft} onChange={(event) => setDraft(event.target.value)} placeholder={`What should this ${intent.toLowerCase()} communicate?`} /></label>
      <div className="communication-actions"><button className="secondary-button" onClick={onIntegrations}><Plug size={14} />Manage integrations</button><button className="primary-button" disabled><Send size={14} />Connect {channel} to continue</button></div>
    </section><aside className="surface-card delivery-preview"><p className="eyebrow">Designed workflow</p><h2>Controlled, not automatic by surprise</h2><ol><li><strong>Describe the intent</strong><span>Update, instruction, question, request or clarification.</span></li><li><strong>Select people and context</strong><span>Threadbox proposes relevant project facts and attachments.</span></li><li><strong>Review the exact action</strong><span>You see the recipient, channel and final message or call brief.</span></li><li><strong>Send and retain the outcome</strong><span>The delivery result and any reply return to the project activity.</span></li></ol></aside></div>
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
    <ProjectCalendarPanel organization={organization} project={project} onImported={load} onIntegrations={onIntegrations} onError={onError} />
    <div className="meeting-list">{meetings.map((meeting) => <article key={meeting.id} className={`meeting-card ${meeting.status === "recording" ? "recording" : ""}`}><div className={`meeting-status-icon ${meeting.status}`} >{meeting.status === "recording" ? <Mic size={18} /> : meeting.status === "recorded" ? <CheckCircle2 size={18} /> : <CalendarDays size={18} />}</div><div className="meeting-card-main"><div className="meeting-card-heading"><input aria-label="Meeting title" defaultValue={meeting.title} onBlur={(event) => { const next = event.target.value.trim(); if (next && next !== meeting.title) void update(meeting, { title: next }); }} /><span className={`status-pill ${meeting.status}`}>{meeting.status}</span></div><div className="meeting-meta"><span><Clock3 size={13} />{formatMeetingDate(meeting.scheduledStart ?? meeting.startedAt ?? meeting.createdAt)}</span>{meeting.durationSeconds !== null && <span>{formatMeetingDuration(meeting.durationSeconds)}</span>}<label><span>Project</span><select value={meeting.projectId ?? ""} disabled={busyId === meeting.id} onChange={(event) => void update(meeting, { projectId: event.target.value || null })}><option value="">Unassigned</option>{projects.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select></label></div><p className="meeting-track-note">{meeting.recordingPath ? "Stereo source saved: microphone left, system audio right." : meeting.status === "recording" ? "Both audio tracks are being captured now." : "Ready for two-track recording."}</p>{expandedTranscriptId === meeting.id && <MeetingTranscriptPanel meeting={meeting} onTasksChanged={onTasksChanged} onError={onError} />}</div><div className="meeting-actions">{meeting.status === "planned" && <button className="primary-button recording-action" disabled={Boolean(activeRecording) || busyId === meeting.id} onClick={() => void start(meeting)}><Mic size={15} />Record</button>}{meeting.status === "recording" && activeRecording?.id === meeting.id && <button className="stop-meeting-button" onClick={() => void onStop()}><Square size={14} />Stop</button>}{meeting.recordingPath && <button className="secondary-button" onClick={() => void api.playRecording(meeting.recordingPath!).catch((reason) => onError(String(reason)))}><Play size={14} />Play</button>}{meeting.recordingPath && <button className="secondary-button" onClick={() => setExpandedTranscriptId((current) => current === meeting.id ? null : meeting.id)}><FileText size={14} />Transcript</button>}<button className="icon-button danger" aria-label="Delete meeting" disabled={meeting.status === "recording" || busyId === meeting.id} onClick={() => void remove(meeting)}><Trash2 size={16} /></button></div></article>)}{meetings.length === 0 && !creating && <section className="empty-module compact"><div className="module-icon"><CalendarDays size={25} /></div><h2>No meetings in this project</h2><p>Create one now or connect Google Calendar when the integration becomes available.</p><button className="secondary-button" onClick={onIntegrations}><Plug size={15} />Open integrations</button></section>}</div>
  </WorkspacePage>;
}

function ProjectCalendarPanel({ organization, project, onImported, onIntegrations, onError }: { organization: Organization; project: Project; onImported: () => Promise<void>; onIntegrations: () => void; onError: (message: string) => void }) {
  const [connections, setConnections] = useState<IntegrationSnapshot[]>([]);
  const [connectionId, setConnectionId] = useState("");
  const [calendars, setCalendars] = useState<ExternalCalendar[]>([]);
  const [calendarId, setCalendarId] = useState("");
  const [events, setEvents] = useState<ExternalCalendarEvent[]>([]);
  const [busy, setBusy] = useState(false);
  const [mode, setMode] = useState<"events" | "create" | "availability">("events");
  const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone || "Europe/Warsaw";
  const [eventTitle, setEventTitle] = useState("");
  const [eventStart, setEventStart] = useState("");
  const [eventEnd, setEventEnd] = useState("");
  const [attendees, setAttendees] = useState("");
  const [preview, setPreview] = useState<CalendarEventDraft | null>(null);
  const [availabilityCalendars, setAvailabilityCalendars] = useState("");
  const [rangeStart, setRangeStart] = useState("");
  const [rangeEnd, setRangeEnd] = useState("");
  const [duration, setDuration] = useState(30);
  const [slots, setSlots] = useState<AvailableSlot[]>([]);

  const selectedConnection = connections.find((item) => item.connection.id === connectionId) ?? null;
  const hasCapability = (id: IntegrationCapabilityId) => selectedConnection?.capabilities.some((item) => item.capability === id && item.status === "granted") ?? false;

  const loadEvents = useCallback(async (nextConnectionId: string, nextCalendarId: string) => {
    if (!nextConnectionId || !nextCalendarId) return;
    const start = new Date();
    const end = new Date(start.getTime() + 30 * 24 * 60 * 60_000);
    setEvents(await api.listGoogleCalendarEvents(nextConnectionId, nextCalendarId, start.toISOString(), end.toISOString()));
  }, []);

  const load = useCallback(async () => {
    try {
      const nextConnections = (await api.listIntegrationConnections(organization.id)).filter((item) => item.connection.provider === "google");
      setConnections(nextConnections);
      const nextConnection = nextConnections.find((item) => item.connection.id === connectionId) ?? nextConnections[0];
      if (!nextConnection) return;
      setConnectionId(nextConnection.connection.id);
      const canRead = nextConnection.capabilities.some((item) => item.capability === "calendar_read" && item.status === "granted");
      if (!canRead) {
        setCalendarId("primary");
        setAvailabilityCalendars((current) => current || "primary");
        return;
      }
      const nextCalendars = await api.listGoogleCalendars(nextConnection.connection.id);
      setCalendars(nextCalendars);
      const nextCalendar = nextCalendars.find((item) => item.id === calendarId) ?? nextCalendars.find((item) => item.primary) ?? nextCalendars[0];
      if (nextCalendar) {
        setCalendarId(nextCalendar.id);
        setAvailabilityCalendars((current) => current || nextCalendar.id);
        await loadEvents(nextConnection.connection.id, nextCalendar.id);
      }
    } catch (reason) { onError(String(reason)); }
  }, [calendarId, connectionId, loadEvents, onError, organization.id]);

  useEffect(() => { void load(); }, [organization.id]);

  async function chooseCalendar(nextCalendarId: string) {
    setCalendarId(nextCalendarId);
    try { setBusy(true); await loadEvents(connectionId, nextCalendarId); }
    catch (reason) { onError(String(reason)); }
    finally { setBusy(false); }
  }

  async function chooseConnection(nextConnectionId: string) {
    setConnectionId(nextConnectionId);
    setEvents([]);
    setCalendars([]);
    const nextConnection = connections.find((item) => item.connection.id === nextConnectionId);
    if (!nextConnection?.capabilities.some((item) => item.capability === "calendar_read" && item.status === "granted")) {
      setCalendarId("primary");
      setAvailabilityCalendars("primary");
      return;
    }
    try {
      setBusy(true);
      const nextCalendars = await api.listGoogleCalendars(nextConnectionId);
      setCalendars(nextCalendars);
      const nextCalendar = nextCalendars.find((item) => item.primary) ?? nextCalendars[0];
      if (nextCalendar) {
        setCalendarId(nextCalendar.id);
        setAvailabilityCalendars(nextCalendar.id);
        await loadEvents(nextConnectionId, nextCalendar.id);
      }
    } catch (reason) { onError(String(reason)); }
    finally { setBusy(false); }
  }

  async function importEvent(event: ExternalCalendarEvent) {
    try { setBusy(true); await api.importGoogleCalendarEvent(connectionId, project.id, event); await onImported(); }
    catch (reason) { onError(String(reason)); }
    finally { setBusy(false); }
  }

  function prepareEvent() {
    if (!eventTitle.trim() || !eventStart || !eventEnd || !calendarId) return;
    setPreview({ organizationId: organization.id, projectId: project.id, connectionId, calendarId, summary: eventTitle.trim(), description: `Created from Threadbox project ${project.name}.`, start: new Date(eventStart).toISOString(), end: new Date(eventEnd).toISOString(), timeZone: timezone, attendees: attendees.split(",").map((item) => item.trim()).filter(Boolean), addGoogleMeet: true });
  }

  async function createEvent() {
    if (!preview) return;
    try { setBusy(true); await api.createGoogleCalendarEvent(preview); setPreview(null); setEventTitle(""); setEventStart(""); setEventEnd(""); setAttendees(""); await loadEvents(connectionId, calendarId); }
    catch (reason) { onError(String(reason)); }
    finally { setBusy(false); }
  }

  async function findTime() {
    if (!rangeStart || !rangeEnd) return;
    const input: FindTimeInput = { connectionId, calendarIds: availabilityCalendars.split(",").map((item) => item.trim()).filter(Boolean), timeMin: new Date(rangeStart).toISOString(), timeMax: new Date(rangeEnd).toISOString(), durationMinutes: duration, bufferMinutes: 0, timeZone: timezone, workdayStart: "09:00", workdayEnd: "17:00" };
    try { setBusy(true); setSlots(await api.findGoogleCalendarTime(input)); }
    catch (reason) { onError(String(reason)); }
    finally { setBusy(false); }
  }

  if (connections.length === 0) return <section className="calendar-project-panel empty"><CalendarDays size={18} /><div><strong>Google Calendar is not connected</strong><small>Connect it for this organisation to import meetings, create reviewed events and find common time.</small></div><button className="secondary-button" onClick={onIntegrations}>Open integrations</button></section>;
  return <section className="calendar-project-panel"><header><div><strong>Google Calendar</strong><small>{selectedConnection?.connection.accountIdentifier}</small></div><select value={connectionId} disabled={busy} onChange={(event) => void chooseConnection(event.target.value)}>{connections.map((item) => <option key={item.connection.id} value={item.connection.id}>{item.connection.accountIdentifier}</option>)}</select></header><nav><button className={mode === "events" ? "active" : ""} onClick={() => setMode("events")}>Upcoming</button><button className={mode === "create" ? "active" : ""} disabled={!hasCapability("calendar_write")} onClick={() => setMode("create")}>Create event</button><button className={mode === "availability" ? "active" : ""} disabled={!hasCapability("calendar_free_busy")} onClick={() => setMode("availability")}>Find a time</button></nav>
    {mode === "events" && (hasCapability("calendar_read") ? <><label className="calendar-picker"><span>Calendar</span><select value={calendarId} disabled={busy} onChange={(event) => void chooseCalendar(event.target.value)}>{calendars.map((calendar) => <option key={calendar.id} value={calendar.id}>{calendar.summary}{calendar.primary ? " - primary" : ""}</option>)}</select></label><div className="external-event-list">{events.map((event) => <article key={`${event.calendarId}:${event.id}`}><div><strong>{event.summary}</strong><small>{event.allDay ? event.start : formatMeetingDate(event.start)}{event.attendees.length ? ` - ${event.attendees.length} attendees` : ""}</small></div><button className="secondary-button" disabled={busy} onClick={() => void importEvent(event)}>Add to project</button></article>)}{events.length === 0 && <p>No events in the next 30 days.</p>}</div></> : <PermissionPrompt text="Grant Read calendar to list and import events." onIntegrations={onIntegrations} />)}
    {mode === "create" && !preview && <div className="calendar-form"><label><span>Title</span><input value={eventTitle} onChange={(event) => setEventTitle(event.target.value)} /></label><div><label><span>Start</span><input type="datetime-local" value={eventStart} onChange={(event) => setEventStart(event.target.value)} /></label><label><span>End</span><input type="datetime-local" value={eventEnd} onChange={(event) => setEventEnd(event.target.value)} /></label></div><label><span>Attendees</span><input value={attendees} onChange={(event) => setAttendees(event.target.value)} placeholder="alex@example.com, sam@example.com" /></label><button className="primary-button" disabled={!eventTitle.trim() || !eventStart || !eventEnd || !calendarId} onClick={prepareEvent}>Review event</button></div>}
    {mode === "create" && preview && <div className="calendar-preview"><p className="eyebrow">Exact action preview</p><strong>{preview.summary}</strong><span>{formatMeetingDate(preview.start)} - {formatMeetingDate(preview.end)}</span><span>{preview.attendees.length ? preview.attendees.join(", ") : "No attendees"}</span><span>Google Meet link requested</span><div><button className="text-button" disabled={busy} onClick={() => setPreview(null)}>Edit</button><button className="primary-button" disabled={busy} onClick={() => void createEvent()}>{busy ? "Creating..." : "Approve and create"}</button></div></div>}
    {mode === "availability" && <div className="calendar-form"><label><span>Calendars or people</span><input value={availabilityCalendars} onChange={(event) => setAvailabilityCalendars(event.target.value)} placeholder="primary, colleague@example.com" /></label><div><label><span>Range start</span><input type="datetime-local" value={rangeStart} onChange={(event) => setRangeStart(event.target.value)} /></label><label><span>Range end</span><input type="datetime-local" value={rangeEnd} onChange={(event) => setRangeEnd(event.target.value)} /></label><label><span>Minutes</span><input type="number" min="5" max="480" value={duration} onChange={(event) => setDuration(Number(event.target.value))} /></label></div><button className="primary-button" disabled={busy || !availabilityCalendars.trim() || !rangeStart || !rangeEnd} onClick={() => void findTime()}>{busy ? "Checking..." : "Find common time"}</button><div className="available-slots">{slots.slice(0, 20).map((slot) => <button key={slot.start} onClick={() => { setEventStart(toDateTimeLocalValue(slot.start)); setEventEnd(toDateTimeLocalValue(slot.end)); setMode("create"); }}>{formatMeetingDate(slot.start)} - {new Intl.DateTimeFormat(undefined, { timeStyle: "short" }).format(new Date(slot.end))}</button>)}</div></div>}
  </section>;
}

function PermissionPrompt({ text, onIntegrations }: { text: string; onIntegrations: () => void }) {
  return <div className="permission-prompt"><span>{text}</span><button className="secondary-button" onClick={onIntegrations}>Manage permissions</button></div>;
}

function toDateTimeLocalValue(value: string): string {
  const date = new Date(value);
  const offset = date.getTimezoneOffset() * 60_000;
  return new Date(date.getTime() - offset).toISOString().slice(0, 16);
}

export function MeetingRecordingBanner({ meeting, project, onStop }: { meeting: Meeting; project: Project | null; onStop: () => Promise<void> }) {
  const [seconds, setSeconds] = useState(() => elapsedRecordingSeconds(meeting.startedAt));
  const [stopping, setStopping] = useState(false);
  useEffect(() => { const timer = window.setInterval(() => setSeconds(elapsedRecordingSeconds(meeting.startedAt)), 1_000); return () => window.clearInterval(timer); }, [meeting.startedAt]);
  return <aside className="meeting-recording-banner" aria-live="polite"><span className="recording-pulse" /><div><strong>Recording {meeting.title}</strong><small>{project?.name ?? "Unassigned"} - microphone + system audio - {formatMeetingDuration(seconds)}</small></div><button disabled={stopping} onClick={() => { setStopping(true); void onStop().finally(() => setStopping(false)); }}><Square size={14} />{stopping ? "Saving..." : "Stop and save"}</button></aside>;
}

export function VocabularyWorkspace({ project, workspace, onError }: { project: Project; workspace: Workspace; onError: (message: string) => void }) {
  const [sets, setSets] = useState<VocabularySet[]>([]);
  const [terms, setTerms] = useState<VocabularyTerm[]>([]);
  const [candidates, setCandidates] = useState<VocabularyCandidate[]>([]);
  const [selectedSetId, setSelectedSetId] = useState<string | null>(null);
  const [newSetName, setNewSetName] = useState("");
  const [newTerm, setNewTerm] = useState("");
  const [newDefinition, setNewDefinition] = useState("");
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      const [nextSets, nextCandidates] = await Promise.all([api.listVocabularySets(), api.vocabularyCandidates(project.id)]);
      setSets(nextSets);
      setCandidates(nextCandidates);
      setSelectedSetId((current) => current && nextSets.some((set) => set.id === current) ? current : nextSets.find((set) => set.projectIds.includes(project.id) || set.alwaysActive)?.id ?? nextSets[0]?.id ?? null);
    } catch (reason) { onError(String(reason)); }
  }, [onError, project.id]);

  useEffect(() => { void load(); }, [load]);
  useEffect(() => {
    if (!selectedSetId) { setTerms([]); return; }
    void api.listVocabularyTerms(selectedSetId).then(setTerms).catch((reason) => onError(String(reason)));
  }, [onError, selectedSetId]);

  const selectedSet = sets.find((set) => set.id === selectedSetId) ?? null;
  const organizationProjects = workspace.projects.filter((item) => item.organizationId === project.organizationId);

  async function createSet() {
    if (!newSetName.trim()) return;
    setBusy(true);
    try {
      const created = await api.createVocabularySet({ name: newSetName.trim(), projectIds: [project.id] });
      setNewSetName("");
      await load();
      setSelectedSetId(created.id);
    } catch (reason) { onError(String(reason)); } finally { setBusy(false); }
  }

  async function updateSet(set: VocabularySet, patch: Partial<Pick<VocabularySet, "name" | "alwaysActive" | "projectIds">>) {
    try {
      await api.updateVocabularySet(set.id, { name: patch.name ?? set.name, alwaysActive: patch.alwaysActive ?? set.alwaysActive, projectIds: patch.projectIds ?? set.projectIds });
      await load();
    } catch (reason) { onError(String(reason)); }
  }

  async function createTerm(canonicalForm = newTerm, definition = newDefinition) {
    if (!selectedSet || !canonicalForm.trim()) return;
    setBusy(true);
    try {
      if (!selectedSet.alwaysActive && !selectedSet.projectIds.includes(project.id)) {
        await api.updateVocabularySet(selectedSet.id, { ...selectedSet, projectIds: [...selectedSet.projectIds, project.id] });
      }
      await api.createVocabularyTerm({ setId: selectedSet.id, canonicalForm: canonicalForm.trim(), definition: definition.trim() || null, language: project.terminologyLanguage ?? project.language ?? "en", priority: 50 });
      setNewTerm("");
      setNewDefinition("");
      setTerms(await api.listVocabularyTerms(selectedSet.id));
      await load();
    } catch (reason) { onError(String(reason)); } finally { setBusy(false); }
  }

  async function removeSet(set: VocabularySet) {
    if (!window.confirm(`Delete vocabulary set ${set.name}?`)) return;
    try { await api.deleteVocabularySet(set.id); await load(); } catch (reason) { onError(String(reason)); }
  }

  async function dismiss(candidate: VocabularyCandidate) {
    try { await api.dismissVocabularyCandidate(project.id, candidate.text); await load(); } catch (reason) { onError(String(reason)); }
  }

  return <WorkspacePage eyebrow={projectPath(project, workspace)} title="Vocabulary" description="Teach transcription the names and terms that carry meaning in this project.">
    <div className="vocabulary-layout"><aside className="vocabulary-sets"><div className="vocabulary-new-set"><input value={newSetName} onChange={(event) => setNewSetName(event.target.value)} placeholder="New set name" onKeyDown={(event) => { if (event.key === "Enter") void createSet(); }} /><button disabled={!newSetName.trim() || busy} onClick={() => void createSet()}><Plus size={14} /></button></div>{sets.map((set) => <button key={set.id} className={selectedSetId === set.id ? "active" : ""} onClick={() => setSelectedSetId(set.id)}><BookOpen size={15} /><span><strong>{set.name}</strong><small>{set.alwaysActive ? "Always active" : `${set.projectIds.length} projects`} - {set.termCount} terms</small></span></button>)}{sets.length === 0 && <p>No vocabulary sets yet.</p>}</aside>
      <div className="vocabulary-main">{selectedSet ? <><section className="surface-card vocabulary-set-settings"><div className="surface-card-heading"><div><p className="eyebrow">Vocabulary set</p><input defaultValue={selectedSet.name} onBlur={(event) => { const name = event.target.value.trim(); if (name && name !== selectedSet.name) void updateSet(selectedSet, { name }); }} /></div><button className="icon-button danger" title="Delete set" onClick={() => void removeSet(selectedSet)}><Trash2 size={15} /></button></div><label className="vocabulary-check"><input type="checkbox" checked={selectedSet.alwaysActive} onChange={(event) => void updateSet(selectedSet, { alwaysActive: event.target.checked })} />Always use this set in every project</label><div className="vocabulary-projects">{organizationProjects.map((item) => <label key={item.id}><input type="checkbox" disabled={selectedSet.alwaysActive} checked={selectedSet.projectIds.includes(item.id)} onChange={(event) => void updateSet(selectedSet, { projectIds: event.target.checked ? [...selectedSet.projectIds, item.id] : selectedSet.projectIds.filter((id) => id !== item.id) })} />{item.name}</label>)}</div></section>
        <section className="surface-card"><div className="surface-card-heading"><div><p className="eyebrow">Terms</p><h2>Canonical words and variants</h2></div></div><div className="term-create"><input value={newTerm} onChange={(event) => setNewTerm(event.target.value)} placeholder="Canonical term" /><input value={newDefinition} onChange={(event) => setNewDefinition(event.target.value)} placeholder="Short definition (optional)" /><button className="primary-button" disabled={!newTerm.trim() || busy} onClick={() => void createTerm()}>Add term</button></div><div className="vocabulary-terms">{terms.map((term) => <VocabularyTermRow key={term.id} term={term} onSaved={async () => { setTerms(await api.listVocabularyTerms(selectedSet.id)); await load(); }} onError={onError} />)}{terms.length === 0 && <p className="transcript-empty">Add the first term. Nothing is learned without your review.</p>}</div></section>
        <section className="surface-card"><div className="surface-card-heading"><div><p className="eyebrow">From transcripts</p><h2>Candidate terms</h2></div><span className="status-pill">Review required</span></div><div className="vocabulary-candidates">{candidates.map((candidate) => <div key={candidate.text}><span><strong>{candidate.text}</strong><small>{candidate.occurrences} occurrences</small></span><button className="text-button" onClick={() => void dismiss(candidate)}><X size={13} />Dismiss</button><button className="secondary-button" onClick={() => void createTerm(candidate.text, "")}>Accept</button></div>)}{candidates.length === 0 && <p className="transcript-empty">No new recurring or domain-shaped candidates in this project's transcripts.</p>}</div></section></> : <EmptyPanel title="Create a vocabulary set" text="Sets let the same terminology serve one project, several projects or every project." />}</div></div>
  </WorkspacePage>;
}

function VocabularyTermRow({ term, onSaved, onError }: { term: VocabularyTerm; onSaved: () => Promise<void>; onError: (message: string) => void }) {
  const [canonicalForm, setCanonicalForm] = useState(term.canonicalForm);
  const [expansion, setExpansion] = useState(term.expansion ?? "");
  const [definition, setDefinition] = useState(term.definition ?? "");
  const [language, setLanguage] = useState(term.language);
  const [variants, setVariants] = useState(term.variants.join(", "));
  const [priority, setPriority] = useState(term.priority);
  async function save() {
    try { await api.updateVocabularyTerm(term.id, { setId: term.setId, canonicalForm, expansion: expansion || null, definition: definition || null, language, variants: variants.split(",").map((value) => value.trim()).filter(Boolean), priority }); await onSaved(); } catch (reason) { onError(String(reason)); }
  }
  async function remove() { try { await api.deleteVocabularyTerm(term.id); await onSaved(); } catch (reason) { onError(String(reason)); } }
  return <article className="vocabulary-term"><div><input value={canonicalForm} onChange={(event) => setCanonicalForm(event.target.value)} aria-label="Canonical term" /><input value={expansion} onChange={(event) => setExpansion(event.target.value)} placeholder="Expansion" /></div><textarea value={definition} onChange={(event) => setDefinition(event.target.value)} placeholder="Definition used by the meeting assistant" /><div><input value={variants} onChange={(event) => setVariants(event.target.value)} placeholder="Observed variants, comma separated" /><input className="term-language" value={language} onChange={(event) => setLanguage(event.target.value)} placeholder="Language" /><label className="term-priority">Priority <input type="number" min="0" max="100" value={priority} onChange={(event) => setPriority(Number(event.target.value))} /></label><button className="secondary-button" disabled={!canonicalForm.trim()} onClick={() => void save()}>Save</button><button className="icon-button danger" title="Delete term" onClick={() => void remove()}><Trash2 size={14} /></button></div></article>;
}

function MeetingTranscriptPanel({ meeting, onTasksChanged, onError }: { meeting: Meeting; onTasksChanged: () => Promise<void>; onError: (message: string) => void }) {
  const [transcript, setTranscript] = useState<MeetingTranscript | null>(null);
  const [jobs, setJobs] = useState<ProcessingJob[]>([]);
  const [analysis, setAnalysis] = useState<MeetingAnalysis | null>(null);
  const [analysisJobs, setAnalysisJobs] = useState<ProcessingJob[]>([]);
  const [modelStatus, setModelStatus] = useState<LanguageModelStatus | null>(null);
  const [cloudSpeechStatus, setCloudSpeechStatus] = useState<SpeechCloudStatus | null>(null);
  const [speechProvider, setSpeechProvider] = useState<SpeechProvider>("local");
  const [processing, setProcessing] = useState(false);
  const [analysing, setAnalysing] = useState(false);

  const load = useCallback(async () => {
    try {
      const [nextTranscript, nextJobs, nextAnalysis, nextAnalysisJobs, nextModelStatus, nextSettings, nextCloudSpeechStatus] = await Promise.all([
        api.meetingTranscript(meeting.id),
        api.meetingTranscriptionJobs(meeting.id),
        api.meetingAnalysis(meeting.id),
        api.meetingAnalysisJobs(meeting.id),
        api.languageModelStatus(),
        api.getSettings(),
        api.speechCloudStatus(),
      ]);
      setTranscript(nextTranscript);
      setJobs(nextJobs);
      setAnalysis(nextAnalysis);
      setAnalysisJobs(nextAnalysisJobs);
      setModelStatus(nextModelStatus);
      setCloudSpeechStatus(nextCloudSpeechStatus);
      setSpeechProvider(nextSettings.speech.provider === "openai" && !nextCloudSpeechStatus.configured ? "local" : nextSettings.speech.provider);
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
      setTranscript(await api.transcribeMeeting(meeting.id, speechProvider));
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
  const analysisStale = Boolean(transcript && analysis && new Date(transcript.updatedAt).getTime() > new Date(analysis.updatedAt).getTime());
  return <section className="meeting-transcript">
    <div className="meeting-transcript-heading"><div><strong>Source transcript</strong><small>{transcript ? `Microphone: ${transcript.microphoneLanguage} - system: ${transcript.systemLanguage} - ${transcript.modelId}` : "Timestamped microphone and system channels are stored separately."}</small><small>{speechProvider === "local" ? "Local: audio stays on this computer." : "OpenAI cloud: this recording will be uploaded."}</small></div>{!processing && latestJob?.status !== "running" && <div className="transcription-actions"><select aria-label="Transcription privacy" value={speechProvider} onChange={(event) => setSpeechProvider(event.target.value as SpeechProvider)}><option value="local">Local</option><option value="openai" disabled={!cloudSpeechStatus?.configured}>OpenAI cloud</option></select><button className="secondary-button" onClick={() => void transcribe()}>{transcript ? "Transcribe again" : latestJob?.status === "failed" ? "Retry transcription" : "Transcribe recording"}</button></div>}{(processing || latestJob?.status === "running" || latestJob?.status === "queued") && <span className="transcription-state">Processing...</span>}</div>
    {latestJob?.status === "failed" && <p className="transcription-error">{latestJob.error}</p>}
    {transcript && <div className="transcript-segments">{transcript.segments.map((segment) => <div className={`transcript-segment ${segment.channel}`} key={segment.id}><button title="Play from this timestamp" onClick={() => void api.playRecording(meeting.recordingPath!, segment.startMs / 1_000).catch((reason) => onError(String(reason)))}>{formatTranscriptTime(segment.startMs)}</button><span>{segment.channel === "microphone" ? "You" : "Others"}</span><TranscriptSegmentText segment={segment} onSaved={load} onError={onError} /></div>)}</div>}
    {!transcript && latestJob?.status !== "failed" && !processing && <p className="transcript-empty">No transcript yet. Choose the privacy boundary above; Threadbox will use the project language and keep that choice with the queued job.</p>}
    {transcript && <section className="meeting-analysis"><div className="meeting-analysis-heading"><div><strong>Meeting assistant</strong><small>{analysis ? `${analysis.provider} - ${analysis.model} - ${analysis.promptVersion}` : "Notes, decisions, your action items and moments addressed to you."}</small><small className="analysis-provider-summary">{modelStatus?.summary}</small></div>{!analysing && latestAnalysisJob?.status !== "running" && <button className="primary-button" disabled={!modelStatus?.configured} onClick={() => void analyse()}>{analysis ? "Analyse again" : latestAnalysisJob?.status === "failed" ? "Retry analysis" : "Analyse meeting"}</button>}{(analysing || latestAnalysisJob?.status === "running" || latestAnalysisJob?.status === "queued") && <span className="transcription-state">Analysing...</span>}</div>{analysisStale && <p className="analysis-stale">The transcript changed. Analyse again to refresh notes and action items.</p>}{latestAnalysisJob?.status === "failed" && <p className="transcription-error">{latestAnalysisJob.error}</p>}{analysis && <><div className="analysis-notes">{analysis.notes}</div><div className="analysis-items">{analysis.items.map((item) => <article className={`analysis-item ${item.kind}`} key={item.id}><span>{analysisItemLabel(item.kind)}</span><div><strong>{item.title}</strong><p>{item.text}</p><button className="analysis-timestamp" onClick={() => void api.playRecording(meeting.recordingPath!, item.startMs / 1_000).catch((reason) => onError(String(reason)))}>{formatTranscriptTime(item.startMs)}</button>{item.taskId && <small>Thread created</small>}</div></article>)}</div></>}</section>}
  </section>;
}

function TranscriptSegmentText({ segment, onSaved, onError }: { segment: TranscriptSegment; onSaved: () => Promise<void>; onError: (message: string) => void }) {
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(segment.text);
  async function save() {
    if (!text.trim()) return;
    try { await api.correctTranscriptSegment(segment.id, text.trim()); setEditing(false); await onSaved(); } catch (reason) { onError(String(reason)); }
  }
  if (!editing) return <button className="transcript-text" title="Correct this segment" onClick={() => setEditing(true)}>{segment.text}</button>;
  return <div className="transcript-correction"><textarea autoFocus value={text} onChange={(event) => setText(event.target.value)} /><span><button className="text-button" onClick={() => { setText(segment.text); setEditing(false); }}>Cancel</button><button className="secondary-button" onClick={() => void save()}>Save correction</button></span></div>;
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

export function IntegrationsWorkspace({ organization, onError }: { organization: Organization; onError: (message: string) => void }) {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [clientId, setClientId] = useState("");
  const [connections, setConnections] = useState<IntegrationSnapshot[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const load = useCallback(async () => {
    try {
      const [nextSettings, nextConnections] = await Promise.all([api.getSettings(), api.listIntegrationConnections(organization.id)]);
      setSettings(nextSettings);
      setClientId(nextSettings.googleOauthClientId);
      setConnections(nextConnections);
    } catch (reason) { onError(String(reason)); }
  }, [onError, organization.id]);
  useEffect(() => { void load(); }, [load]);
  async function saveClientId() {
    if (!settings) return;
    try { setBusy("client-id"); setSettings(await api.updateSettings({ ...settings, googleOauthClientId: clientId.trim() })); }
    catch (reason) { onError(String(reason)); }
    finally { setBusy(null); }
  }
  async function connect(connectionId?: string) {
    try { setBusy(connectionId ?? "connect"); await api.connectGoogle(organization.id, connectionId); await load(); }
    catch (reason) { onError(String(reason)); }
    finally { setBusy(null); }
  }
  async function changeCapability(connectionId: string, capability: IntegrationCapabilityId, granted: boolean) {
    try {
      setBusy(`${connectionId}:${capability}`);
      if (granted) await api.grantGoogleCapability(connectionId, capability);
      else await api.revokeGoogleCapability(connectionId, capability);
      await load();
    } catch (reason) { onError(String(reason)); }
    finally { setBusy(null); }
  }
  async function disconnect(connectionId: string) {
    if (!window.confirm("Disconnect this Google account and delete its stored token? Imported meetings remain in Threadbox.")) return;
    try { setBusy(connectionId); await api.disconnectGoogle(connectionId); await load(); }
    catch (reason) { onError(String(reason)); }
    finally { setBusy(null); }
  }
  const integrations = [
    { name: "Email", kind: "Messages", text: "Send reviewed updates, requests and instructions, then retain delivery and replies.", icon: Mail, status: "Preview" },
    { name: "Slack", kind: "Workspace", text: "Send messages, ask questions and bring replies back into the project context.", icon: MessageSquareText, status: "Preview" },
    { name: "Phone", kind: "Calls", text: "Place a reviewed call from a prepared brief and retain its result or recording.", icon: Phone, status: "Research" },
  ];
  return <WorkspacePage eyebrow="Organisation" title="Integrations" description={`Connections available to ${organization.name}. Each integration clearly states what it reads, creates and retains.`}>
    <section className="surface-card google-integration"><div className="surface-card-heading"><div><p className="eyebrow">Calendar</p><h2>Google Calendar</h2><p>Connect an account first, then enable only the calendar outcomes you need.</p></div><span className="status-pill ready">Available</span></div>
      <details className="oauth-client-settings" open={!settings?.googleOauthClientId}><summary>Google app configuration</summary><p>Development builds need a Desktop app OAuth client ID. Release builds will include the verified Threadbox client ID, so end users will not configure this.</p><div><input value={clientId} onChange={(event) => setClientId(event.target.value)} placeholder="123456.apps.googleusercontent.com" /><button className="secondary-button" disabled={!settings || !clientId.trim() || busy === "client-id"} onClick={() => void saveClientId()}>Save client ID</button></div></details>
      <div className="google-connections">{connections.filter((item) => item.connection.provider === "google").map((snapshot) => <GoogleConnectionCard key={snapshot.connection.id} snapshot={snapshot} busy={busy} onReconnect={() => void connect(snapshot.connection.id)} onCapability={(capability, granted) => void changeCapability(snapshot.connection.id, capability, granted)} onDisconnect={() => void disconnect(snapshot.connection.id)} />)}
        <button className="secondary-button google-connect" disabled={!settings?.googleOauthClientId || busy !== null} onClick={() => void connect()}><Plus size={15} />{busy === "connect" ? "Finish in browser..." : "Connect Google account"}</button>
      </div>
      <p className="integration-permission-note">Google desktop apps do not support incremental authorization. Adding a capability reopens Google consent for the complete set currently enabled in Threadbox. Revoking a capability blocks it locally; disconnecting deletes the token.</p>
    </section>
    <div className="integration-grid">{integrations.map(({ name, kind, text, icon: Icon, status }) => <section className="integration-card" key={name}><div className="integration-logo"><Icon size={21} /></div><div><p className="eyebrow">{kind}</p><h2>{name}</h2><p>{text}</p></div><span className="status-pill">{status}</span></section>)}</div>
  </WorkspacePage>;
}

const googleCapabilities: Array<{ id: IntegrationCapabilityId; title: string; text: string }> = [
  { id: "calendar_read", title: "Read calendar", text: "List calendars and read selected event details." },
  { id: "calendar_write", title: "Create calendar events", text: "Create reviewed events. Google's scope also technically permits viewing and editing events." },
  { id: "calendar_free_busy", title: "Check availability", text: "Read busy intervals without importing event titles or descriptions." },
];

function GoogleConnectionCard({ snapshot, busy, onReconnect, onCapability, onDisconnect }: { snapshot: IntegrationSnapshot; busy: string | null; onReconnect: () => void; onCapability: (capability: IntegrationCapabilityId, granted: boolean) => void; onDisconnect: () => void }) {
  const granted = (capability: IntegrationCapabilityId) => snapshot.capabilities.some((item) => item.capability === capability && item.status === "granted");
  return <article className="google-account-card"><header><div><strong>{snapshot.connection.displayName}</strong><small>{snapshot.connection.accountIdentifier}</small></div><span className="model-ready"><CheckCircle2 size={14} />Connected</span></header><div className="capability-list">{googleCapabilities.map((capability) => <div key={capability.id}><span><strong>{capability.title}</strong><small>{capability.text}</small></span><button className={granted(capability.id) ? "text-button danger" : "secondary-button"} disabled={busy !== null} onClick={() => onCapability(capability.id, !granted(capability.id))}>{busy === `${snapshot.connection.id}:${capability.id}` ? "Finish in browser..." : granted(capability.id) ? "Revoke" : "Grant"}</button></div>)}</div><footer><button className="text-button" disabled={busy !== null} onClick={onReconnect}>Repair access</button><button className="text-button danger" disabled={busy !== null} onClick={onDisconnect}>Disconnect</button></footer></article>;
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
