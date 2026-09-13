import { afterEach, describe, expect, it, vi } from "vitest";
import { formatDueDate, taskMatchesView } from "./date";
import type { Task } from "./types";

const task = (overrides: Partial<Task>): Task => ({
  id: "1",
  title: "Test task",
  notes: "",
  status: "todo",
  priority: "mid",
  projectId: null,
  sourceType: "manual",
  sourceUrl: null,
  sourceLabel: null,
  sourceAuthor: null,
  sourceExcerpt: null,
  dueAt: null,
  remindAt: null,
  screenshots: [],
  audioAttachments: [],
  links: [],
  fileAttachments: [],
  createdAt: "2026-09-08T08:00:00.000Z",
  updatedAt: "2026-09-08T08:00:00.000Z",
  completedAt: null,
  deletedAt: null,
  ...overrides,
});

describe("taskMatchesView", () => {
  it("includes every active priority in Inbox", () => {
    expect(taskMatchesView(task({ priority: "high" }), "inbox")).toBe(true);
    expect(taskMatchesView(task({ priority: "low" }), "inbox")).toBe(true);
  });

  it("does not include completed tasks in active views", () => {
    expect(taskMatchesView(task({ status: "done" }), "inbox")).toBe(false);
  });

  it("filters active tasks by priority", () => {
    expect(taskMatchesView(task({ priority: "high" }), "high")).toBe(true);
    expect(taskMatchesView(task({ priority: "low" }), "high")).toBe(false);
  });

  it("shows removed tasks only in Deleted", () => {
    const removed = task({ deletedAt: "2026-09-08T12:00:00.000Z" });
    expect(taskMatchesView(removed, "deleted")).toBe(true);
    expect(taskMatchesView(removed, "inbox")).toBe(false);
  });
});

describe("formatDueDate", () => {
  afterEach(() => vi.useRealTimers());

  it("uses relative time for dates within a month", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-09-08T12:00:00.000Z"));
    expect(formatDueDate("2026-09-08T14:00:00.000Z")).toBe("in about 2 hours");
  });

  it("uses a calendar date after a month", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-09-08T12:00:00.000Z"));
    expect(formatDueDate("2026-11-12T14:00:00.000Z")).toContain("2026");
  });
});
