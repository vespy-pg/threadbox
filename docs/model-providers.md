# Speech and language model providers

Status: configuration implemented, see the Status section at the end. Analysis itself, and the
supervisor that starts and stops a local server, arrive with the analysis step in
`implementation-plan.md`.

## The shape of the decision

Two independent choices, each changeable at any time without losing data, and each presented once
during first run because neither has a default that suits everyone:

1. **Speech recognition.** Local only, in the first desktop release. Choice of model size and of
   language behaviour.
2. **Language model for analysis.** Local model, an external provider reached with the user's own API
   key, or an agent command line tool the user has already installed and authorised.

Both a local model and an external provider ship, rather than one first and the other later. They are
the same client code behind the same interface, and the choice between them is the user's to make per
installation, not a staging decision.

Both sit behind the provider interfaces described in `meeting-agent-plan.md`, so switching is
configuration. Notes record which provider, model and prompt version produced them, so a switch does
not make earlier output unexplainable.

## Speech recognition

Local recognition through whisper.cpp, which Threadbox already uses in `src-tauri/src/speech.rs`. Two
changes are needed for meetings.

**Language must stop being fixed.** The current call sets Polish unconditionally. Meetings need
per-project language configuration, automatic detection for unknown material, and the ability to state
that a meeting is in one language while its terminology is in another, which is the normal case for
technical work in Polish. `whisper-rs` exposes detection through `FullParams::set_detect_language`,
verified in 0.14.4 at `src/whisper_params.rs:289`.

**Model size must be a choice.** The bundled `ggml-small.bin` is adequate for a dictated task title and
not for an hour of several speakers. Larger models are markedly better and markedly slower, and the
right trade-off depends on hardware the application cannot assume. Offer a small, medium and large
option with an honest statement of speed on the current machine, and keep the small model as the
default for voice notes so the existing feature does not slow down.

Sending audio to a hosted recognition service is not offered on the desktop. Local recognition is good
enough there, and the privacy cost buys nothing.

## Language model: what is offered

**Local model.** Runs on the user's machine, nothing leaves it. Quality is bounded by hardware and
weakest exactly where the user wants help most, which is explaining an unfamiliar subject. This is the
only option that keeps the strictest privacy level intact and it must always remain available. What it
costs while nobody is using it is a real question with a concrete answer, below.

**External provider with the user's own API key.** Anthropic, OpenAI, OpenRouter, and any endpoint
speaking the OpenAI-compatible protocol, which covers most of the rest without special cases. The user
supplies a key; the application never proxies through infrastructure belonging to Threadbox.

**A locally installed agent command line tool.** The application invokes a tool the user has already
installed and signed in to, such as `claude` or `codex`, and reads its output. Their credentials stay
in their own tool and Threadbox never sees them. It costs a process launch per request and gives less
control over model parameters, which is an acceptable trade for not requiring a second payment.

**Official provider sign-in, where the provider publishes one.** A provider slot for an OAuth flow the
provider itself offers for inference. This is how the working examples do it: the GitHub Copilot plugin
signs the user in to GitHub through GitHub's own OAuth and calls GitHub's own service, and the JetBrains
assistant signs in to a JetBrains account. Both are the vendor's own product using the vendor's own
flow. Implement this per provider as such flows become available, rather than assuming none exist.

## Keeping the local model from costing anything when it is idle

A local model is expensive to host and cheap to not host. The expense is not the weights on disk, it
is the process holding them resident: several gigabytes of RAM, or of video memory that other
applications then cannot get. Threadbox is a tray application that runs all day, so a local model that
stays loaded would be the single most expensive thing on the machine, most of the time doing nothing.

Five rules, and the first two do most of the work.

**Inference runs in a separate process, never inside Threadbox.** The local provider speaks the
OpenAI-compatible HTTP protocol to a server on `localhost`. That is the same client code as an external
provider, so there is one code path rather than two. It also means stopping the server returns every
byte to the operating system, which is not true of weights loaded into the application's own address
space, and that a crash in inference cannot take the task inbox with it.

**Nothing is loaded until something is asked, and it is unloaded again afterwards.** No model is
started at login or when the window opens. The first request that needs one starts the server, and the
interface says the model is starting instead of presenting the wait as slow inference. After a
configurable idle period, ten minutes by default, Threadbox stops the server it started. The next
request starts it again, paying the load time once more. The idle cost is then exactly zero: no
process, no resident memory, no reserved video memory, only the weights on disk.

**Analysis is batch work, so it is queued, not chatted with.** The whole analysis of one meeting is a
single job: notes, decisions, action items and term explanations run against a model that is loaded
once and released when the job ends. This matters more than the idle timer, because it is the
difference between one load per meeting and one load per prompt. It also fits the persisted job queue
that transcription needs anyway.

**Threadbox only stops what Threadbox started.** A user who already runs a local model server as a
system service, typically Ollama, owns that process. Threadbox then uses it and never terminates it. It
does set the unload behaviour per request, because with Ollama it is the daemon, not the caller, that
decides how long weights stay resident, and a per-request setting is the only way a caller can
influence that without changing the user's service configuration.

**The state is visible and the user can force it.** The settings show whether a model is currently
loaded and how much memory it holds, offer an explicit unload, and let the idle timeout be changed or
switched off by someone who would rather pay the memory than the load time.

## Login, precisely

Two different things share the name "sign in with", and conflating them is what makes this subject
confusing.

**Identity.** OpenAI's [Sign in with ChatGPT](https://help.openai.com/en/articles/20001410-sign-in-with-chatgpt)
is an OpenID Connect identity layer: it shares name, email address and profile picture, and explicitly
not conversations, tokens or billing. It answers "who is this user", not "may this app use their
subscription".

**Inference on a subscription.** Using a consumer ChatGPT or Claude subscription to generate output from
inside another application is currently available only in the provider's own tools, such as the Codex
command line tool or Claude Code. There is no published integration that grants a third-party
application the same access.

## What is not offered, and why

**Reusing another product's session tokens.** Community libraries exist that borrow the OAuth tokens a
first-party command line tool stored on disk and call the provider's internal endpoints with them. They
work, and they are unaffiliated with the providers, undocumented, outside the terms those tokens were
issued under, and one endpoint change away from breaking. Threadbox does not ship this. A user who wants
subscription pricing uses the locally installed tool route, which reaches the same place through the
front door.

**Driving a chat web interface with stored credentials.** Breaks the providers' terms, breaks whenever
their pages change, and puts Threadbox in possession of the user's primary account password.

## Storing credentials

API keys do not belong in the settings file. They go to the operating system keyring, with the settings
file holding only which provider is selected and a reference. A key must be removable from the
interface, and removing it must actually delete it rather than blanking a field.

Nothing outbound happens silently: the interface shows which provider will be used before a request is
made, and records after the fact that it was.

## Open questions

- Whether a failed external request should fall back to a local model automatically, or stop and ask.
  Automatic fallback quietly changes the privacy properties of a request and is probably wrong.
- Which local server to manage when the user has none installed. Both `llama.cpp` in server mode and
  Ollama speak the protocol above; the choice is about what is reasonable to download and supervise
  from a desktop application, not about the interface.
- Which providers currently publish an inference sign-in flow usable by a third-party application. The
  answer changes over time and should be rechecked rather than assumed.

## Status

Implemented:

- **Speech model is a choice.** Small, medium and large (v3 turbo) are offered with their download
  size and an honest sentence about the trade, each downloaded separately, and a part-finished
  download is not mistaken for an installed model. Voice notes stay on the small model whatever is
  chosen, so the existing capture feature does not slow down.
- **Language is no longer fixed.** Detection per recording is the default, a language can be stated
  instead, and a separate terminology language can be stated when the two differ. A project overrides
  both, resolved through the nearest ancestor that states one and finally the global setting, so a
  surprising language is traceable to where it was set.
- **The language model provider is configurable** as a local server, an external provider with the
  user's own key, or an agent command line tool. Each has its own fields, and a test button asks the
  configured provider whether it is reachable. That button is the only thing that reaches a provider;
  nothing else does until analysis exists.
- **Keys live in the operating system keyring**, one entry per provider so switching provider does not
  discard the other key. The settings file records only which provider is selected. A key can be
  removed, which deletes the credential, and no code path returns a stored key to the interface.
- **The choice is offered once during the first run**, as the second screen of the welcome flow, and
  skipping it is allowed: nothing breaks until analysis is asked for.

Recorded but not yet acted on:

- `managed` and the idle timeout for a local server are stored, and nothing starts or stops a process
  yet. The supervisor those settings describe is part of the analysis step, because there is nothing
  to supervise until something makes requests. Until then the local option means a server the user
  runs.
- Official provider sign-in has no implementation, because the survey above found no published flow a
  third-party application may use for inference. The provider list is the place to add one.
