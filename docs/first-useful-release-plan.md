# First useful release plan

Status: approved product direction and implementation plan, recorded on 2026-09-14.

Implementation progress: Milestones 0 through 4 are implemented in code. Milestone 1 still needs one
credentialed OpenAI smoke test. Milestones 2 and 3 still need a verified production Google OAuth
client and credentialed Google account smoke tests. Milestone 4 still needs credentialed end-to-end
tests against Microsoft 365 and an independent IMAP/SMTP host.

## Release outcome

The first useful Threadbox release must turn meetings, email and calendars into project work without
requiring a server operated by the user. It targets Linux and Windows from one codebase and includes:

- local and cloud speech-to-text;
- calendar reading and event creation under separately granted permissions;
- email reading and composing or sending under separately granted permissions;
- Google Calendar and Gmail as the first polished account connection;
- Microsoft 365 mail through Microsoft Graph;
- generic incoming IMAP and outgoing SMTP for other providers;
- free-time search across calendars whose availability the connected accounts are permitted to read;
- the existing local and external language-model analysis options.

A user-owned VPS, Threadbox Node, Slack, click-to-call from a paired phone and automated telephone
conversations remain planned, but they are not release blockers. WhatsApp and Facebook Messenger are
parked indefinitely and must not influence the first release architecture or navigation.

## Product experience

The release is not a full mail client or a replacement calendar. Integrations feed the organisation,
project and meeting workflows:

1. Connect an account in the organisation Integrations surface.
2. Grant only the capability currently needed.
3. Import or link an email, thread, calendar event or availability result to a project.
4. Create a reviewed reply, new message or event from that project context.
5. Record the action and provider result in the project activity.

The connection screen must speak in user outcomes, not protocol names. `Read email`, `Create and send
email`, `Read calendar`, `Create calendar events`, `Check availability`, `Local transcription` and
`Cloud transcription` are separate, visible capabilities. A connection remains useful when only one
of them is granted.

## Permission model

Permissions are stored per account connection, not as one organisation-wide boolean. Revoking a
capability must remove or invalidate its credential grant without deleting already imported project
records.

| Capability | Reads external data | Creates external data | First provider implementation |
|---|---:|---:|---|
| Mail metadata | Yes, headers and identifiers | No | Gmail API |
| Mail content | Yes, bodies and requested attachments | No | Gmail API |
| Mail compose and send | No mailbox read required | Yes, drafts and messages | Gmail API |
| Calendar read | Yes, calendars and events | No | Google Calendar API |
| Calendar event management | Provider scope also permits event read | Yes, events | Google Calendar API |
| Availability | Yes, busy intervals only | No | Google Calendar FreeBusy API |
| Cloud transcription | Yes, sends the selected audio | Creates a transcript in Threadbox only | OpenAI transcription API initially |

Google authentication may identify the account once, but [Google's installed-app flow](https://developers.google.com/identity/protocols/oauth2/native-app)
does not support incremental authorization. Threadbox still enables capabilities one at a time in its own interface;
when another Google capability is added, it reauthorizes the complete union of capabilities currently
enabled for that connection. Gmail read and compose scopes are restricted scopes and create
verification and security-assessment obligations if restricted data is stored or transmitted by a
server. The first release keeps sync and processing on the desktop and requests only the union needed
for the locally enabled capabilities.

Google Calendar has a read-only scope and a combined view-and-edit event scope, but no general
create-only event scope. Threadbox still exposes `Read calendar` and `Manage calendar events` as
separate product capabilities and does not synchronise event content when reading is disabled. The
consent screen must state that Google's management scope technically permits both viewing and editing.
Availability has its own narrower free/busy scope.

Generic mail accounts expose incoming and outgoing access independently. IMAP credentials grant read
and synchronisation. SMTP credentials grant submission. Where a provider supports OAuth, OAuth is
preferred; an app password or password is an explicit compatibility fallback stored only in the
operating system keyring.

## Speech-to-text

Speech recognition and language-model analysis remain two different provider decisions.

### Local transcription

The existing whisper.cpp pipeline remains the privacy-first default. Model download, project language,
terminology hints, two-channel transcription, persisted jobs and retry behaviour stay unchanged.

### Cloud transcription

Add a `SpeechProvider` contract rather than branching meeting logic. The first cloud adapter uses an
official hosted transcription API. Each job records provider, model, language settings, request time,
completion time and whether audio left the device.

Before upload, the interface shows the provider and the recording being sent. Cloud transcription is
never an automatic fallback after local transcription fails. An API credential is stored in the
operating system keyring, and temporary request files are deleted after completion.

The first implementation supports recorded files and uses OpenAI `whisper-1`, because it supplies the
segment timestamps required by Threadbox source transcripts. Each stereo recording is split into
microphone and system tracks before upload. Each track is divided into ten-minute mono WAV chunks that
stay below the [provider's 25 MB file limit](https://developers.openai.com/api/docs/guides/speech-to-text),
and returned timestamps are placed back on the original meeting timeline. Live streaming
transcription is a separate latency-sensitive feature and is not required for this release.

## Mail integration

All providers implement one internal contract:

- list folders or labels;
- synchronise message headers incrementally;
- fetch a selected message body and attachments on demand;
- search messages;
- create a draft;
- send a reviewed message;
- reply while preserving provider thread identifiers;
- report delivery acceptance or a provider error.

Provider adapters are:

1. Gmail API for Gmail and Google Workspace.
2. Microsoft Graph for Outlook and Microsoft 365.
3. IMAP for incoming mail and SMTP submission for other providers.

Threadbox stores provider identifiers and the project-selected material, not an unconditional copy of
the entire mailbox. Initial sync should be bounded by folder, date and count. Background polling runs
only while the desktop application is available. An always-on webhook service belongs to the later
Threadbox Node phase.

## Calendar and availability

The Google Calendar adapter implements:

- account calendar listing;
- bounded incremental event synchronisation;
- event import or linking to a project and meeting;
- event creation after preview;
- attendee, time zone, conferencing link and provider identifier preservation;
- free/busy queries without importing event titles or descriptions when only availability is needed.

`Find a time` takes participants, duration, date range, working hours, time zone and optional buffers.
It intersects busy ranges from every connected or shared calendar and ranks the remaining slots. It
can only include a person when the authenticated account is allowed to see that person's availability.
An email address alone does not grant calendar access. Scheduling polls and negotiation by email are
later fallbacks for people whose availability cannot be read.

Google exposes free/busy independently of full event contents through
`POST /calendar/v3/freeBusy`, with dedicated free/busy scopes. The adapter should prefer that narrower
capability for availability-only users.

## Shared integration foundation

Before provider adapters, add these shared records and services:

- `integration_connections`: organisation, provider, account identity and health;
- `integration_capabilities`: separately granted read, create, send and availability permissions;
- `external_objects`: stable provider identifiers linked to projects, meetings and people;
- `sync_cursors`: per-account incremental synchronisation state;
- `external_actions`: draft, approval, execution and final status;
- `execution_attempts`: immutable provider requests, timestamps, receipts and safe error details.

Secrets never enter these tables. They contain keyring references only. Provider payload logs must be
redacted, and imported content follows the same project and backup rules as native Threadbox content.

## Windows strategy

Windows is a port of the current Tauri application, not a separate product. The React interface,
domain model, SQLite schema, provider contracts, transcription queue and analysis logic remain shared.
Platform-specific behaviour sits behind Rust traits or small target-specific modules.

The current portability audit identifies these Windows adapters:

- replace PulseAudio and PipeWire system-loopback discovery with Windows WASAPI loopback capture;
- replace X11 and the Linux desktop screenshot portal with a Windows capture implementation;
- configure Windows Credential Manager as the keyring backend;
- verify notifications, tray, global shortcuts, autostart, file opening and single-instance behaviour;
- register browser native messaging through the Windows registry if browser capture remains enabled;
- build and sign an NSIS or MSI installer on a Windows build runner.

Tauri already renders the same application through WebView2 and supports Windows installers. The port
therefore needs platform implementation and acceptance testing, not a new UI or second database.
Windows packages should be built on Windows CI rather than treated as a Linux cross-compilation output.

The supported product target is Windows 10 and 11 on x86-64. Older systems are not a release goal even
if the framework can technically start on some of them.

## Implementation sequence

### Milestone 0: contracts and portability gate

Implementation: shared schema, capability enforcement, target-specific dependencies and the Windows
workflow are in place. The missing Windows icon and cross-platform ZIP path regression found by the
first runs are fixed, and the Windows test and installer workflow passes.

- Define provider-neutral speech, mail and calendar contracts.
- Add integration, capability, external-object, sync-cursor and external-action schema migrations.
- Separate the current Linux-only screenshot, audio and keyring dependencies by target.
- Establish a Windows CI job that compiles tests and produces an unsigned development installer.

Acceptance: Linux behaviour is unchanged, the core compiles on Windows, and no provider can bypass
the capability records.

### Milestone 1: local and cloud speech-to-text

Implementation: local and OpenAI cloud paths, separate credential storage, chunked two-track uploads,
explicit controls for meetings and voice notes, persisted job provider/model/language, restart-safe
resumption and transcript provenance are in place. Automated parser, chunking, migration and local
regression tests pass. A real cloud request is intentionally pending until a user API key is supplied.

- Preserve local whisper.cpp as the default.
- Add a cloud speech provider and provider-specific settings.
- Add explicit per-job local or cloud selection, upload disclosure, retry and provenance.
- Verify both providers on voice notes and two-track meeting recordings.

Acceptance: the same recording can be transcribed locally or in the cloud, failed cloud access never
silently falls back or uploads through another provider, and the source recording remains unchanged.

### Milestone 2: Google connection and calendar

Implementation: desktop browser OAuth with PKCE and a random loopback callback, keyring token storage
and refresh, independent local capability grants, calendar/event listing, event import, reviewed event
creation with immutable execution attempts, and multi-calendar free/busy intersection are implemented.
The availability algorithm applies IANA time zones across daylight-saving changes, working hours and
buffers. A verified production OAuth client and credentialed end-to-end tests remain release work.

- Implement desktop OAuth with incremental permission grants and token refresh.
- Add account and permission management to organisation Integrations.
- Read selected calendars and import or link events to meetings and projects.
- Create reviewed events with attendees and time zones.
- Query and display common free slots for calendars visible to the connected account.

Acceptance: read-only users cannot create events, users without the Threadbox read capability do not
receive general event synchronisation even when Google's management scope is broader, revoked grants
stop the affected operation, and free-time results handle time zones and daylight-saving transitions.

### Milestone 3: Gmail

Implementation: the Google account connection now offers independent metadata, content, draft and
send capabilities. The adapter supports bounded header pages, Gmail history cursors with a bounded
full-sync fallback, on-demand body and attachment access, project message and thread links, replies,
reviewed draft creation and reviewed sending. Project imports retain only the material explicitly
selected by the user. A verified OAuth client and credentialed end-to-end tests remain release work.

- Add bounded incremental header synchronisation and on-demand body retrieval.
- Link selected messages and threads to projects and people.
- Create drafts, replies and new messages through separately granted compose or send access.
- Add reviewed sending and immutable execution attempts.

Acceptance: mailbox reading works without send access, sending works without mailbox read access,
attachments require an explicit fetch, and Threadbox never downloads the whole mailbox by default.

### Milestone 4: other mail accounts

Implementation: Microsoft desktop OAuth with PKCE, delegated Graph capabilities, token refresh,
bounded folders and message pages, delta synchronisation, selected body and attachment retrieval,
drafts, replies and reviewed sending use the provider-neutral mail surface. Standards-based accounts
configure incoming IMAP and outgoing SMTP independently, keep both credentials in the operating
system keyring, reject plaintext transport, expose separate connection diagnostics and synchronise a
bounded Inbox window using UIDVALIDITY and UID cursors. Fastmail and Proton Mail Bridge presets plus
manual host settings are available. SMTP does not pretend to provide provider-side drafts. Automated
contract tests pass; real-account acceptance remains release work.

- Add Microsoft Graph with delegated OAuth permissions.
- Add generic IMAP incoming and SMTP outgoing adapters.
- Ship tested presets for common hosts and a manual advanced configuration.
- Add connection diagnostics that distinguish authentication, TLS, incoming and outgoing failures.

Acceptance: at least Gmail, Microsoft 365 and one independent standards-based mail host pass read-only,
send-only and combined end-to-end tests.

### Milestone 5: Windows feature parity

Implementation status: platform adapters, native CI, signed-update controls and the reproducible
desktop release workflow are complete. Publishing requires the release secrets and repository
variables listed in `windows-release.md`; hardware and real-provider acceptance still requires a
clean Windows machine.

- Complete WASAPI microphone and system-loopback recording.
- Complete Windows screenshot, credentials and desktop integration adapters.
- Exercise OAuth callbacks, local and cloud transcription, mail, calendar and free-time search on a
  clean Windows installation.
- Add code signing, update delivery and a reproducible release pipeline.

Acceptance: the same user archive opens on Linux and Windows, all MVP workflows pass on both systems,
and platform-specific failures are stated without Linux terminology.

### Milestone 6: release hardening

Implementation status: privacy receipts now record approved mail and calendar writes plus meeting
audio or transcript data sent to configured cloud providers. Receipts contain metadata and byte
counts when known, never a second copy of the content, and are visible per organisation in
Integrations.

- Complete Google OAuth verification work required by the selected Gmail scopes.
- Add provider rate-limit, token-expiry, offline and partial-sync recovery tests.
- Add privacy receipts showing what left the device and why.
- Run real-device transcription quality and performance checks on supported hardware profiles.
- Finalise backups, migration rollback strategy, onboarding and account disconnection.

Acceptance: a new user can reach the useful loop without documentation, every external write has a
preview, and removing a connection deletes its secrets while preserving the user's project history.

## Deferred work

- user-owned VPS and automated Threadbox Node provisioning;
- always-on webhooks and cross-device synchronisation;
- Slack communication;
- paired Android click-to-call;
- third-party automated telephone conversations;
- live streaming transcription;
- WhatsApp and Facebook Messenger.

The shared connector and external-action contracts must keep the first five items possible without
adding inactive UI or implementation work to this release.

## Primary references

- [Google Calendar FreeBusy API](https://developers.google.com/workspace/calendar/api/v3/reference/freebusy/query)
- [Google Calendar scopes](https://developers.google.com/workspace/calendar/api/auth)
- [Gmail API scopes](https://developers.google.com/workspace/gmail/api/auth/scopes)
- [Gmail sending](https://developers.google.com/workspace/gmail/api/guides/sending)
- [Microsoft Graph mail API](https://learn.microsoft.com/en-us/graph/api/resources/mail-api-overview)
- [OpenAI speech-to-text](https://developers.openai.com/api/docs/guides/speech-to-text)
- [Tauri Windows prerequisites](https://v2.tauri.app/start/prerequisites/)
- [Tauri Windows installers](https://v2.tauri.app/distribute/windows-installer/)
- [Tauri updater](https://v2.tauri.app/plugin/updater/)
- [Tauri Windows code signing](https://v2.tauri.app/distribute/sign/windows/)
