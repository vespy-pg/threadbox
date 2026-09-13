import { describe, expect, it } from "vitest";
import { mediaPath, setMediaRoot } from "./media";

describe("mediaPath", () => {
  it("resolves a stored reference against the media root", () => {
    setMediaRoot("/home/user/.local/share/threadbox/media/");
    expect(mediaPath("blobs/ab/abcdef.png")).toBe("/home/user/.local/share/threadbox/media/blobs/ab/abcdef.png");
  });

  it("leaves inline data and absolute paths alone", () => {
    setMediaRoot("/home/user/.local/share/threadbox/media");
    expect(mediaPath("data:image/png;base64,cG5n")).toBe("data:image/png;base64,cG5n");
    expect(mediaPath("/tmp/elsewhere.png")).toBe("/tmp/elsewhere.png");
  });
});
