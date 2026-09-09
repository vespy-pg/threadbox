# Contributing to Threadbox

Thank you for helping improve Threadbox.

## Development workflow

1. Create a focused branch.
2. Keep website-specific extraction inside `extension/adapters/`.
3. Keep shared browser behavior independent of any website.
4. Add tests for data and date logic changes.
5. Run all checks documented in `README.md` before opening a pull request.

## Privacy and security

- Do not add analytics or remote data processing without an explicit product decision.
- Never log message text, screenshots, email addresses, or native messaging payloads.
- Request the narrowest browser permissions possible.
- Treat all page content as untrusted input.
- Never execute code or HTML obtained from a captured page.

## Language

Source code, documentation, issue templates, and user interface text must be written in English.
