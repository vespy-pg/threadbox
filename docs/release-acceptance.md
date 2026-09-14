# Release acceptance record

Automated tests prove deterministic behavior, but they cannot prove microphone quality, system
loopback behavior, provider consent or recognition performance on a real computer. Complete one
copy of this record for every release candidate and attach it to the draft release.

## Build under test

- Version:
- Commit:
- Tester:
- Date:
- Operating system and version:
- CPU and RAM:
- Audio devices:

## Automated preflight

- [ ] UI tests, production build, extension build and extension lint pass.
- [ ] Rust formatting, clippy with warnings denied and the full Rust test suite pass.
- [ ] `npm audit --omit=dev` reports no production vulnerabilities.
- [ ] Review the full development audit. The current `web-ext` toolchain reports an upstream
  `image-size` denial-of-service advisory through `addons-linter`. It processes only trusted
  repository extension assets in CI; replace or update it when an upstream fixed release exists.

## Speech recognition profiles

Use the same five-minute sample with names, dates, action items and project vocabulary on each
supported hardware profile. Record wall time, peak memory, transcript language, obvious word error
count and whether the application remained responsive.

| Profile | Model | Audio duration | Wall time | Peak memory | Obvious errors | Responsive | Result |
| --- | --- | ---: | ---: | ---: | ---: | --- | --- |
| Windows baseline | small | | | | | | Not run |
| Windows recommended | medium | | | | | | Not run |
| Linux baseline | small | | | | | | Not run |
| Linux recommended | medium | | | | | | Not run |
| OpenAI cloud comparison | whisper-1 | | | n/a | | | Not run |

## MVP workflow evidence

- [ ] First launch explains projects, local processing and optional provider connections.
- [ ] Organisation, nested project, person, thread, meeting and document survive restart.
- [ ] Microphone, system and combined recording work.
- [ ] Local and cloud transcription show the correct privacy boundary.
- [ ] Google Calendar read, create and multi-person availability work independently.
- [ ] Gmail metadata, body, compose and send permissions work independently.
- [ ] Microsoft metadata, body, compose and send permissions work independently.
- [ ] Independent IMAP-only and SMTP-only accounts work.
- [ ] Every external write has a final preview and an immutable execution attempt.
- [ ] Privacy receipts identify provider, destination, reason and data categories.
- [ ] Disconnect deletes credentials while retaining imported project history.
- [ ] A backup exported on Windows restores on Linux, and the reverse direction works.
- [ ] A signed update installs from the draft release and rejects a modified artifact.

## Outcome

- Result: Not run
- Blocking findings:
- Evidence links:
