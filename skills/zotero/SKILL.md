---
name: zotero
description: Search, read, and manage Zotero through the local zotero-cli integration. Use from the librarian agent for papers, books, metadata, full text, annotations, notes, tags, collections, citations, and library updates.
---

# Zotero CLI

Run Zotero through the packaged CLI. Keep `ZOTERO_LOCAL=true` for fast local reads; existing API credentials enable hybrid writes.

```bash
nix shell ~/nix-config#zotero-mcp -c env ZOTERO_LOCAL=true zotero-cli <command>
```

Zotero desktop must be running with its local API enabled. Never print or inspect Zotero credentials.

## Search And Read

```bash
nix shell ~/nix-config#zotero-mcp -c env ZOTERO_LOCAL=true zotero-cli search "query" --limit 10
nix shell ~/nix-config#zotero-mcp -c env ZOTERO_LOCAL=true zotero-cli get metadata <item-key>
nix shell ~/nix-config#zotero-mcp -c env ZOTERO_LOCAL=true zotero-cli get metadata <item-key> --format bibtex
nix shell ~/nix-config#zotero-mcp -c env ZOTERO_LOCAL=true zotero-cli get fulltext <item-key>
nix shell ~/nix-config#zotero-mcp -c env ZOTERO_LOCAL=true zotero-cli get children <item-key>
nix shell ~/nix-config#zotero-mcp -c env ZOTERO_LOCAL=true zotero-cli ann list <item-key>
nix shell ~/nix-config#zotero-mcp -c env ZOTERO_LOCAL=true zotero-cli ann search "query"
nix shell ~/nix-config#zotero-mcp -c env ZOTERO_LOCAL=true zotero-cli notes list <item-key>
nix shell ~/nix-config#zotero-mcp -c env ZOTERO_LOCAL=true zotero-cli coll list
nix shell ~/nix-config#zotero-mcp -c env ZOTERO_LOCAL=true zotero-cli tags list
nix shell ~/nix-config#zotero-mcp -c env ZOTERO_LOCAL=true zotero-cli library info
```

Prefer metadata or annotation searches before loading full text. Capture the Zotero item key, item version when available, and page or annotation locator in librarian-store evidence.

## Manage

Writes require Zotero web API credentials in the inherited environment:

```bash
nix shell ~/nix-config#zotero-mcp -c env ZOTERO_LOCAL=true zotero-cli notes create --item-key <key> --text "note"
nix shell ~/nix-config#zotero-mcp -c env ZOTERO_LOCAL=true zotero-cli edit <key> --add-tags "reviewed"
nix shell ~/nix-config#zotero-mcp -c env ZOTERO_LOCAL=true zotero-cli add doi <doi> --collections "Reading List"
nix shell ~/nix-config#zotero-mcp -c env ZOTERO_LOCAL=true zotero-cli add file --filepath /path/to/paper.pdf --title "Optional title" --collections "Reading List"
nix shell ~/nix-config#zotero-mcp -c env ZOTERO_LOCAL=true zotero-cli collections manage --item-keys <key> --add-to "Collection"
```

`add file` imports a local PDF or EPUB and attempts to extract identifiers and
metadata. `add doi` can retrieve an open-access PDF through Zotero CLI's provider
cascade. Use idempotent add behavior and existing collections where possible.
If a write fails because credentials are absent, report that limitation rather
than requesting secrets in chat.
