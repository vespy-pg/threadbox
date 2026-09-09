(function initializeSlackAdapter() {
  const { addCaptureButton, compact, observeMessages } = window.ThreadboxCapture;

  function messagePermalink(container) {
    const direct = container.querySelector('a[href*="/archives/"][href*="p"]')?.href;
    if (direct) return direct;
    const channel = location.pathname.match(/\/client\/[^/]+\/([^/]+)/)?.[1];
    const timestamp = container.dataset.ts || container.getAttribute("data-item-key")?.replace(/^.*-/, "");
    if (!channel || !timestamp) return location.href;
    return `${location.origin}/archives/${channel}/p${timestamp.replace(".", "")}`;
  }

  function payload(container) {
    const text = compact(container.querySelector('[data-qa="message-text"], [data-qa="message_content"], .c-message_kit__text')?.textContent || container.textContent);
    const author = compact(container.querySelector('[data-qa="message_sender_name"], .c-message__sender_link')?.textContent, 160);
    const channel = compact(document.querySelector('[data-qa="channel_name"], [data-qa="channel_header_title"]')?.textContent, 160);
    return {
      title: text.slice(0, 100) || "Follow up on a Slack message",
      sourceType: "slack",
      sourceUrl: messagePermalink(container),
      sourceLabel: channel || "Slack",
      sourceAuthor: author || null,
      sourceExcerpt: text || null
    };
  }

  observeMessages(() => {
    document.querySelectorAll('[data-qa="message_container"], .c-virtual_list__item[data-item-key]').forEach((container) => addCaptureButton(container, () => payload(container)));
  });
})();
