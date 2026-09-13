# Domain model

Status: draft. Implementation in progress, see `implementation-plan.md`.

## Scope of this document

How meeting material, tasks, people and documents are organised, and what "sharing between projects"
means in a single-user desktop application. It does not describe multi-user access control, which is a
different problem and is deliberately absent.

## Two levels, plus nesting

**Organisation** is the top level. It exists because a user works for more than one employer or client
and material must not be mixed between them by accident.

**Project** sits inside an organisation. A project may have a parent project, so a large engagement can
hold workstreams without inventing a third named level. Nesting is by parent reference, not by a fixed
depth, and the interface should discourage going deeper than two or three levels rather than the schema
forbidding it.

Everything that carries content belongs to exactly one project: meetings, transcripts, notes, tasks and
documents. A record with no project is in the inbox, which is the current behaviour of Threadbox and
stays valid.

## Sharing is context scope, not permission

The instinct to configure "whether projects share data" is right, but at a single user the question is
not who may read what. The user may read everything they own. The real question is narrower and more
useful: **what material is assembled as context when a model summarises a meeting, explains a term or
extracts tasks.**

That has direct consequences. Wrong context makes notes worse: terminology from an unrelated client
bleeds into a summary, and the model confidently attributes a decision to the wrong project. Correct
context makes them better: an earlier meeting in the same project is often what makes the current one
comprehensible.

So sharing is modelled as scope, with three settings on a project and two on an organisation:

- An organisation is `isolated` or `shared`. Shared means its projects may draw context from one
  another by default.
- A project is `inherit`, `isolated` or `shared`. `isolated` overrides a shared organisation, which is
  the setting a confidential workstream inside an otherwise open organisation needs.
- Independently, a project may link to specific other projects, in either direction, listed
  explicitly. This covers the case the settings above cannot: two projects in different organisations
  that genuinely share vocabulary or people, such as a supplier and the client they build for.

Context never crosses an organisation boundary unless an explicit project link says so. That is the one
hard rule, because accidental crossing there is the failure with real consequences.

When true multi-user sharing arrives, these settings become the natural place to hang permissions. They
are not permissions today and must not be presented as if they were.

## People are global

A person is a top-level record, not content inside a project. Three reasons, and the first is decisive:

1. The same person appears in several projects and several organisations, and a duplicate record per
   project would fragment exactly the history that makes them worth tracking.
2. Recognising that a participant addressed the user by name requires knowing the user's own name
   variants regardless of which project the meeting belongs to.
3. Name variants feed the vocabulary described in `vocabulary-domain.md`, which is also global.

A person therefore carries a display name, spoken and written variants of it, and a flag marking the
one record that is the user. Membership of a project is a separate association with an optional role.

## Documents and free-form project information

A project holds documents: plain notes typed by the user, files, and links. These serve two purposes,
and the second is the reason they are worth the schema: they are reference material for the user, and
they are context for the model. A project brief pasted as a note measurably improves the first meeting
notes generated for that project.

Documents follow the same media rules as everything else, so their files are content-addressed and
referenced relative to the media root rather than by absolute path.

## A meeting may borrow context from other projects

A meeting belongs to one project, and what is discussed in it does not respect that boundary. A meeting
on one project routinely spends ten minutes on problems belonging to another, and notes written without
that second project's material will be wrong about the part that matters.

Static project links do not cover this, because the overlap is an event rather than a standing
relationship. A meeting therefore carries, in addition to its owning project, a list of projects it may
draw context from. It is set before the meeting when the agenda is known, during the meeting when the
subject turns, and after the meeting when the transcript shows what was actually discussed.

The owning project still decides where the material is filed, who is offered as a participant, and
which vocabulary is loaded first. Borrowed context only widens what the model may read.

## Assigning and moving a meeting

A meeting can be assigned to a project before it starts, while it runs, or after it ends, and
reassignment moves all of its material with it: recording, transcript, notes and any tasks that were
created from it and have not been edited since.

Two rules follow from that. Material is addressed through the meeting rather than copied per project,
so a move is a change of one reference. And a move is recorded, because a meeting filed under the wrong
project and silently moved later is confusing precisely when the user is trying to remember where
something was discussed.

Automatic suggestion of the project is expected to be useful and is never applied without confirmation,
for the reasons given in `meeting-agent-plan.md`.

## Open questions

- Whether an organisation needs its own documents and vocabulary, or whether a root project inside it
  is a sufficient home for shared material.
- Whether borrowed context should be suggested automatically from the transcript, and how a suggestion
  is presented without implying the material has already been filed in the other project.
