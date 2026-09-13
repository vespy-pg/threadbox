# Information architecture

Status: implemented foundation. Individual modules remain intentionally incomplete.

## Primary product rule

Threadbox exists to make work easier, faster and more obvious. The interface must reduce cognitive
load, show the current organisation and project without requiring recall, and make the next useful
action discoverable. A feature that is powerful but hidden behind an unrelated dialog has not met this
requirement.

## Application hierarchy

An organisation is the highest visible work context. Its stable surfaces are:

- **Overview** - urgent work and meaningful signals aggregated from its projects.
- **People** - employees, collaborators and clients associated with the organisation.
- **Projects** - the project directory and project configuration.
- **Integrations** - connections to external systems, starting with Google Calendar.

A project is selected within an organisation. Its stable surfaces are:

- **Overview** - the project's current state and shortcuts into active work.
- **Threads** - the task and follow-up inbox that formed the original Threadbox application.
- **Meetings** - calendar events, recordings, transcripts, notes, decisions and action items.
- **Documents** - notes, links and files used by people and as assistant context.

The global inbox stays outside projects. It contains captured work that has not been filed yet and is
not a substitute for a project.

## Extensibility rule

New work domains must become named organisation or project surfaces according to their scope. They do
not accumulate as unrelated dialogs in the sidebar. Automatic calls and work distribution are expected
future domains; the shell must accommodate them without another navigation redesign.

## Calendar boundary

Threadbox does not become a calendar provider. Google Calendar is the first integration and supplies
the schedule. Threadbox owns what happens around an event: project context, capture, transcription,
analysis, decisions and resulting work.

## Progressive implementation

The shell may show the stable home of an unfinished module when that helps explain the product, but it
must label the status plainly and must never present a non-working action as available. Visual polish is
continuous; the hierarchy and navigation are established before meeting workflows so those workflows
do not have to be moved later.
