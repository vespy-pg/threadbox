(function initializeThreadboxShared() {
  const BUTTON_ATTRIBUTE = "data-threadbox-capture";

  function compact(value, limit = 1200) {
    return String(value || "").replace(/\s+/g, " ").trim().slice(0, limit);
  }

  function addCaptureButton(target, payloadFactory) {
    if (!target || target.querySelector(`:scope > [${BUTTON_ATTRIBUTE}]`)) return;
    const button = document.createElement("button");
    button.type = "button";
    button.setAttribute(BUTTON_ATTRIBUTE, "true");
    button.className = "threadbox-capture-button";
    button.title = "Add message to Threadbox";
    button.setAttribute("aria-label", "Add message to Threadbox");
    button.innerHTML = '<span aria-hidden="true">T</span>';
    button.addEventListener("click", async (event) => {
      event.preventDefault();
      event.stopPropagation();
      button.dataset.state = "sending";
      try {
        const payload = payloadFactory();
        const response = await browser.runtime.sendMessage({ type: "capture", payload });
        if (!response?.ok) throw new Error(response?.error || "Threadbox did not accept the task");
        button.dataset.state = "saved";
        button.innerHTML = '<span aria-hidden="true">✓</span>';
        button.title = "Saved to Threadbox";
      } catch (error) {
        button.dataset.state = "error";
        button.title = String(error);
      }
    });
    target.appendChild(button);
  }

  function observeMessages(scan) {
    let queued = false;
    const schedule = () => {
      if (queued) return;
      queued = true;
      requestAnimationFrame(() => {
        queued = false;
        scan();
      });
    };
    const observer = new MutationObserver(schedule);
    observer.observe(document.documentElement, { childList: true, subtree: true });
    scan();
    return observer;
  }

  window.ThreadboxCapture = { addCaptureButton, compact, observeMessages };
})();
