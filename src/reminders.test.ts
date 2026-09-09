import { describe, expect, it } from "vitest";
import { appendTranscriptNotes, isGeneratedTaskTitle, normalizeLink, reminderDate, sortTasksForView, taskTitle, timestampedTranscript, transcriptTitle } from "./App";
import type { Task } from "./types";

describe("reminder presets", () => {
  const now = new Date(2026, 8, 8, 17, 0, 0);

  it("adds a relative preset to the current time", () => {
    expect(reminderDate("3h", "08:30", now).getTime()).toBe(now.getTime() + 3 * 60 * 60_000);
  });

  it("uses the configured local time tomorrow", () => {
    expect(reminderDate("tomorrow", "09:15", now)).toEqual(new Date(2026, 8, 9, 9, 15, 0));
  });
});

describe("task links", () => {
  it("adds HTTPS to a domain entered without a scheme", () => {
    expect(normalizeLink("example.com/path")).toBe("https://example.com/path");
  });

  it("rejects non-web protocols", () => {
    expect(() => normalizeLink("file:///tmp/note.txt")).toThrow("Only HTTP and HTTPS links are supported.");
  });
});

describe("audio transcripts", () => {
  it("includes the local recording timestamp", () => {
    expect(timestampedTranscript("Buy milk", new Date(2026, 8, 8, 17, 7))).toBe("[Audio transcript 2026-09-08 17:07]\nBuy milk");
  });

  it("uses a complete word for a generated title", () => {
    const transcript = "This is a deliberately long transcription whose title should stop at a complete word";
    expect(transcriptTitle(transcript)).toBe("This is a deliberately long transcription whose...");
  });

  it("keeps a short generated title unchanged", () => {
    expect(transcriptTitle("Call Alice tomorrow")).toBe("Call Alice tomorrow");
  });

  it("uses typed notes when the title is empty or still has its placeholder", () => {
    expect(taskTitle("", "Call Alice tomorrow")).toBe("Call Alice tomorrow");
    expect(taskTitle("New task", "Buy milk on the way home")).toBe("Buy milk on the way home");
  });

  it("keeps an explicit title", () => {
    expect(taskTitle("Shopping", "Buy milk")).toBe("Shopping");
  });

  it("recognizes only an empty or default title as generated", () => {
    expect(isGeneratedTaskTitle("")).toBe(true);
    expect(isGeneratedTaskTitle("New task")).toBe(true);
    expect(isGeneratedTaskTitle("Shopping")).toBe(false);
  });

  it("appends completed transcripts without duplicating them", () => {
    const first = "[Audio transcript 2026-09-08 17:07]\nBuy milk";
    const second = "[Audio transcript 2026-09-08 17:09]\nCall Alice";
    expect(appendTranscriptNotes("Context", [first, first, second])).toBe(`Context\n\n${first}\n\n${second}`);
  });
});

describe("task priority ordering", () => {
  it("sorts by priority and then newest creation time", () => {
    const base: Omit<Task, "id" | "title" | "priority" | "createdAt"> = { notes: "", status: "inbox", sourceType: "manual", sourceUrl: null, sourceLabel: null, sourceAuthor: null, sourceExcerpt: null, dueAt: null, remindAt: null, screenshots: [], audioAttachments: [], links: [], fileAttachments: [], updatedAt: "2026-09-08T10:00:00Z", completedAt: null, deletedAt: null };
    const tasks: Task[] = [
      { ...base, id: "low", title: "Low", priority: "low", createdAt: "2026-09-08T12:00:00Z" },
      { ...base, id: "high-old", title: "High old", priority: "high", createdAt: "2026-09-08T09:00:00Z" },
      { ...base, id: "high-new", title: "High new", priority: "high", createdAt: "2026-09-08T11:00:00Z" },
      { ...base, id: "mid", title: "Mid", priority: "mid", createdAt: "2026-09-08T13:00:00Z" },
    ];
    expect(sortTasksForView(tasks).map((task) => task.id)).toEqual(["high-new", "high-old", "mid", "low"]);
  });
});
