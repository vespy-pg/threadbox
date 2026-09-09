(function initializeWhatsAppAdapter() {
  const { addCaptureButton, compact, observeMessages } = window.ThreadboxCapture;

  function payload(container) {
    const metadata = container.querySelector("[data-pre-plain-text]")?.getAttribute("data-pre-plain-text") || "";
    const author = compact(metadata.replace(/^\[[^\]]+\]\s*/, "").replace(/:\s*$/, ""), 160);
    const copy = container.cloneNode(true);
    copy.querySelectorAll("[data-threadbox-capture], [aria-label]").forEach((node) => node.remove());
    const text = compact(copy.textContent);
    const conversation = compact(document.querySelector('header [title]')?.getAttribute("title") || document.querySelector("header")?.textContent, 160);
    return {
      title: text.slice(0, 100) || `Follow up with ${conversation || "WhatsApp contact"}`,
      sourceType: "whatsapp",
      sourceUrl: "https://web.whatsapp.com/",
      sourceLabel: conversation || "WhatsApp",
      sourceAuthor: author || null,
      sourceExcerpt: [metadata, text].filter(Boolean).join(" ") || null
    };
  }

  observeMessages(() => {
    document.querySelectorAll('[data-id] [data-pre-plain-text]').forEach((content) => {
      const container = content.closest('[data-id]');
      if (container) addCaptureButton(container, () => payload(container));
    });
  });
})();
