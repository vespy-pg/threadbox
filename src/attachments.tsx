import { Download, FileText, Trash2 } from "lucide-react";
import type { FileAttachment } from "./types";

export const maximumAttachmentBytes = 5 * 1024 * 1024;

export async function filesToAttachments(files: FileList | File[]): Promise<FileAttachment[]> {
  const attachments: FileAttachment[] = [];
  for (const file of Array.from(files)) {
    if (file.size > maximumAttachmentBytes) throw new Error(`${file.name} exceeds the 5 MB file limit.`);
    attachments.push({
      id: crypto.randomUUID(),
      name: file.name,
      mimeType: file.type || mimeTypeFromName(file.name),
      dataUrl: await fileToDataUrl(file),
      sizeBytes: file.size,
      createdAt: new Date().toISOString(),
    });
  }
  return attachments;
}

export function FileAttachmentList({ attachments, onOpen, onDownload, onRemove }: { attachments: FileAttachment[]; onOpen: (attachment: FileAttachment) => void; onDownload: (attachment: FileAttachment) => void; onRemove: (id: string) => void }) {
  return <div className="file-attachments">{attachments.map((attachment) => <div className="file-attachment" key={attachment.id}><button type="button" className="file-open" onClick={() => onOpen(attachment)}><FileText size={17} /><span><strong>{attachment.name}</strong><small>{formatFileSize(attachment.sizeBytes)}</small></span></button><button type="button" className="remove-attachment download-attachment" title="Download file" onClick={() => onDownload(attachment)}><Download size={14} /></button><button type="button" className="remove-attachment" title="Remove file" onClick={() => onRemove(attachment.id)}><Trash2 size={14} /></button></div>)}</div>;
}

function mimeTypeFromName(name: string): string {
  const lower = name.toLocaleLowerCase();
  if (lower.endsWith(".pdf")) return "application/pdf";
  if (lower.endsWith(".md") || lower.endsWith(".markdown")) return "text/markdown";
  if (lower.endsWith(".json") || lower.endsWith(".jsonl")) return "application/json";
  if (lower.endsWith(".csv")) return "text/csv";
  if (lower.endsWith(".tsv")) return "text/tab-separated-values";
  return "text/plain";
}

function fileToDataUrl(file: File): Promise<string> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(reader.error ?? new Error("Could not read the selected file."));
    reader.readAsDataURL(file);
  });
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
