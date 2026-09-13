import { useCallback, useEffect, useMemo, useState } from "react";
import { Building2, ExternalLink, FilePlus2, FileText, Link2, Plus, StickyNote, Trash2, X } from "lucide-react";

import { api } from "./api";
import { fileToDataUrl } from "./attachments";
import { mediaSource } from "./media";
import { speechLanguages } from "./providers";
import type { ContextSharing, Organization, Project, ProjectDocument, ProjectLanguage } from "./types";

export interface Workspace {
  organizations: Organization[];
  projects: Project[];
}

export const emptyWorkspace: Workspace = { organizations: [], projects: [] };

/** The full path of a project, which is what makes one recognisable outside its own tree. */
export function projectPath(project: Project, workspace: Workspace): string {
  const names: string[] = [];
  let current: Project | undefined = project;
  const guard = new Set<string>();
  while (current && !guard.has(current.id)) {
    guard.add(current.id);
    names.unshift(current.name);
    current = workspace.projects.find((item) => item.id === current?.parentId);
  }
  const organization = workspace.organizations.find((item) => item.id === project.organizationId);
  return [organization?.name, ...names].filter(Boolean).join(" / ");
}

export function useWorkspace(onError: (message: string) => void): { workspace: Workspace; reload: () => Promise<void> } {
  const [workspace, setWorkspace] = useState<Workspace>(emptyWorkspace);
  const reload = useCallback(async () => {
    try {
      const [organizations, projects] = await Promise.all([api.listOrganizations(), api.listProjects()]);
      setWorkspace({ organizations, projects });
    } catch (reason) {
      onError(String(reason));
    }
  }, [onError]);
  useEffect(() => { void reload(); }, [reload]);
  return { workspace, reload };
}

/** Chooses the project a task belongs to. No project means the inbox, which is the default. */
export function ProjectSelect({ value, workspace, compact, onChange }: { value: string | null; workspace: Workspace; compact?: boolean; onChange: (projectId: string | null) => void }) {
  const options = useMemo(
    () => workspace.projects.map((project) => ({ id: project.id, label: projectPath(project, workspace) })).sort((first, second) => first.label.localeCompare(second.label)),
    [workspace],
  );
  return <select className={compact ? "project-select compact" : "project-select"} value={value ?? ""} onChange={(event) => onChange(event.target.value || null)}>
    <option value="">No project (inbox)</option>
    {options.map((option) => <option key={option.id} value={option.id}>{option.label}</option>)}
  </select>;
}

export function ProjectsDialog({ workspace, onReload, onClose, onError }: { workspace: Workspace; onReload: () => Promise<void>; onClose: () => void; onError: (message: string) => void }) {
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const selectedProject = workspace.projects.find((project) => project.id === selectedProjectId) ?? null;

  useEffect(() => {
    if (selectedProjectId && !workspace.projects.some((project) => project.id === selectedProjectId)) setSelectedProjectId(null);
  }, [selectedProjectId, workspace.projects]);

  async function run(action: () => Promise<unknown>) {
    try {
      await action();
      await onReload();
    } catch (reason) {
      onError(String(reason));
    }
  }

  function addOrganization() {
    const name = window.prompt("Organisation name");
    if (name?.trim()) void run(() => api.createOrganization({ name: name.trim() }));
  }

  function addProject(organizationId: string, parentId: string | null) {
    const name = window.prompt(parentId ? "Name of the project inside this one" : "Project name");
    if (name?.trim()) void run(async () => { const created = await api.createProject({ organizationId, parentId, name: name.trim() }); setSelectedProjectId(created.id); });
  }

  function removeOrganization(organization: Organization) {
    if (!window.confirm(`Delete ${organization.name} and every project inside it?`)) return;
    void run(() => api.deleteOrganization(organization.id));
  }

  function removeProject(project: Project) {
    if (!window.confirm(`Delete ${project.name} and every project nested inside it? Tasks keep their reference until you move them.`)) return;
    void run(() => api.deleteProject(project.id));
  }

  return <div className="modal-backdrop">
    <div className="settings-dialog projects-dialog">
      <div className="dialog-header">
        <div><p className="eyebrow">Threadbox</p><h2>Projects</h2></div>
        <button className="icon-button" onClick={onClose}><X /></button>
      </div>
      <p className="field-help projects-intro">Organisations keep separate clients apart. A project holds the material a meeting is filed under, and the documents below are what an analysis of that meeting will read first.</p>
      <div className="projects-layout">
        <div className="projects-tree">
          {workspace.organizations.map((organization) => <section key={organization.id} className="organization-block">
            <header>
              <span className="organization-name"><Building2 size={15} />{organization.name}</span>
              <span className="organization-actions">
                <button type="button" className="icon-button" title="Add a project" onClick={() => addProject(organization.id, null)}><Plus size={15} /></button>
                <button type="button" className="icon-button" title="Delete this organisation" onClick={() => removeOrganization(organization)}><Trash2 size={15} /></button>
              </span>
            </header>
            <ProjectBranch organizationId={organization.id} parentId={null} depth={0} workspace={workspace} selectedProjectId={selectedProjectId} onSelect={setSelectedProjectId} onAddChild={addProject} onRemove={removeProject} />
          </section>)}
          {workspace.organizations.length === 0 && <p className="field-help">No organisations yet. Add one for each employer or client whose material must not mix.</p>}
          <button type="button" className="secondary-button add-organization" onClick={addOrganization}><Plus size={15} />Add organisation</button>
        </div>
        <div className="project-detail">
          {selectedProject
            ? <ProjectDetail key={selectedProject.id} project={selectedProject} workspace={workspace} onReload={onReload} onError={onError} />
            : <p className="field-help">Select a project to edit its description, its context sharing and its documents.</p>}
        </div>
      </div>
    </div>
  </div>;
}

function ProjectBranch({ organizationId, parentId, depth, workspace, selectedProjectId, onSelect, onAddChild, onRemove }: { organizationId: string; parentId: string | null; depth: number; workspace: Workspace; selectedProjectId: string | null; onSelect: (id: string) => void; onAddChild: (organizationId: string, parentId: string) => void; onRemove: (project: Project) => void }) {
  const children = workspace.projects.filter((project) => project.organizationId === organizationId && (project.parentId ?? null) === parentId);
  if (children.length === 0) return null;
  return <ul className="project-branch">
    {children.map((project) => <li key={project.id}>
      <div className={`project-node ${project.id === selectedProjectId ? "selected" : ""}`} style={{ paddingLeft: `${8 + depth * 14}px` }}>
        <button type="button" className="project-node-name" onClick={() => onSelect(project.id)}>{project.name}{project.contextSharing !== "inherit" && <span className={`sharing-badge ${project.contextSharing}`}>{project.contextSharing}</span>}</button>
        <span className="project-node-actions">
          <button type="button" className="icon-button" title="Add a project inside this one" onClick={() => onAddChild(organizationId, project.id)}><Plus size={14} /></button>
          <button type="button" className="icon-button" title="Delete this project" onClick={() => onRemove(project)}><Trash2 size={14} /></button>
        </span>
      </div>
      <ProjectBranch organizationId={organizationId} parentId={project.id} depth={depth + 1} workspace={workspace} selectedProjectId={selectedProjectId} onSelect={onSelect} onAddChild={onAddChild} onRemove={onRemove} />
    </li>)}
  </ul>;
}

export function ProjectDetail({ project, workspace, onReload, onError }: { project: Project; workspace: Workspace; onReload: () => Promise<void>; onError: (message: string) => void }) {
  const [name, setName] = useState(project.name);
  const [description, setDescription] = useState(project.description);
  const [documents, setDocuments] = useState<ProjectDocument[]>([]);
  const [noteBody, setNoteBody] = useState("");
  const [scope, setScope] = useState<string[]>([]);
  const [language, setLanguage] = useState<ProjectLanguage | null>(null);

  const loadDocuments = useCallback(async () => {
    try {
      setDocuments(await api.listProjectDocuments(project.id));
      setScope(await api.projectContextScope(project.id));
      setLanguage(await api.projectLanguage(project.id));
    } catch (reason) {
      onError(String(reason));
    }
  }, [onError, project.id]);

  useEffect(() => { void loadDocuments(); }, [loadDocuments]);
  useEffect(() => { setName(project.name); setDescription(project.description); }, [project.description, project.name]);

  async function save(patch: Partial<Project>) {
    try {
      await api.updateProject({ id: project.id, ...patch });
      await onReload();
      setLanguage(await api.projectLanguage(project.id));
    } catch (reason) {
      onError(String(reason));
    }
  }

  async function addDocument(input: Parameters<typeof api.createProjectDocument>[0]) {
    try {
      await api.createProjectDocument(input);
      await loadDocuments();
    } catch (reason) {
      onError(String(reason));
    }
  }

  function addLink() {
    const url = window.prompt("Address of the link");
    if (url?.trim()) void addDocument({ projectId: project.id, kind: "link", url: url.trim() });
  }

  async function addFiles(files: FileList | null) {
    if (!files) return;
    for (const file of Array.from(files)) {
      try {
        await addDocument({ projectId: project.id, kind: "file", fileName: file.name, mimeType: file.type || null, dataUrl: await fileToDataUrl(file) });
      } catch (reason) {
        onError(String(reason));
      }
    }
  }

  async function removeDocument(id: string) {
    try {
      await api.deleteProjectDocument(id);
      await loadDocuments();
    } catch (reason) {
      onError(String(reason));
    }
  }

  const borrowed = scope.filter((id) => id !== project.id);

  return <div className="project-detail-body">
    <label className="field-label" htmlFor="project-name">Name</label>
    <input id="project-name" value={name} onChange={(event) => setName(event.target.value)} onBlur={() => { if (name.trim() && name !== project.name) void save({ name: name.trim() }); }} />

    <label className="field-label" htmlFor="project-description">Description</label>
    <textarea id="project-description" rows={3} value={description} onChange={(event) => setDescription(event.target.value)} onBlur={() => { if (description !== project.description) void save({ description }); }} placeholder="What this project is, in the words you would use to explain it to someone joining it." />

    <label className="field-label" htmlFor="project-sharing">Context sharing</label>
    <select id="project-sharing" value={project.contextSharing} onChange={(event) => void save({ contextSharing: event.target.value as ContextSharing })}>
      <option value="inherit">Inherit from the organisation</option>
      <option value="shared">Shared with the other projects here</option>
      <option value="isolated">Isolated</option>
    </select>
    <p className="field-help">{borrowed.length === 0 ? "Only this project's own material is offered to a model working on it." : `A model working here may also read: ${borrowed.map((id) => { const found = workspace.projects.find((item) => item.id === id); return found ? projectPath(found, workspace) : id; }).join(", ")}.`}</p>

    <label className="field-label" htmlFor="project-language">Meeting language</label>
    <select id="project-language" value={project.language ?? ""} onChange={(event) => void save({ language: event.target.value || null })}>
      <option value="">Inherit</option>
      {speechLanguages.map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
    </select>

    <label className="field-label" htmlFor="project-terminology">Terminology language</label>
    <select id="project-terminology" value={project.terminologyLanguage ?? ""} onChange={(event) => void save({ terminologyLanguage: event.target.value || null })}>
      <option value="">Same as the meeting language</option>
      {speechLanguages.filter((option) => option.value !== "auto").map((option) => <option key={option.value} value={option.value}>{option.label}</option>)}
    </select>
    {language && <p className="field-help">{languageExplanation(language, project, workspace)}</p>}

    <div className="documents-heading">
      <p className="field-label">Documents</p>
      <div className="media-actions">
        <label className="secondary-button file-document-button"><FilePlus2 size={15} />Add file<input className="hidden-file-input" type="file" multiple onChange={(event) => { void addFiles(event.target.files); event.target.value = ""; }} /></label>
        <button type="button" className="secondary-button" onClick={addLink}><Link2 size={15} />Add link</button>
      </div>
    </div>
    <div className="note-composer">
      <textarea rows={2} value={noteBody} onChange={(event) => setNoteBody(event.target.value)} placeholder="Paste a brief, a glossary or anything worth remembering about this project." />
      <button type="button" className="secondary-button" disabled={!noteBody.trim()} onClick={() => { void addDocument({ projectId: project.id, kind: "note", body: noteBody }).then(() => setNoteBody("")); }}><StickyNote size={15} />Add note</button>
    </div>
    <ul className="document-list">
      {documents.map((document) => <li key={document.id} className={`document-row ${document.kind}`}>
        <span className="document-icon">{document.kind === "note" ? <StickyNote size={15} /> : document.kind === "link" ? <Link2 size={15} /> : <FileText size={15} />}</span>
        <div className="document-body">
          <strong>{document.title}</strong>
          {document.kind === "note" && document.body.trim() && <p>{document.body}</p>}
          {document.kind === "link" && document.url && <a href={document.url} target="_blank" rel="noreferrer">{document.url}<ExternalLink size={12} /></a>}
          {document.kind === "file" && document.mediaPath && <button type="button" className="text-button" onClick={() => void api.openMedia(document.mediaPath ?? "").catch((reason) => onError(String(reason)))}>Open{document.sizeBytes ? ` (${Math.max(1, Math.round(document.sizeBytes / 1024))} kB)` : ""}</button>}
          {document.kind === "file" && document.mimeType?.startsWith("image/") && document.mediaPath && <img className="document-thumbnail" src={mediaSource(document.mediaPath)} alt={document.title} />}
        </div>
        <button type="button" className="icon-button" title="Delete this document" onClick={() => void removeDocument(document.id)}><Trash2 size={15} /></button>
      </li>)}
      {documents.length === 0 && <li className="field-help">Nothing here yet. A project brief pasted as a note is the cheapest way to make the first generated meeting notes useful.</li>}
    </ul>
  </div>;
}

/** Says which language recognition will actually use here, and where that answer came from. */
export function languageExplanation(resolved: ProjectLanguage, project: Project, workspace: Workspace): string {
  const spoken = speechLanguages.find((option) => option.value === resolved.language)?.label ?? resolved.language;
  const terminology = speechLanguages.find((option) => option.value === resolved.terminologyLanguage)?.label ?? resolved.terminologyLanguage;
  const sameLanguage = resolved.terminologyLanguage === resolved.language;
  const source = resolved.inheritedFrom === null
    ? "the setting in Settings"
    : resolved.inheritedFrom === project.id
      ? "this project"
      : (() => {
          const owner = workspace.projects.find((item) => item.id === resolved.inheritedFrom);
          return owner ? projectPath(owner, workspace) : "a parent project";
        })();
  const terminologyPart = sameLanguage ? "" : `, with terminology in ${terminology}`;
  return `Recognition here uses ${spoken}${terminologyPart}, from ${source}.`;
}
