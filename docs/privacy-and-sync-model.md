# Privacy and sync model

Status: draft. Not implemented. This document records product decisions before code is written.

## Why this document exists

Threadbox today is a single-device local-first task inbox. Two planned changes break that
assumption: synchronisation between a user's devices, and the meeting agent described in
`meeting-agent-plan.md`, which adds recordings, transcripts and machine-generated notes of
conversations involving other people.

Meeting content is a different class of data from a task title. A transcript can contain
commercial terms, patient or client references, opinions about colleagues, and the words of people
who never installed Threadbox. Anything the product does with that content has to be a deliberate
choice, not a side effect of a feature.

`CONTRIBUTING.md` requires an explicit product decision before remote data processing is added to
Threadbox. This document is that decision.

## The two variables behind every privacy level

Users think in terms of trust. Code has to work in terms of placement. There are only two
placement questions, and every privacy level is a named combination of their answers:

1. **Where does the archive live?** Which machines hold recordings, transcripts, notes and tasks.
2. **Where does the compute run?** Which machine performs speech recognition and language-model
   analysis.

A third question is independent of both and must never be folded into them:

3. **May content be sent to an external language-model provider?**

The reason for keeping the third separate is that it does not follow from the first two. A user who
keeps everything on one laptop may still want the best available model for a difficult meeting. A
user who accepts hosted storage may still refuse to let any third-party model see the transcript.
Treating "cloud" as a single slider would make both of those users unserviceable.

The implementation therefore knows about three configuration values, not four levels. The four
levels are presets over those values, presented in the interface. Presets rather than free
configuration, because the combinations that are coherent are few and the ones that are merely
confusing are many.

## Level 1: This device only

**Requires** a desktop or laptop. A phone alone cannot host this level: it can record a meeting held
in the room, but it cannot capture the audio of a meeting running on a computer, and running a
useful language model on it is not realistic.

**Gives** the strongest guarantee the product can make. No account, no network traffic, no
identifiers. Speech recognition and analysis run on the machine that recorded the meeting.

**Costs** access from any other device, and any protection against losing the machine. There is no
backup in this level by definition, so the interface must offer an explicit local export and say
plainly that a lost or wiped disk means lost history. Analysis quality is bounded by the hardware,
which for a consumer GPU means a small quantised model and results measurably weaker than a hosted
frontier model.

## Level 2: Own devices, no server

**Requires** at least one computer. That computer is the hub: it holds the full archive and performs
the compute. Phones are thin peers that contribute recordings of in-person meetings and display
notes and tasks.

**Gives** the archive on the machine the user already trusts, plus phone access to notes, tasks and
the live meeting view, with no third party involved at any point.

**Costs** availability. Two devices can only reconcile when they can reach each other, which in
practice means the same local network. A phone that leaves the network shows the state it had when
it left, and contributes its recordings on return. Reaching devices across the internet without any
intermediary is not possible in the general case, because both sides usually sit behind address
translation; a user who wants that is choosing level 3. On iOS this level is further limited to
reconciliation while the application is in the foreground, since the platform does not permit
background synchronisation daemons.

## Level 3: Own server

**Requires** a machine the user controls that is reachable over the network. Two shapes qualify and
both must be supported, because they suit different people:

- The user's own computer, promoted to a reachable hub through a private network overlay or a
  tunnel. Data never leaves hardware the user owns.
- A rented server or a network storage appliance running Threadbox headless. Convenient and always
  available, but the hosting provider has physical access to the disk, which is a real difference
  from the first shape and must be stated in the interface rather than hidden behind the same label.

**Gives** synchronisation from anywhere, off-site backup, and the option to move compute off a weak
device: a phone-only meeting can be transcribed by the user's own server instead of the phone.

**Costs** operational responsibility. The user runs, updates and secures the machine. This level
should therefore never be the default and never be presented as the easy option.

## Level 4: Hosted service

This level splits into two, and the split is the most important distinction in this document,
because the two halves differ in what the operator can see.

**Level 4a, hosted storage.** The service stores and relays encrypted data and holds no key that can
open it. Compute stays on the user's own devices. The operator can see how much data a user has and
when they sync, and nothing else. This is the level that should be the paid default.

**Level 4b, hosted processing.** The service receives audio or transcripts in a form it can read,
because speech recognition and analysis run on its hardware. This is what makes the product work for
someone whose only device is a phone, and it is the only level at which the operator holds meeting
content. It requires a data processing agreement, a retention policy, deletion on request, and a
clear statement of which sub-processors are involved. It must be opt-in per account and reversible.

**Gives** zero setup, availability everywhere, backup, frontier-model quality regardless of the
user's hardware, and the foundation for later team features.

**Costs** for 4a, trust that the client encryption is correct and a recurring fee. For 4b,
additionally, the operator's infrastructure holds conversations that involve people who are not
users of the product.

## The external language model axis

Independent of the level above, with three settings:

- **Local models only.** Nothing leaves the device for analysis. Weaker notes, and the topic
  explanation feature described in `meeting-agent-plan.md` degrades the most, because explaining an
  unfamiliar subject is exactly where a small model is weakest.
- **External model, per meeting.** Off by default. The user marks an individual meeting as eligible,
  and only that transcript is sent. This should be the recommended setting: it matches how the need
  actually arises, which is one difficult meeting rather than all of them.
- **External model, always.** Convenient, and an informed choice for users whose meetings are not
  confidential.

Speech recognition is deliberately not on this axis. Local speech recognition is good enough on any
computer, so sending audio to a third party for transcription buys nothing on a computer and is only
considered when the recording device is a phone operating at level 3 or 4b.

Whatever the setting, a request to an external model must record which provider, which model and
which prompt version produced a note, so that a user can later tell what was sent where.

## Invariants that hold at every level

These are the rules the implementation may not break, whichever level is active:

1. **Level 1 is a complete product.** Every feature that can work without a network works without a
   network. No feature is degraded to push users up the levels.
2. **The level is per installation, and lowering it is possible.** Moving from a hosted level back
   to device-only must export the archive and leave the service holding nothing.
3. **No silent transmission.** Any outbound request carrying user content is visible in the
   interface before it happens and recorded after it.
4. **Encryption is the client's job.** No level relies on the operator behaving well for
   confidentiality, except 4b, where server-side reading is the explicit purpose.
5. **Recording is visible to the user who records.** A recording indicator and a per-meeting record
   of what was captured are core features, not settings. What participants are told is the user's
   responsibility, but the product must never make it easy to forget that recording is happening.

## Data layer work this requires before any level above 1

Four properties are missing from the current schema. All four are cheap now, with 29 tasks and 4 MB
of media in a development database, and expensive after the first release that has users.

1. **Content-addressed media with relative paths.** `audio_attachments_json` and the screenshot
   fields currently store absolute filesystem paths in a field named `dataUrl`, for example
   `/home/<user>/.local/share/threadbox/media/...`. Such a record is meaningless on another
   platform, another account or another device. Media must be named by a hash of their contents and
   referenced relative to a media root. The field name should change with the meaning.
2. **A change log and a schema version.** Migrations in `src-tauri/src/database.rs` detect missing
   columns through `PRAGMA table_info`. That is adequate for a single device and insufficient for
   reconciliation, which needs to know what changed since two devices last met. Record-level
   `updated_at` and `deleted_at` already exist and are the right foundation; an append-only
   operation log and a `user_version` are not present and should be added before they are needed.
3. **A device identity.** A keypair generated on first run, with a human-readable device name. This
   is what makes level 4a possible later; retrofitting identity into an installed base is far
   harder than generating a key that goes unused for a year.
4. **A platform-neutral core.** Capture, storage paths and notification must sit behind interfaces,
   with PulseAudio, X11 and desktop-only assumptions confined to platform modules. Android and iOS
   clients are a stated goal and the core must not have to be rewritten for them.

## Documents that become wrong when this ships

`SECURITY.md` currently states that the application does not intentionally transmit task content,
screenshots or audio to a Threadbox service. That sentence is accurate today and must be revised in
the same change that introduces any level above 2, rather than left to contradict the product.

## Open questions

- Which level is the default for a new installation. Level 1 is the honest default and level 2 is
  the more useful one.
- Whether level 2 attempts internet reconciliation through address translation at all, or whether
  local network only is stated plainly and users who want more are pointed at level 3.
- Whether the hosted service ever offers a shared team archive, which is a different product with
  different obligations and should not be assumed by this model.
