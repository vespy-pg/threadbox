# Meeting agent plan

Status: draft. Not implemented. Read together with `privacy-and-sync-model.md`, which defines the
privacy levels this plan has to fit, and `vocabulary-domain.md`, which describes the terminology
feature the transcription quality depends on.

## The need

Threadbox exists because work arrives through conversations. Meetings are the largest source of such
work and the one Threadbox currently does nothing about. Four distinct failures happen in a meeting,
and they need separating because they have different solutions:

1. **Not everything is remembered afterwards.** Solved by a record: transcript plus notes.
2. **Some subjects are not fully understood while they are discussed.** Solved by explanation against
   a glossary of the user's own domain, not by summarising harder.
3. **Commitments made in the meeting are forgotten.** Solved by extracting the user's own action
   items, distinct from everything else that was agreed.
4. **Even a remembered commitment is not captured, because capturing it is a separate act the user
   has to remember to perform.** Solved only by the agent creating the task itself.

The fourth is the reason this belongs inside Threadbox rather than beside it. A separate tool that
produces a file to import would leave the last gap open, which is the gap that actually loses work.

## What the codebase already provides

More than a new feature would usually start with:

- **System audio capture already works.** `src-tauri/src/native_audio.rs:54` has a `"system"` input
  mode that locates a PulseAudio or PipeWire source whose name contains `monitor` or `loopback`. The
  hard part of recording remote participants is done.
- **Local speech recognition already ships.** `src-tauri/src/speech.rs` runs whisper.cpp through
  `whisper-rs`, with model download and status handling.
- **The platform choice already supports the goal.** Tauri 2 targets Android and iOS as well as the
  desktop, so the stated multi-platform ambition does not require a second application.
- **Media are already files rather than database blobs**, with a media directory next to the SQLite
  database.

Three gaps matter. Capture is currently either the microphone or the system monitor, never both at
once. The recognition language is fixed to Polish and the model is `ggml-small.bin`, neither of which
suits meetings. And the schema has one `tasks` table, with no notion of a project, a meeting or a
vocabulary.

## Capabilities, and how hard each one is

| Capability | Difficulty | Note |
|---|---|---|
| Two-track recording, user and remote participants | Low | Both sources exist, they need to run together |
| Transcript separated into user and remote | Low | One track transcribed per channel, no diarisation needed |
| Notes, decisions, the user's own action items | Medium | Quality depends on which model performs analysis |
| Moments where the user was addressed | Medium | Name detection is reliable, intent detection is not |
| Explaining unfamiliar subjects | Medium | Needs the glossary, and a capable model |
| Assigning a meeting to a project | Medium | Needs projects in the schema and a confirmation step |
| Live view of the current subject | High | A different engine from post-hoc processing |
| Multi-device sync without a hosted service | High | See `privacy-and-sync-model.md` |
| Full parity on iOS | Not achievable | See the platform table below |

## Architecture decisions to take before writing code

**Two channels, not two files, and not a mix.** Record the microphone and the system monitor as the
left and right channel of one recording. Transcribing each channel separately then yields a reliable
split between the user and everyone else, which is most of what speaker separation is needed for,
without any diarisation model and without the token or licence requirements one would bring.
Separating individual remote speakers is a later concern and stays out of the first version.

**The transcript is the source of truth; notes are derived.** Store the transcript with timestamps,
and store notes as artefacts that record which model and which prompt version produced them. Early
results from a small local model will be poor, models will be replaced, and prompts will be rewritten.
Without a preserved transcript, none of those improvements can be applied to past meetings.

**Recognition and analysis sit behind interfaces.** One contract each, with several implementations:
local whisper.cpp, local language model, an external provider, and later the user's own server. No
business logic calls a model directly. This is what makes the privacy levels a matter of
configuration rather than a matter of forking the pipeline, and it is also what allows a free local
tier and a higher-quality tier to coexist.

**Processing is a queue of persisted jobs.** Capture, transcription and analysis are separate steps,
each restartable. A twelve-minute recording took four minutes to transcribe with a large model on a
consumer GPU during evaluation; an hour-long meeting followed by analysis is long enough that power
loss, memory exhaustion and application restarts are ordinary events rather than edge cases.

**Projects are a first-class entity from the start.** Meetings, tasks and vocabulary sets all
reference them, and reconciling a relational graph across devices without stable identifiers from day
one is the kind of mistake that is paid for later.

**Automatic project assignment always ends in a confirmation.** Guessing the project from
participants, calendar entry and vocabulary hits will be right often enough to be useful and wrong
often enough that filing notes silently would be worse than asking.

## Stages

### Stage 1: Local meeting notes on Linux, device-only

The complete useful loop at the strictest privacy level. Two-track recording started from a keyboard
shortcut and a recording indicator; the meeting, project and vocabulary entities in the schema;
recordings stored as content-addressed files; the job queue; language detection and a model
appropriate for meetings, with the vocabulary prompt from `vocabulary-domain.md`; analysis producing
notes, decisions, the user's own action items and the moments where the user was addressed, each
linked to a timestamp in the recording so a doubtful passage can be replayed; and action items
created directly as Threadbox tasks.

A documented import format is still worth defining here, not because this feature needs it, but so
that other capture sources can feed tasks in later. The agent itself does not use it.

### Stage 2: Foundation for multiple devices

The four data layer changes listed in `privacy-and-sync-model.md`, then local network discovery and
device pairing, with a computer acting as the hub. An Android client that reads notes and tasks comes
before an Android client that records, because reading is the more valuable half and by far the
easier one.

### Stage 3: The user's own server

The hub running headless, reachable beyond the local network, and able to accept recognition and
analysis work from a device that cannot perform it. This is also the point at which a phone becomes a
useful recorder for in-person meetings, since it no longer has to process what it captures.

### Stage 4: Live view, then hosted tiers

Streaming recognition with a rolling summary of the current subject and what preceded it, plus alerts
when the user is addressed. This is a second recognition engine tuned for latency rather than
accuracy, which is why it comes after everything above rather than being retrofitted into it. Hosted
storage and hosted processing follow, as defined in `privacy-and-sync-model.md`.

## Platform reality

| | Linux | Windows | macOS | Android | iOS |
|---|---|---|---|---|---|
| Record an online meeting held on this device | Yes | Yes | Needs a virtual audio device | Limited | Only through a user-started broadcast extension, unverified |
| Record an in-person meeting through the microphone | Yes | Yes | Yes | Yes | Yes |
| Local speech recognition | Yes | Yes | Yes | Small models | Small models |
| Local language model analysis | Yes | Yes | Yes | No | No |
| Background synchronisation | Yes | Yes | Yes | Yes | Foreground only |
| Read notes, tasks and the live view | Yes | Yes | Yes | Yes | Yes |

The consequence for iOS is a design rule rather than a limitation to work around: **the device that
records is not assumed to be the device that displays.** With that rule, an iPhone loses one scenario,
recording an online meeting that is being held on the iPhone itself, and keeps everything else. Note
that recording a cellular telephone call is blocked by the platform and no approach changes that.

macOS is included in the table because Tauri makes it nearly free, not because it has been requested.

## Open decisions

- **Local model or external provider for the first version of analysis.** An external provider gives
  a working feature sooner and shows whether the output is worth having; a local model from the start
  avoids discovering late that the private tier is too weak to sell. With the provider interface
  above, this is a question of order rather than a permanent choice.
- **Which model performs analysis at the device-only level**, and whether a 6 GB consumer GPU
  running a quantised model of around eight billion parameters produces action items good enough to
  act on. This needs measuring on real meetings, not estimating.
- **Whether the live view is worth its cost** compared with fast post-meeting notes, which is a
  question about how the user actually behaves in a meeting and cannot be answered from the
  architecture.
