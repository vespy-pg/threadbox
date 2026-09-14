# Backup and recovery

Threadbox backup version 5 is a portable ZIP archive containing a consistent SQLite snapshot,
stored media and a human-readable manifest. It covers organisations, projects, people, threads,
meetings, transcripts, analysis, documents, vocabulary, imported provider objects, external action
history and privacy receipts.

Operating system credentials are deliberately excluded. OAuth tokens, API keys, IMAP passwords and
SMTP passwords remain in the source computer's credential store. Restoring on another computer
preserves project history but every external account must be connected again.

## Restore safety

Restore is an explicit replacement operation. Before changing live data, Threadbox:

1. rejects legacy task-only archives and unsafe ZIP paths;
2. extracts into a unique staging directory beside the live database;
3. runs SQLite integrity and foreign-key checks;
4. rejects a schema created by a newer unsupported Threadbox version;
5. migrates an older supported snapshot in staging;
6. checkpoints both databases and removes stale WAL sidecars;
7. renames the live database and media to rollback paths;
8. swaps in the validated snapshot and restores the originals if either rename fails.

Rollback files are deleted only after the database and media swaps both succeed. The interface then
reloads the workspace from the restored database.

Version 4 and older archives still contain portable task JSON and media, but they are not complete
workspace snapshots and cannot replace a current workspace. Keep them for manual task recovery.
