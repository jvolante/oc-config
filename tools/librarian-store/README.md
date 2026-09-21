# librarian-store

```sh
nix run . -- init
nix run . -- source upsert --kind web --external-id docs-1 --title Docs --version 1
nix run . -- claim add --text 'The API is stable' --topic api --temporal-kind current
nix run . -- search API
nix run . -- context API
nix run . -- inbox add ./incoming/report.pdf
nix run . -- inbox add --sensitivity restricted ./incoming/private.dat
nix run . -- inbox status
nix run . -- inbox process [--limit N] [JOB_ID ...]
nix run . -- document list [--status filed|quarantined] [--limit N]
nix run . -- document show DOCUMENT_ID [--limit N]
nix run . -- document search QUERY [--limit N]
```

The database is selected by `LIBRARIAN_DB`, then `$XDG_DATA_HOME/opencode/librarian/knowledge.db`, or `~/.local/share/opencode/librarian/knowledge.db`.

The store separates source revisions, cited evidence, reusable claims, topics,
relationships, and review reasons. FTS5 searches all claim text, topics, source
titles, and evidence excerpts. Updating a source revision marks only dependent
claims for review; adding current supporting evidence revalidates that source
relationship.

Numeric revisions advance in order. Unordered tags or hashes require the
explicit `source upsert --make-current` assertion before replacing a current
revision.

Script reviews are explicit. `review_after` accepts at most 512 Unicode
characters. A script must exit 0 (trigger), 1 (false), or >=2 (unknown).

```sh
nix run . -- claim review-approval <claim-id> --capability gh-auth
nix run . -- claim approve-review <claim-id> --capability gh-auth --hash <approval-hash>
```

Approval hashes the exact script plus sorted, deduplicated capabilities. Only
`network` and `gh-auth` are accepted; `gh-auth` implies network. Checks run only
through a user `systemd-run` transient service with a fail-closed sandbox. A
changed script or capability set invalidates approval. Condition reviews are
metadata-only and remain unknown.

All claim-bearing commands execute pending approved checks before returning and
include freshness, review outcome, active reasons, and citations in their JSON.
Fired checks latch until `claim review` retires them.

Exports and backups are confined to `exports/` and `backups/` beside the
database. Their `--output` argument is a single file name, not an arbitrary
filesystem path.

This MVP does not evaluate condition expressions and requires a systemd user manager for script execution. Search/context use the same ranked FTS retrieval.

## Raw-source inbox

`init` creates a private store layout adjacent to the database:
`inbox/`, `archive/objects/`, `archive/text/`, and `quarantine/`.
`inbox process` claims jobs with expiring ownership leases, extracts text in a
systemd sandbox, and files successful jobs in the content-addressed archive.
Failed extraction, OCR-only PDFs, and oversized lines are retained in
`quarantine/`; OCR is reported in the quarantined job detail because this MVP
has no OCR engine. `document show` is bounded by its validated limit and caps
each chunk.

`inbox add` accepts regular, non-symlink, non-empty files and copies them into
the inbox without changing the originals. It hashes each file during the copy,
rejects files larger than 512 MiB, and publishes an atomically renamed job file
only after the copy is synced. Input names are sanitized and never become
paths outside the inbox. Sensitivity defaults to `internal` and may be
`public`, `internal`, or `restricted`.

Content already represented by a filed document or a non-failed inbox job is
reported as `duplicate`; filed duplicates return the existing document ID and
do not create a job. Hidden temporary files are not jobs and are ignored by
`inbox status`. Schema mismatches fail closed without migration or reset.
