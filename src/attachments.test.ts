import { describe, expect, it } from "vitest";
import { filesToAttachments, maximumAttachmentBytes } from "./attachments";

describe("file attachments", () => {
  it("rejects files larger than 5 MB before reading them", async () => {
    const file = { name: "large.pdf", type: "application/pdf", size: maximumAttachmentBytes + 1 } as File;
    await expect(filesToAttachments([file])).rejects.toThrow("exceeds the 5 MB file limit");
  });

  it("accepts any file type because previews use system applications", async () => {
    const file = { name: "archive.zip", type: "application/zip", size: 100 } as File;
    class TestFileReader {
      result: string | null = null;
      error: Error | null = null;
      onload: (() => void) | null = null;
      onerror: (() => void) | null = null;

      readAsDataURL() {
        this.result = "data:application/zip;base64,AA==";
        this.onload?.();
      }
    }
    const original = globalThis.FileReader;
    globalThis.FileReader = TestFileReader as unknown as typeof FileReader;
    try {
      const attachments = await filesToAttachments([file]);
      expect(attachments[0]).toMatchObject({ name: "archive.zip", mimeType: "application/zip", sizeBytes: 100 });
    } finally {
      globalThis.FileReader = original;
    }
  });
});
