# Project AI rules

1. The primary product requirement is to make work easier, faster, and more obvious. Every interface and workflow must minimize cognitive load, clearly show the current organization and project context, and prefer direct, discoverable actions over hidden controls or configuration-heavy flows.
2. Treat Threadbox as an extensible work assistant, not as a task-list application. Organizations and projects form the navigation context; threads, meetings, people, documents, integrations, and future work modules live within that structure.
3. Design the application shell and navigation to accommodate additional project and organization modules without turning them into a growing collection of dialogs.
4. Keep all generated project artifacts in English.
5. Protect the user's workstation during verification. Use `scripts/check.sh` for the full Docker
   check; its default is one CPU, 2 GB of memory, one Cargo job, and reduced process priority. Run
   lightweight frontend checks during iteration and reserve the full check for meaningful milestones.
6. Read and follow the relevant shared machine rules in `~/.ai/rules/`; they remain the source of
   truth and must not be duplicated here.
