---
description: search, read, and create Confluence pages and comments via the REST API. Load this skill when the user references a Confluence page, space key (e.g. MRDE, ALIG), asks about documentation, wiki pages, or requests any Confluence-related action.
name: confluence
---

# /confluence

Interact with Confluence using the REST API via `confluence-curl` and `jq`. All
operations read credentials from environment variables — never hardcode them.

## Environment Variables

| Variable | Description |
|---|---|
| `$CONFLUENCE_SPACES` | Optional. Comma-separated space keys, e.g. `MRDE,ALIG`. When set, `confluence-search` automatically appends `AND space in (...)` to every CQL query. |

## Helper Functions (preferred)

Source `~/jvscripts/mybashrc.sh` before using these in scripts.

```bash
# Search pages/content using CQL
confluence-search '<cql>'
confluence-search '<cql>' <max_results>   # default 1000

# Get full page content including body HTML
confluence-page <page-id>

# List comments on a page
confluence-comments <page-id>

# Post a comment on a page
confluence-comment <page-id> "<text>"

# Create a new page
confluence-create <SPACE> "<title>" "<body>" [parent-page-id]

# Update an existing page (fetches current version automatically)
confluence-update <page-id> "<title>" "<body>"
```

All helpers emit JSON, so every output can be piped directly into `jq`.

## Raw API Access via `confluence-curl`

For endpoints the named helpers don't cover, use `confluence-curl`. It handles
auth and prepends `$CONFLUENCE_URL/rest/api/` — pass only the endpoint path.
Flags can appear before or after the endpoint.

```bash
# GET
confluence-curl <endpoint> | jq '<filter>'

# GET with query params
confluence-curl <endpoint> -G --data-urlencode 'param=value' | jq '<filter>'

# POST
confluence-curl -X POST -d '<json>' <endpoint> | jq '<filter>'
```

Examples:

```bash
# Get a page with specific expansions
confluence-curl "content/12345?expand=body.storage,version,ancestors" \
  | jq '{title, body: .body.storage.value}'

# Search with custom CQL
confluence-curl content/search \
  -G \
  --data-urlencode 'cql=type=page AND space=MRDE AND text~"wire detection"' \
  --data-urlencode 'limit=10' \
  --data-urlencode 'expand=space,ancestors' \
  | jq '.results[] | {id, title, space: .space.key}'

# Get child pages
confluence-curl "content/12345/child/page" \
  | jq '.results[] | {id, title}'

# Update a page (requires current version number)
confluence-curl -X PUT \
  -d '{"version":{"number":2},"title":"Updated Title","type":"page","body":{"storage":{"value":"<p>new content</p>","representation":"storage"}}}' \
  "content/12345" \
  | jq '{id, title, version: .version.number}'
```

## CQL (Confluence Query Language) Patterns

CQL is Confluence's search syntax, analogous to Jira's JQL.

```
# Pages you've contributed to
type=page AND contributor=currentUser()

# Pages in a specific space
type=page AND space=MRDE

# Full-text search
type=page AND text~"wire detection"

# Pages you created
type=page AND creator=currentUser()

# Pages updated recently
type=page AND lastModified >= "2026-01-01"

# Combine filters
type=page AND space=MRDE AND text~"pilotage" AND contributor=currentUser()
```

## jq Filters — Extract Only What You Need

Page bodies are verbose HTML/storage format. Always filter aggressively.

```bash
# Title, space, and URL only
confluence-search 'type=page AND contributor=currentUser()' \
  | jq '{title, space, url}'

# Just page IDs (for xargs)
confluence-search 'type=page AND space=MRDE' \
  | jq -r '.id'

# Page body as plain text (strips HTML tags)
confluence-page 12345 \
  | jq -r '.body | gsub("<[^>]+>"; "")'

# Comments with author and date only
confluence-comments 12345 \
  | jq '.comments[] | {author, created}'

# Pages with their ancestor breadcrumb
confluence-search 'type=page AND space=MRDE' \
  | jq '{title, path: (.ancestors | join(" > "))}'
```

## Chaining Examples

```bash
# Read body of every page you've contributed to in a space
confluence-search 'type=page AND space=MRDE AND contributor=currentUser()' \
  | jq -r '.id' \
  | xargs -I{} bash -c 'confluence-page {}' \
  | jq '{title, body: (.body | gsub("<[^>]+>"; ""))}'

# Find pages mentioning a topic and show their full URL
confluence-search 'type=page AND text~"wire detection"' \
  | jq -r '"'"$CONFLUENCE_URL"'\(.url)"'

# Get comments on all pages in a space you contributed to
confluence-search 'type=page AND space=MRDE AND contributor=currentUser()' \
  | jq -r '.id' \
  | xargs -I{} bash -c 'confluence-comments {}' \
  | jq 'select(.comments | length > 0)'
```

## Page Body Format

Confluence pages use **storage format** (a subset of XHTML) for the body. When
reading pages the body will contain HTML tags. When creating or updating pages,
pass valid storage format HTML.

```bash
# Read — strip tags for plain text summary
confluence-page 12345 | jq -r '.body | gsub("<[^>]+>"; "")'

# Create — simple HTML is sufficient
confluence-create MRDE "My New Page" "<h1>Overview</h1><p>Content here.</p>"

# Create under a parent page
confluence-create MRDE "Sub Page" "<p>Content.</p>" 451848824

# Update — confluence-update fetches the current version automatically
confluence-update 12345 "My Page" "<h1>Updated</h1><p>New content.</p>"
```

The typical edit workflow:

```bash
# 1. Find the page
confluence-search 'type=page AND space=MRDE AND text~"pilotage"' | jq '{id, title}'

# 2. Read the current body
confluence-page 12345 | jq -r '.body'

# 3. Write the updated body, then apply
confluence-update 12345 "Page Title" "<p>Updated content.</p>"
```

## Common Mistakes to Avoid

- **Never omit `jq` filtering** — page bodies are large HTML blobs that will flood context
- **Page IDs are numeric strings** — get them from `confluence-search` first
- **Don't use raw `curl` directly** — use `confluence-curl` so auth and the base URL are handled consistently
- **Use `confluence-update` not raw PUT** — it fetches the current version automatically; raw PUT requires the exact current version number or you get a 409
- **`CONFLUENCE_URL` and `JIRA_URL` are different hosts** — don't mix them
