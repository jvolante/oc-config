---
name: librarian-store
description: Use the librarian-store CLI for durable knowledge and inbox operations.
---

## Inbox preflight

Before research, check the inbox with `nix run ~/.config/opencode/tools/librarian-store -- inbox status`. If pending jobs exist, have the librarian dispatch `source-filer` and wait for its compact filing result before researching.
