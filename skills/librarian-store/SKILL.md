---
name: librarian-store
description: Use the librarian-store CLI for durable knowledge and inbox operations.
---

## Inbox preflight

Before every research task, run `nix run ~/.config/opencode/tools/librarian-store -- inbox status`. If pending sources exist, the librarian dispatches `source-filer` and waits for its compact result before continuing. If jobs are processing, report them and allow stale leases to recover rather than duplicating work.

## Source-filer workflow

The source-filer runs:

```bash
nix run ~/.config/opencode/tools/librarian-store -- inbox process
nix run ~/.config/opencode/tools/librarian-store -- document list --status filed --classification-status unclassified
nix run ~/.config/opencode/tools/librarian-store -- document show <document-id> --limit <bounded-limit>
nix run ~/.config/opencode/tools/librarian-store -- document classify <document-id> \
  --title "<title derived from the bounded document show metadata>" \
  --doc-type <type> --authority <authority> --topic <topic> --topic <topic> --scholarly
```

Classify every newly filed unclassified document. Derive a non-empty title from the bounded `document show` metadata and always pass it to `document classify`; do not use an unbounded source body to choose the title. This stage never creates claims. Return counts and document IDs/statuses only; note failures and retries without returning source bodies. Do not inspect hidden inbox files, use raw SQLite, access project files, or use the web.

## Document and Zotero outcomes

Only a public document confidently identified as a scholarly PDF or paper may be imported. Use the content-addressed archive object path with idempotent `zotero-cli add file`, target `Librarian Inbox`, create the collection if missing, and record both the item and attachment keys:

```bash
nix run ~/.config/opencode/tools/librarian-store -- document zotero-link <document-id> \
  --item-key <item-key> --attachment-key <attachment-key>
```

On an import failure, record `document zotero-failed` and do not retry in a tight loop. For internal, restricted, non-PDF, non-paper, or uncertain documents, record `document zotero-skip`; never send those files to Zotero.
