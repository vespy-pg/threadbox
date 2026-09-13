import { convertFileSrc } from "@tauri-apps/api/core";

/**
 * Stored media is recorded relative to the media root, so that the data directory can be moved or
 * restored on another machine without rewriting the database. Displaying a file needs the absolute
 * path, which only the desktop side knows, so it is fetched once at startup.
 */
let mediaRoot = "";

export function setMediaRoot(root: string): void {
  mediaRoot = root.replace(/\/+$/, "");
}

const inTauri = (): boolean => "__TAURI_INTERNALS__" in window;

/** The absolute path of a stored reference, left untouched for inline data. */
export function mediaPath(reference: string): string {
  if (reference.startsWith("data:") || reference.startsWith("/") || !mediaRoot) return reference;
  return `${mediaRoot}/${reference}`;
}

/** A value usable as an `src` attribute. */
export function mediaSource(reference: string): string {
  if (reference.startsWith("data:") || !inTauri()) return reference;
  return convertFileSrc(mediaPath(reference));
}
