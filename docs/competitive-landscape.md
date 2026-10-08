# Competitive landscape

Status: initial market scan

Last updated: 2026-09-14

## Product baseline

This analysis treats a product as a direct competitor when it covers a meaningful part of the core
Threadbox workflow:

1. Capture or import a meeting.
2. Transcribe it and produce structured notes, decisions, and action items.
3. Preserve the result in organization or project context.
4. Suggest, assign, or execute follow-up work.

Generic AI automation products are not direct competitors merely because they connect to calendars,
email, or Slack.

## Closest direct competitors

### Greg

Greg is currently the closest public match to the complete Threadbox direction. It joins Google
Meet, Zoom, and Microsoft Teams, produces speaker-labelled transcripts, summaries, decisions, and
owned action items, then places tasks on project boards. It can post to Slack, draft email, update a
CRM, create calendar holds, communicate over SMS, and call the user with preparation or debrief
briefings.

The main visible differences are that Greg is bot-first and cloud-first, is strongly oriented toward
sales and CRM workflows, and does not present privacy routing between local, private-server, and
cloud processing as part of its product model.

Source: <https://getgreg.io/>

### Notion AI Meeting Notes and Custom Agents

Notion is the strongest platform threat. Its desktop application captures system audio without a
meeting bot, transcribes and summarizes meetings, extracts decisions and action items, and stores
them beside projects, documents, databases, and tasks. Meeting completion can trigger Custom Agents
that update project trackers, post recaps, create tickets, and perform other workspace actions.

Notion already owns the surrounding workspace in many organizations. Threadbox cannot differentiate
only by putting meeting notes next to projects. Its stronger opportunities are simpler workflows,
privacy-aware local processing, evidence and approval for suggested actions, and communication
across providers rather than primarily within one configurable workspace.

Sources:

- <https://www.notion.com/product/ai-meeting-notes>
- <https://www.notion.com/releases/2026-07-31>
- <https://www.notion.com/help/ai-meeting-notes>

### MeetGeek

MeetGeek records with or without a bot, transcribes, summarizes, extracts decisions and action
items, and pushes results into connected tools. It advertises agentic workflows, custom agents,
voice agents, automated Gmail follow-up, Google Calendar actions, and connectivity through Make,
Zapier, and n8n.

MeetGeek overlaps heavily with the meeting-to-automation pipeline. Its public positioning is meeting
intelligence and cross-application automation rather than a calm, project-first operating space.

Source: <https://meetgeek.ai/>

### Meeting.ai

Meeting.ai captures online and in-person meetings, produces transcripts, minutes, action items, and
visual summaries, and gives its agent a cloud computer to create finished decks, documents,
spreadsheets, PDFs, and research based on meeting context. It also includes calendar, contacts,
files, shared workspaces, and searchable meeting history.

It is a close competitor for the idea that meeting output should become completed work rather than
remain a transcript. Its emphasis is deliverable generation rather than persistent project
operations, people management, and controlled communication.

Sources:

- <https://meeting.ai/en>
- <https://meeting.ai/en/docs/agent/your-agent>

### Fellow

Fellow captures online and in-person meetings through bots, desktop botless recording, native Zoom
capture, or mobile recording. It generates recaps and suggested action items, maintains an internal
action-item list, and synchronizes accepted actions with project tools such as Asana, Jira, Linear,
Trello, ClickUp, and Microsoft To Do. Ask Fellow can search meeting history, edit workspace content,
and complete supported tasks through chat.

Fellow is especially strong in agendas, meeting governance, enterprise compliance, and integrations.
Its requirement to accept AI-generated action items before synchronizing them is a useful safety
pattern for Threadbox.

Sources:

- <https://fellow.ai/>
- <https://help.fellow.ai/en/articles/2477293-fellow-glossary>

### Sembly AI

Sembly captures meetings and produces transcripts, structured notes, decisions, risks, requirements,
and action items with owners and deadlines. Collections group meetings by client, project, sprint,
or engagement. Tasks can be pushed to Salesforce, HubSpot, Asana, Zapier, or MCP-connected tools.

This is a direct competitor for project-grouped meeting knowledge and action extraction, although
the actual work usually continues in an external system.

Source: <https://www.sembly.ai/product/meetings/>

### Circleback

Circleback provides meeting transcription, notes, assigned action items, a universal task list,
cross-meeting search, and post-meeting automations. Its automations can send notes to Slack, update
Notion and CRMs, create Linear issues or monday.com items, and invoke webhooks.

It directly competes with the capture-to-action pipeline, but it is more meeting-centric than
organization-and-project-centric and relies on integrations for most downstream execution.

Sources:

- <https://circleback.ai/>
- <https://support.circleback.ai/en/articles/10460573-getting-started-with-automations>

### Fireflies.ai

Fireflies provides live transcription, notes, action-item assignment, meeting search, and a large
integration catalogue. It can automatically create project-management or Microsoft To Do tasks and
write meeting notes and action items into CRM records.

It is a mature direct competitor for meeting capture and automated task handoff. Its core remains a
meeting intelligence layer feeding external tools rather than the primary project workspace.

Source: <https://fireflies.ai/product/real-time>

### Granola

Granola is a botless meeting notebook connected to calendars. It combines user notes with a meeting
transcript, organizes meetings into team spaces and folders, drafts Gmail follow-ups, sends notes to
Slack and CRMs, creates project tasks through Zapier, and exposes meeting context to other agents
through MCP.

It competes strongly on low-friction capture and note quality. Its action execution and project
structure are still primarily supplied through other tools.

Sources:

- <https://www.granola.ai/integrations>
- <https://docs.granola.ai/help-center/getting-started/granola-101>

## Platform threats

### Microsoft 365 Copilot

Teams Copilot generates meeting summaries, decisions, and action items. Microsoft 365 Copilot
Cowork can separately send email, schedule meetings, create documents, post to Teams, manage files,
and run scheduled work. The pieces already exist inside one vendor ecosystem even if the public
workflow is not yet as coherent as a dedicated meeting-to-action product.

Sources:

- <https://support.microsoft.com/en-US/teams/copilot/catch-up-on-meetings-with-microsoft-365-copilot-in-teams>
- <https://support.microsoft.com/en-us/Microsoft-365-Copilot/get-started-with-cowork>

### Google Workspace with Gemini

Google Meet captures notes, decisions, next steps, and assigned tasks into Google Docs and attaches
them to Calendar events. It is currently weaker at executing arbitrary follow-up work, but it has a
large distribution advantage and already controls the calendar, meeting, email, document, and chat
surfaces used by many prospective customers.

Source: <https://support.google.com/meet/answer/14754931>

## Local and privacy-focused competitors

These products compete directly with local capture and private transcription, but their public
offerings generally stop before broad project operations or execution:

- Earshot - open-source Windows and macOS recording, local storage, notes, and reviewed action-item
  suggestions: <https://tryearshot.app/>
- Private Notetaker - Windows and macOS meeting transcription, summaries, and action items entirely
  on the device: <https://privatenotetaker.com/>
- Local First - meeting minutes, decisions, action items, folders, and local or Swiss processing:
  <https://www.localfirst.ch/en.html>
- LokalBot - local meeting memory, decisions, action items, work timeline, and an optional approved
  server path: <https://www.lokalbot.com/>
- Daisy - on-device recording, diarization, transcripts, summaries, decisions, and action items:
  <https://www.daisylocal.app/>
- Kapinote - local-first macOS capture, transcription, structured notes, decisions, action items, and
  grounded search: <https://kapinote.com/>

## Competitive conclusion

The meeting transcription and summary layer is already crowded and is not a defensible product on
its own. Even meeting-to-task export is common.

The strongest Threadbox differentiation should be the complete combination of:

- A project-first and organization-first work context rather than an archive of unrelated meetings.
- Meetings as one evidence source alongside email, documents, messages, calls, and manual notes.
- Suggested actions that retain a link to the source statement, decision, and responsible person.
- Explicit review and granular permission gates before external actions are performed.
- Useful built-in follow-through rather than requiring the user to assemble Zapier workflows.
- A clear privacy route for each operation: on-device, later private server, or selected cloud model.
- Provider-neutral calendar and email access instead of locking the workflow to one office suite.
- An organization overview that shows decisions, risks, overdue commitments, and cross-project work.

The closest single current comparison is Greg. The strongest incumbent threat is Notion. MeetGeek,
Fellow, Sembly, Circleback, Fireflies, and Granola define the expected baseline for capture quality,
meeting search, and integrations.

This scan is based on publicly advertised functionality and does not verify every paid feature or
product claim through hands-on testing.
