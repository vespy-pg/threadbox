import { formatDistanceToNow, parseISO } from "date-fns";
import type { Task, TaskView } from "./types";

export function taskMatchesView(task: Task, view: TaskView): boolean {
  if (view === "deleted") return Boolean(task.deletedAt);
  if (task.deletedAt) return false;
  if (view === "done") return task.status === "done";
  if (task.status === "done") return false;
  if (view === "inbox") return true;
  return task.priority === view;
}

export function formatDueDate(value: string | null): string {
  if (!value) return "No date";
  const date = parseISO(value);
  const monthInMilliseconds = 30 * 24 * 60 * 60_000;
  if (Math.abs(date.getTime() - Date.now()) <= monthInMilliseconds) {
    return formatDistanceToNow(date, { addSuffix: true });
  }
  return date.toLocaleDateString([], { year: "numeric", month: "short", day: "numeric" });
}

export function toDateTimeLocal(value: string | null): string {
  if (!value) return "";
  const date = new Date(value);
  const offset = date.getTimezoneOffset();
  return new Date(date.getTime() - offset * 60_000).toISOString().slice(0, 16);
}
