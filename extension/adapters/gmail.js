(function initializeGmailAdapter() {
  const { addCaptureButton, compact, observeMessages } = window.ThreadboxCapture;

  function payload(container) {
    const body = compact(container.querySelector('.a3s, [data-message-id] .ii')?.textContent || container.textContent);
    const authorNode = container.querySelector('.gD, [email]');
    const author = compact(authorNode?.getAttribute("email") || authorNode?.textContent, 160);
    const subject = compact(document.querySelector('h2.hP, [data-thread-perm-id] h2')?.textContent, 240);
    return {
      title: subject ? `Reply: ${subject}` : body.slice(0, 100) || "Follow up on an email",
      sourceType: "gmail",
      sourceUrl: location.href,
      sourceLabel: subject || "Gmail",
      sourceAuthor: author || null,
      sourceExcerpt: body || null
    };
  }

  observeMessages(() => {
    document.querySelectorAll('.adn, [data-message-id]').forEach((container) => {
      if (container.closest('[data-threadbox-capture]')) return;
      addCaptureButton(container, () => payload(container));
    });
  });
})();
