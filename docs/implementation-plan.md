# Implementation plan

The desktop meeting agent, built in order. Each step is expected to leave the application working.

## Scope of this phase

Desktop computers only. The user's own machine is the server: it records, stores, transcribes and
analyses. No synchronisation, no separate server, no hosted service, no mobile client, no live view.

Those are not abandoned, and `privacy-and-sync-model.md` states the properties that keep them
reachable. Nothing in this phase may make them harder: identifiers stay stable, media stay
content-addressed and relatively referenced, and recognition and analysis stay behind provider
interfaces.

## Steps

**1. Domain foundation.** Organisations, projects with nesting, people as global records with project
membership, explicit project-to-project context links, and a project reference on tasks. Migrations
become versioned through `user_version` instead of column probing. Defined in `domain-model.md`.

**2. Project content and media discipline.** Documents and notes on a project. Media referenced
relative to the media root and named by content hash, with a migration for the absolute paths currently
stored in `audio_attachments_json` and the screenshot fields.

**3. Provider configuration.** Speech recognition model and language settings; language model provider
as a local model, an external API key, or a locally installed agent tool. First-run choice, changeable
later, keys in the operating system keyring. Defined in `model-providers.md`.

**4. Meetings and two-track recording.** The meeting record, simultaneous capture of the microphone and
the system monitor into two channels of one recording, a visible recording indicator, and assignment to
a project before, during or after the meeting with material moving on reassignment.

**5. Transcription pipeline.** A persisted job queue so long work survives restarts, recognition per
channel, language detection, and the vocabulary prompt from `vocabulary-domain.md`. The transcript is
stored with timestamps and is the source of truth.

**6. Analysis.** Meeting notes, decisions, the user's own action items, and the moments where the user
was addressed, each anchored to a timestamp. Explanations of unfamiliar terms against the project
glossary. Action items become Threadbox tasks directly. Every artefact records the provider, model and
prompt version that produced it.

**7. Vocabulary management.** Sets, terms and their variants in the interface, candidate harvesting from
transcripts, and learning from the user's corrections.

## Status

**Steps 1 to 3 are implemented**, on branch `feature/meeting-agent-domain-foundation`, not merged.
The application shell has subsequently been reorganised around organisations and projects before
starting step 4. `information-architecture.md` records the product structure this establishes.

### Step 1, domain foundation

- Migrations are versioned through `user_version`. Version 1 is the schema as it shipped, applied
  idempotently so existing installations reporting version zero are not disturbed. Version 2 adds the
  tables below and the project reference on tasks.
- New tables: `organizations`, `projects`, `project_context_links`, `people`, `organization_people`,
  `project_people`.
- `tasks` gains a nullable `project_id`, validated against a live project on create and update. A task
  without one is in the inbox, which is the existing behaviour.
- Context scope resolution is implemented as `project_context_scope`: the project itself, its ancestors
  and nested projects, other projects of the same organisation when sharing is effectively enabled on
  both sides, and projects in any organisation reached through an explicit link. `inherit` resolves
  against the nearest ancestor stating a preference, then the organisation.
- One person may be marked as the user, enforced by clearing the flag elsewhere on write.
- Tauri commands are registered for all of the above. No user interface yet: that lands with step 2,
  because a project picker without project content to show is not worth designing twice.
- Moving a project between organisations is rejected with an explicit error rather than half-supported.

Verified with the repository's own checks in `Dockerfile.check`: `cargo fmt --check`, `cargo clippy
--all-targets -- -D warnings`, and `cargo test` with 19 tests passing, 9 of them new for this step. The
graphical application has not been launched, since the host lacks the GTK development libraries and the
project builds in Docker.

### Step 2, project content and media discipline

- Media is content-addressed. Every stored file lives at
  `blobs/<first two hex digits of its sha256>/<the whole sha256><extension>` under the media root, and
  rows record only that relative path.
  Identical bytes stored twice are one file, and the data directory can be moved or restored elsewhere
  without rewriting the database. `src-tauri/src/media.rs` holds the store; resolving a reference
  rejects absolute paths and traversal, so a path arriving from the interface cannot leave storage.
- Migration to `user_version` 3 rewrites the absolute paths that earlier installations recorded in
  `screenshots_json`, `audio_attachments_json` and `file_attachments_json`, moving each file into the
  blob store. A path whose file is already gone is dropped rather than kept as a broken reference, and
  the emptied per-task directories are removed.
- The same migration folds the legacy `screenshot_data_url` column into `screenshots_json` and clears
  it. That column was also a bug: reading a task merged it back into the list on every listing, so a
  task carrying one grew an extra copy of its screenshot each time the list was opened.
- Deleting media is now a collection rather than a directory removal, because blobs are shared. Every
  reference in `tasks` and `project_documents` is gathered and unreferenced blobs are deleted. It runs
  when a task is permanently deleted, when retention purges one, and when an update drops a reference.
  Soft-deleted rows still count as references, so only a permanent delete frees bytes.
- `project_documents` holds notes, links and files on a project, in `src-tauri/src/documents.rs`. A
  note takes its title from its first line when none is given, a link without a scheme is completed and
  validated, and a file is stored in the blob store with its size recorded. Documents move between
  projects; the kind and a file's bytes do not change, so replacing a file means adding another
  document and the earlier one stays citable.
- The interface is organised around the selected organisation and project. Organisation Overview,
  People, Projects and Integrations are stable top-level surfaces; each project has Overview, Threads,
  Meetings and Documents. The old task list is the project's Threads surface, while the global inbox
  remains the home for unassigned work. The task detail and capture form carry a project picker.

Verified with `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test`, now 33
tests with 14 new for this step, plus `tsc --noEmit` and `vitest`, 28 tests with 4 new. The graphical
application has not been launched, for the same reason as in step 1.

### Step 3, provider configuration

- Speech recognition has a model catalogue (small, medium, large v3 turbo) instead of one hard-coded
  file, each with its download size and an honest note, downloaded on demand. A truncated download is
  not counted as installed.
- The recognition language stopped being hard-coded Polish. `auto` detects per recording, a language
  can be stated, and a terminology language can be stated separately. `projects` gains `language` and
  `terminology_language` in `user_version` 4, resolved through the nearest ancestor that states one
  and then the global setting; `project_language` returns the resolved answer and where it came from.
- `src-tauri/src/providers.rs` holds both provider settings groups, their validation and the reachability
  probe. `src-tauri/src/secrets.rs` puts API keys in the operating system keyring, one entry per
  provider, and no path returns a stored key to the interface.
- Settings now carry `speech` and `languageModel`. Both have serde defaults, so a settings file written
  before this step still loads, which a test covers.
- The interface gained a speech section with per-model download, a language model section per provider
  kind with a test button and key management, and a second welcome screen offering both choices once.

The settings for a managed local server, its start command and its idle timeout, are stored and
nothing supervises a process yet: there is nothing to supervise until analysis makes requests. Stated
plainly in `model-providers.md` rather than implied to work.

Verified with the same commands as step 2: 47 Rust tests, 31 interface tests, `tsc --noEmit`, clippy
with `-D warnings`. The graphical application has not been launched, for the same reason as before.

### Organisation-first application shell

- The selected organisation is always visible and controls the Overview, People, Projects and
  Integrations surfaces. Projects are visible in the same navigation rather than hidden in a dialog.
- Selecting a project reveals its Overview, Threads, Meetings and Documents surfaces. Quick capture
  inherits the current project, while the global inbox remains available for unassigned work.
- Organisation Overview aggregates open, overdue and high-priority work from its projects. Meeting
  and Google Calendar surfaces state their next-step status without pretending to be functional.
- People remain canonical global records and gain organisation membership with an optional role in
  schema version 5. The People surface creates and associates people without requiring a project.
- The full project check defaults to one CPU, 2 GB RAM and one Cargo job through `scripts/check.sh`.

Verified with 48 Rust tests, clippy with `-D warnings`, 31 interface tests, `tsc --noEmit` and the
production interface build. The graphical application has not been launched.

- Steps 4 to 7: not started.
