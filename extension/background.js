const NATIVE_HOST = "com.threadbox.capture";

browser.runtime.onInstalled.addListener(() => {
  browser.contextMenus.create({
    id: "threadbox-add-selection",
    title: "Add selection to Threadbox",
    contexts: ["selection"]
  });
  browser.contextMenus.create({
    id: "threadbox-add-page",
    title: "Add page to Threadbox",
    contexts: ["page", "link"]
  });
});

function sendNative(payload) {
  return browser.runtime.sendNativeMessage(NATIVE_HOST, { action: "capture", ...payload })
    .then((response) => response || { ok: false, error: "Threadbox returned no response" })
    .catch((error) => ({ ok: false, error: `${error.message}. Start Threadbox once to install the browser connection.` }));
}

browser.runtime.onMessage.addListener((message, sender) => {
  if (message?.type === "capture") {
    if (message.payload?.sourceType !== "whatsapp" || !sender.tab?.windowId) return sendNative(message.payload);
    return browser.tabs.captureVisibleTab(sender.tab.windowId, { format: "jpeg", quality: 45 })
      .then((screenshot) => sendNative({ ...message.payload, screenshotDataUrl: screenshot.length < 700_000 ? screenshot : null }))
      .catch(() => sendNative(message.payload));
  }
  if (message?.type === "ping") {
    return browser.runtime.sendNativeMessage(NATIVE_HOST, { action: "ping" })
      .then(() => ({ ok: true }))
      .catch((error) => ({ ok: false, error: error.message }));
  }
  return undefined;
});

browser.contextMenus.onClicked.addListener((info, tab) => {
  const excerpt = (info.selectionText || "").trim();
  const title = excerpt.slice(0, 100) || tab?.title || "Follow up on a web page";
  void sendNative({
    title,
    sourceType: "web",
    sourceUrl: info.linkUrl || tab?.url || null,
    sourceLabel: tab?.title || "Web",
    sourceAuthor: null,
    sourceExcerpt: excerpt || null
  });
});
