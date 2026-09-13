# Vocabulary domain

Status: draft. Not implemented.

## Why this exists

Speech recognition fails hardest on exactly the words that carry the meaning of a sentence: product
and project names, ticket prefixes, drug and device names, acronyms, and the names of colleagues. A
transcript that renders every ordinary word correctly and mangles the one term the sentence is about
is close to useless, and a task extracted from it is worse than useless, because it looks correct.

A general model cannot be expected to know a user's domain. It can, however, be told about it. This
document describes a first-class vocabulary domain in Threadbox: terms, grouped into sets, attached
to projects, and used at three separate points in the pipeline.

This feature is worth building early for a reason unrelated to meetings: it improves the voice note
transcription that Threadbox already ships in `src-tauri/src/speech.rs`. It is the one part of the
meeting agent that makes the current product better on its own, so it does not have to wait for the
rest.

## Where vocabulary is used

Three uses, with different constraints. Implementing only the first is a common mistake, because the
first is the one with a hard size limit.

### 1. Bias during recognition

`whisper-rs` exposes Whisper's initial prompt through `FullParams::set_initial_prompt`, verified in
version 0.14.4 at `src/whisper_params.rs:798`. Terms placed there make the model measurably more
likely to produce them. A lower-level `set_tokens` also exists at `src/whisper_params.rs:260` for
passing prompt tokens directly, which is the route to take if token budgeting has to be exact.

The constraint that shapes the whole design: **the prompt is capped at half of the model's text
context, which is 224 tokens in practice.** A domain vocabulary of several hundred terms does not
fit. This is precisely why the feature must be sets of words rather than one flat list. The set
relevant to the meeting at hand is what gets sent, not everything the user has ever entered.

Two risks to guard and measure rather than assume away:

- A prompt that is long, repetitive or stylistically odd can make Whisper echo it into the output or
  fall into repetition. The prompt builder needs an upper bound and the output needs a check for
  echoed prompt content.
- Biasing toward a term raises false positives for it. A vocabulary containing a common word as a
  domain term will start finding that term where it was not said.

### 2. Correction after recognition

Matching transcript words against the full vocabulary, with tolerance for the ways recognition fails:
near-miss spellings, split or joined words, and phonetically similar substitutions. This pass has no
size limit, so it covers the terms that did not fit in the prompt, and it can use the record of how a
term has been misrecognised in the past.

Two rules for this pass. The original recognised text is never overwritten in storage; corrections
are recorded alongside it with the term that was applied, so a wrong correction stays visible and
reversible. And a correction is only applied above a confidence threshold, because silently turning a
correctly recognised ordinary word into a domain term is a worse failure than leaving the error.

### 3. Context for the analysis model

The definitions attached to terms are given to the language model that writes the meeting notes.
This serves two purposes at once. The notes come out using the user's own terminology, and the model
can explain a term the user does not know, which is directly the need behind the topic explanation
feature in `meeting-agent-plan.md`. A glossary built for accuracy turns out to be the same asset
needed for comprehension.

## Data model

A term carries: a canonical written form; an optional expansion, for acronyms; an optional short
definition, used for explanation and for model context; a language tag, since a Polish-language
meeting can contain English product names and the two behave differently; observed variants, being
the misrecognitions seen for this term; and a priority, used to decide what fits in the prompt
budget.

Terms belong to named sets. A set is the unit a user thinks in: one per client, one per system, one
per medical speciality. Sets attach to projects, and a set may attach to several projects, so shared
industry vocabulary is entered once.

For any single recording, the effective vocabulary is the union of the sets attached to its project
and an always-active set for terms that apply everywhere, such as the names of the user's own
colleagues. Priority ordering across that union decides what reaches the recognition prompt.

Every record follows the same rules as the rest of the schema, because vocabulary has to
synchronise: a stable identifier, `updated_at`, and a tombstone rather than a hard delete. See
`privacy-and-sync-model.md` for why these are required before any multi-device level.

## Filling the vocabulary without data entry

Nobody will type three hundred terms into a form. Three routes, in order of how much they are worth:

1. **Harvest candidates from transcripts.** Terms that recur across meetings and do not appear in
   ordinary language are candidates. Present them for one-tap acceptance, grouped by the project the
   meetings belonged to.
2. **Learn from corrections.** When a user fixes a word in a transcript, record the pair as an
   observed variant of the term. This is the highest quality signal available and it costs the user
   nothing beyond the correction they wanted to make anyway.
3. **Import.** A plain text or CSV list, so a team can share a starting vocabulary.

Nothing is added to the vocabulary automatically. A dictionary that grows without review will start
biasing recognition toward its own earlier mistakes, which is a failure mode that gets worse over
time and is hard to notice.

## Privacy notes

A vocabulary is not neutral metadata. A set of client names, internal project codenames and
colleagues' surnames describes the user's employer and their work. It therefore belongs inside the
encrypted archive and inside the export, and it must not be sent to an external language model unless
the external model setting in `privacy-and-sync-model.md` permits it for that meeting.

Worth stating plainly, because it is a genuine advantage: the recognition prompt and the correction
pass are entirely local. Vocabulary makes transcription better at the device-only privacy level, with
no network involved at all.

## Open questions

- Whether to support explicit pronunciation hints, or rely only on observed variants. Hints are more
  powerful and considerably more work to enter.
- How to handle a term that is spelled identically in two languages but pronounced differently.
- Whether set membership should be inferred from the meeting's participants rather than only from its
  project, since the same project can have internal and client-facing meetings with different
  vocabulary.
