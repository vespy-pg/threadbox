import { describe, expect, it } from "vitest";
import { languageExplanation, projectPath, type Workspace } from "./projects";
import type { Organization, Project } from "./types";

const organization = (id: string, name: string): Organization => ({
  id, name, notes: "", contextSharing: "isolated",
  createdAt: "2026-09-12T08:00:00Z", updatedAt: "2026-09-12T08:00:00Z", deletedAt: null,
});

const project = (id: string, organizationId: string, name: string, parentId: string | null): Project => ({
  id, organizationId, parentId, name, description: "", contextSharing: "inherit",
  language: null, terminologyLanguage: null,
  createdAt: "2026-09-12T08:00:00Z", updatedAt: "2026-09-12T08:00:00Z", deletedAt: null,
});

const workspace: Workspace = {
  organizations: [organization("aptvision", "Aptvision"), organization("nhs", "NHS")],
  projects: [
    project("engage", "aptvision", "Engage Hub", null),
    project("api", "aptvision", "API", "engage"),
    project("orphan", "aptvision", "Detached", "missing-parent"),
  ],
};

describe("projectPath", () => {
  it("spells a project out from its organisation down", () => {
    expect(projectPath(workspace.projects[0], workspace)).toBe("Aptvision / Engage Hub");
    expect(projectPath(workspace.projects[1], workspace)).toBe("Aptvision / Engage Hub / API");
  });

  it("still names a project whose parent is missing", () => {
    expect(projectPath(workspace.projects[2], workspace)).toBe("Aptvision / Detached");
  });
});

describe("languageExplanation", () => {
  const engageHub = workspace.projects[0];
  const nestedApi = workspace.projects[1];

  it("names the global setting when no project states a language", () => {
    expect(languageExplanation({ language: "auto", terminologyLanguage: "auto", inheritedFrom: null }, nestedApi, workspace))
      .toBe("Recognition here uses Detect per recording, from the setting in Settings.");
  });

  it("names the ancestor a language was inherited from, and the terminology language", () => {
    expect(languageExplanation({ language: "pl", terminologyLanguage: "en", inheritedFrom: engageHub.id }, nestedApi, workspace))
      .toBe("Recognition here uses Polish, with terminology in English, from Aptvision / Engage Hub.");
  });

  it("says this project when it states its own", () => {
    expect(languageExplanation({ language: "en", terminologyLanguage: "en", inheritedFrom: nestedApi.id }, nestedApi, workspace))
      .toBe("Recognition here uses English, from this project.");
  });
});
