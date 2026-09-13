# Communication integrations

Status: implementation boundary, updated on 2026-09-14. Google Calendar and Gmail now have desktop
adapters, while other external communication remains planned. The active release scope is defined in
`first-useful-release-plan.md`.

## Product boundary

Threadbox should model communication as a project action, not as a separate workflow for every
vendor. A user chooses the purpose, recipient, content and timing first. The organisation integration
then determines which provider can execute it.

Every outbound action needs the same visible lifecycle:

`draft -> awaiting approval -> queued -> sending -> sent or failed -> reply or follow-up`

The stored action should include the organisation, project, recipient identity, sending identity,
channel, exact approved payload, attachments, schedule, provider receipt and every execution attempt.
The application must never turn a preview control into a real external side effect.

## Feasibility by channel

| Channel | Feasibility | Provider boundary | Recommended first scope |
|---|---|---|---|
| Google Calendar | High | Google OAuth and Calendar API | Import project meetings and create reviewed events |
| Gmail | Implemented, awaiting credentialed verification | Google OAuth and Gmail API | Bounded reading, project links, drafts and reviewed sending |
| Slack messages | High | Slack app, OAuth and scoped bot token | Post to selected channels and direct-message conversations |
| Slack calls | Partial | Slack Calls API only represents an external call in Slack | Attach a Threadbox or telephony call link, do not treat Slack as the media provider |
| Email beyond Gmail | High | Microsoft Graph or standard SMTP and IMAP per account type | Add after the Google flow proves the shared action model |
| Telephone network | High | Cloud telephony, SIP provider, or a paired Android phone | Start with a user-confirmed Android dial action or a cloud-provider proof of concept |

Official references:

- [Google Calendar API overview](https://developers.google.com/workspace/calendar/api/guides/overview)
- [Gmail API sending guide](https://developers.google.com/workspace/gmail/api/guides/sending)
- [Slack `chat.postMessage`](https://api.slack.com/methods/chat.postMessage)
- [Slack Calls API](https://api.slack.com/apis/calls)
- [Twilio Voice Call resource](https://www.twilio.com/docs/voice/api/call-resource)

WhatsApp and Facebook Messenger are parked. They are not part of the first useful release and should
not add navigation, provider abstractions or approval complexity until the decision is revisited.

## Telephone integration choices

### Paired Android phone

This is the closest match for calling from the user's existing handset and SIM. A small Android
companion application pairs with Threadbox, receives a reviewed command over an authenticated local
connection and opens the system dialler with the selected number. Android recommends `ACTION_DIAL`,
which leaves the final Call action to the user and avoids the direct-call permission. `ACTION_CALL`
can place the call immediately but needs `CALL_PHONE` and should only be considered for a separately
enabled automation mode.

The desktop can prepare the purpose, contact, script and follow-up. Capturing handset audio or
controlling third-party calling applications is a separate, platform-restricted problem and must not
be implied by the initial dial integration.

References: [Android common intents](https://developer.android.com/guide/components/intents-common),
[minimising Android permissions](https://developer.android.com/privacy-and-security/minimize-permission-requests),
[Android TelecomManager](https://developer.android.com/reference/android/telecom/TelecomManager).

### Cloud telephony

A provider such as Twilio can originate and receive PSTN or SIP calls, connect audio to a desktop or
browser client, emit status callbacks and optionally record calls. This is the practical route for
automated calls, but the visible calling identity belongs to the provider account and must be a
provider number or an allowed verified caller ID. It is not the same as controlling the user's phone.

### SIP or PBX

A user-owned SIP provider or PBX keeps more infrastructure under the user's control and fits the same
Threadbox action interface. It adds provisioning, NAT, audio-device and operations work, so it should
follow the simpler proof of concept.

## Desktop delivery and inbound events

Sending from a desktop application is straightforward. Reliable inbound replies, delivery receipts
and provider webhooks are harder because a laptop may be asleep, offline or unreachable from the
internet. The first version can poll APIs that support it. Full automation needs an optional
always-reachable, user-authorised Threadbox bridge that validates webhooks and delivers events to the
desktop later.

OAuth credentials belong to the operating system keyring. Google Calendar and Gmail share one Google
connection with separately enabled product capabilities. Because [Google does not support incremental
authorization for installed apps](https://developers.google.com/identity/protocols/oauth2/native-app),
adding one capability reauthorizes the union of scopes currently
enabled for that connection. Desktop OAuth uses PKCE, the system browser and a random loopback
callback, never asks the user to paste account passwords into Threadbox.

Gmail keeps metadata reading, content reading, draft creation and sending as separate Threadbox
capabilities. The adapter initially lists at most 30 headers in the project interface and uses a
stored Gmail history ID for later changes. An expired history ID triggers another bounded inbox
listing, not a whole-mailbox download. Opening a message fetches its body only with content access,
and every attachment has its own explicit fetch action. Gmail search also needs content access
because the API does not permit its `q` parameter under the metadata-only scope. Outbound messages
are RFC-compliant MIME payloads encoded for the Gmail API; the exact recipient, subject and body are
stored with the reviewed external action before Gmail is called.

## Local language model on the current computer

Threadbox already supports an OpenAI-compatible local HTTP endpoint and a managed command. Ollama is
the simplest initial runtime because it exposes an OpenAI-compatible endpoint at
`http://127.0.0.1:11434/v1`. `llama.cpp` server remains a lighter alternative when Threadbox should
own the exact executable and model file.

The inspected computer has 30 GiB system RAM and an NVIDIA GeForce RTX 3060 Mobile GPU, but the NVIDIA
driver is currently unavailable to `nvidia-smi`. Ollama and llama.cpp are not installed. CPU inference
is possible but will be substantially slower; GPU setup should be repaired and verified before using
meeting analysis routinely.

Start with `gemma3:4b` (about 3.3 GB of model data) for responsiveness. Test `qwen3:8b` (about 5.2 GB)
only after GPU acceleration works, because model size is not the full video-memory requirement. The
Threadbox local provider should then use:

- endpoint: `http://127.0.0.1:11434/v1`;
- model: `gemma3:4b` initially;
- API key: none for a local Ollama endpoint;
- lifecycle: an existing Ollama service is user-owned, while Threadbox only stops a server process it
  started itself.

References: [Ollama OpenAI compatibility](https://docs.ollama.com/api/openai-compatibility),
[Ollama GPU support](https://docs.ollama.com/gpu), [Gemma 3 model sizes](https://ollama.com/library/gemma3),
[Qwen 3 model sizes](https://ollama.com/library/qwen3), and
[llama.cpp server](https://github.com/ggml-org/llama.cpp/blob/master/tools/server/README.md).

## Later communication implementation order

1. Deliver the mail and calendar foundation in `first-useful-release-plan.md`.
2. Add Slack messaging against the shared external-action contract.
3. Build a paired Android click-to-call proof of concept for calls from the user's handset.
4. Add cloud telephony when automated calling becomes the active milestone.
5. Add the optional inbound webhook bridge when polling is no longer sufficient.

The local model setup is independent and can proceed alongside these integrations after the NVIDIA
driver is operational. A user-owned VPS and automated deployment belong to a later Threadbox Node
milestone.
