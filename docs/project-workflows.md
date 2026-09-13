# Project work workflows

Status: interaction model and interface preview. Meetings, threads, documents and vocabulary are
implemented. Outbound communication, calls and automated delegation are represented in the shell but
do not execute external actions yet.

## Product model

A project is a shared memory and action context, not a folder containing unrelated tools. Work begins
with an intent, uses project context, performs an action and records its outcome:

`intent -> people and context -> action -> outcome`

The interface should ask what needs to happen before asking which integration should perform it. The
same instruction may be sent by email today and Slack tomorrow without becoming a different kind of
project work.

## Core scenarios

1. **Loose agreement or note.** Capture an observation, assumption, decision or unresolved question.
   It may later become a thread, meeting agenda item, document or outbound message.
2. **Work and delegation.** Create a task, identify the responsible person, attach source material and
   follow its status. Automatic assignment must still show who receives what and why.
3. **Meeting.** Plan or import the event, record it, transcribe it, derive decisions and action items,
   then place the results back into the project.
4. **Document or evidence.** Add a note, link, file, recording or other source that explains a decision
   or constrains an action. The same material can support several later actions without duplication.
5. **Outbound message.** Communicate information, an instruction, request, question, clarification or
   decision confirmation through email, WhatsApp, Slack, Facebook or a later channel.
6. **Call.** Prepare a purpose and brief, select a person and channel, place the call, optionally retain
   the recording or transcript, and capture the result and follow-up work.
7. **Automated follow-up.** Trigger a reviewed sequence such as send a request, wait for a response,
   remind the recipient and escalate to the project owner. Each external side effect remains visible.

## Shared building blocks

These scenarios should compose from the same records instead of creating one schema per integration:

- intent and requested outcome;
- organisation, project and optional parent project;
- participants, responsible person and approver;
- source material, including files and recordings;
- communication channel and external identity;
- draft, approval state and scheduled time;
- execution attempts and provider receipts;
- result, reply, transcript and derived follow-up work.

This suggests a future project activity model with typed actions and immutable execution attempts.
Threads, meetings and documents can link to that activity without being replaced by it.

## Safety and clarity constraints

- Drafting and sending are separate states. Threadbox never makes a real call or sends a message from
  a preview control.
- Before execution, show the exact recipient, identity, channel, content, attachments and schedule.
- Keep incoming replies and delivery failures in the project activity, not only inside provider logs.
- Permissions belong to each organisation integration and state exactly what Threadbox may read and
  change.
- Automation may reduce repeated work, but it must remain interruptible and explain its next action.

## Navigation consequence

Project navigation contains stable modules for Overview, Threads, Meetings, Documents, Communication
and Vocabulary. The Overview begins with outcomes rather than storage types. Communication is the home
for messages and calls across providers, while Integrations remains organisation-level configuration.
This keeps future channels from expanding the project sidebar into one menu item per vendor.
