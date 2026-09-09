// @vitest-environment jsdom

import { createRoot } from "react-dom/client";
import { act, createElement } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DateTimePicker } from "./DateTimePicker";
import { formatDisplay } from "./DateTimePicker";

let cleanup: (() => void) | null = null;
(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  cleanup?.();
  cleanup = null;
});

describe("formatDisplay", () => {
  it("formats a dated task without combining incompatible Intl options", () => {
    expect(() => formatDisplay(new Date(2026, 8, 8, 16, 30), "24h")).not.toThrow();
    expect(formatDisplay(new Date(2026, 8, 8, 16, 30), "24h")).toContain("2026");
  });

  it("supports a 12-hour clock", () => {
    expect(() => formatDisplay(new Date(2026, 8, 8, 16, 30), "12h")).not.toThrow();
  });

  it("keeps the picker open while changing time and commits with Done", () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const root = createRoot(container);
    const onChange = vi.fn();
    cleanup = () => {
      act(() => root.unmount());
      container.remove();
    };

    act(() => root.render(createElement(DateTimePicker, { value: "2026-09-08T16:30", onChange, clockFormat: "24h" })));
    act(() => (container.querySelector(".date-time-trigger") as HTMLButtonElement).click());
    act(() => (document.querySelector(".clock-face button") as HTMLButtonElement).click());

    expect(document.querySelector(".date-time-popover")).not.toBeNull();
    expect(onChange).not.toHaveBeenCalled();

    const done = Array.from(document.querySelectorAll(".picker-footer button")).find((button) => button.textContent === "Done") as HTMLButtonElement;
    act(() => done.click());

    expect(onChange).toHaveBeenCalledWith("2026-09-08T01:30");
    expect(document.querySelector(".date-time-popover")).toBeNull();
  });

  it("clears the value and closes the picker", () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const root = createRoot(container);
    const onChange = vi.fn();
    cleanup = () => {
      act(() => root.unmount());
      container.remove();
    };

    act(() => root.render(createElement(DateTimePicker, { value: "2026-09-08T16:30", onChange, clockFormat: "24h" })));
    act(() => (container.querySelector(".date-time-trigger") as HTMLButtonElement).click());
    act(() => (document.querySelector(".clear-date") as HTMLButtonElement).click());

    expect(onChange).toHaveBeenCalledWith("");
    expect(document.querySelector(".date-time-popover")).toBeNull();
  });
});
