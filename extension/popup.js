const titleInput = document.querySelector("#title");
const captureButton = document.querySelector("#capture");
const status = document.querySelector("#status");
const message = document.querySelector("#message");

browser.runtime.sendMessage({ type: "ping" }).then((result) => {
  status.textContent = result?.ok ? "Desktop app connected" : "Desktop app not connected";
  status.classList.toggle("error", !result?.ok);
});

browser.tabs.query({ active: true, currentWindow: true }).then(([tab]) => {
  titleInput.value = tab?.title || "";
});

captureButton.addEventListener("click", async () => {
  const [tab] = await browser.tabs.query({ active: true, currentWindow: true });
  const title = titleInput.value.trim();
  if (!title) return titleInput.focus();
  captureButton.disabled = true;
  const result = await browser.runtime.sendMessage({ type: "capture", payload: { title, sourceType: "web", sourceUrl: tab?.url || null, sourceLabel: tab?.title || "Web", sourceAuthor: null, sourceExcerpt: null } });
  message.textContent = result?.ok ? "Saved to your Inbox." : result?.error || "Could not save the task.";
  message.classList.toggle("error", !result?.ok);
  captureButton.disabled = false;
});
