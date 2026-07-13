---
description: query, update, and create Jira tickets via the REST API using curl and jq. Load this skill when the user references a Jira ticket key (e.g. TAG-123, ISL-456), asks about tickets, issues, sprints, or project status, or requests any Jira-related action.
name: jira
---

# /jira

Interact with Jira via the REST API using `jira-curl` and `jq`. `jira-curl` takes care of authentication for you. NEVER use raw curl.

## Environment Variables

| Variable | Description |
|---|---|
| `$JIRA_PROJECTS` | Optional. Comma-separated project keys, e.g. `ASDF,SFTW`. When set, `jira-query` automatically appends `AND project in (...)` to every JQL query. |

## Helper Functions (preferred)

```bash
# Search across all projects — accepts any JQL
jira-query '<jql>'
jira-query '<jql>' <max_results>   # default 1000

# Your own open tickets (non-Done), optionally scoped to a project
jira-mine
jira-mine ISL

# Full detail on one issue
jira-issue <KEY>

# Comments on one issue
jira-comments <KEY>

# List valid status transitions for an issue
jira-transitions <KEY>

# Change status — fuzzy matches transition name (case-insensitive)
jira-transition <KEY> "<transition name>"

# Post a comment
jira-comment <KEY> "<text>"

# Create a new issue
jira-create <PROJECT> <TYPE> "<summary>" ["<description>"]
```

## Output format — IMPORTANT

`jira-query`, `jira-mine`, `jira-epic-issues`, and `jira-sprint-issues` all return
a **flat JSON array** of objects. Always pipe with `.[]` — never `.issues[]`:

```bash
jira-query '...' | jq -r '.[] | .key'          # correct
jira-mine        | jq -r '.[] | .key'          # correct
jira-query '...' | jq -r '.issues[] | .key'    # WRONG — .issues does not exist
```

Each object has: `key`, `type`, `summary`, `status`, `priority`, `assignee`.

`jira-issue`, `jira-comments`, `jira-transitions` return a single object — no array wrapper.

## Raw API Access via `jira-curl`

For endpoints or fields the named helpers don't cover, use `jira-curl`. It handles
auth and prepends `$JIRA_URL/rest/api/2/` — pass only the endpoint path.

```bash
# GET
jira-curl <endpoint> | jq '<filter>'

# POST
jira-curl -X POST -d '<json>' <endpoint> | jq '<filter>'

# GET with query params
jira-curl <endpoint> -G --data-urlencode 'param=value' | jq '<filter>'
```

Examples:

```bash
# Get raw issue fields
jira-curl issue/TAG-469 | jq '.fields | {summary, status: .status.name}'

# Search with custom fields
jira-curl search \
  -G \
  --data-urlencode 'jql=assignee = currentUser()' \
  --data-urlencode 'fields=summary,status,comment' \
  | jq '.issues[] | {key, status: .fields.status.name, comments: .fields.comment.total}'

# Post a transition by ID
jira-curl -X POST -d '{"transition":{"id":"71"}}' issue/TAG-469/transitions

# Update an issue field
jira-curl -X PUT \
  -d '{"fields":{"priority":{"name":"High"}}}' \
  issue/TAG-469
```

## Key API Endpoints

| Purpose | Method | Endpoint |
|---|---|---|
| Search issues (JQL) | GET | `/rest/api/2/search?jql=<jql>&maxResults=<n>&fields=<fields>` |
| Get issue | GET | `/rest/api/2/issue/<KEY>` |
| Get comments | GET | `/rest/api/2/issue/<KEY>/comment` |
| Get transitions | GET | `/rest/api/2/issue/<KEY>/transitions` |
| Apply transition | POST | `/rest/api/2/issue/<KEY>/transitions` |
| Add comment | POST | `/rest/api/2/issue/<KEY>/comment` |
| Create issue | POST | `/rest/api/2/issue` |
| Update issue | PUT | `/rest/api/2/issue/<KEY>` |

## Useful JQL Patterns

```
# Your open tickets across all projects
assignee = currentUser() AND status != Done

# In Progress only
assignee = currentUser() AND status = "In Progress"

# Specific project
project = TAG AND assignee = currentUser()

# Multiple projects
project in (TAG, ISL) AND assignee = currentUser()

# Updated recently
assignee = currentUser() AND updated >= -7d

# Reported by you
reporter = currentUser() AND status != Done
```

**Important:** Do not use `ORDER BY` in JQL — this Jira instance rejects it with a 400.

## jq Filters — Extract Only What You Need

```bash
# Keys and statuses from jira-query / jira-mine (flat array — use .[])
jira-query 'assignee = currentUser()' \
  | jq '.[] | {key, summary, status}'

# Just the keys as plain text (for xargs)
jira-query 'assignee = currentUser() AND status != Done' \
  | jq -r '.[] | .key'

# jira-mine with project filter
jira-mine ISL | jq -r '.[] | "\(.key)\t\(.summary)"'

# Comments — skip issues with no comments
jira-comments TAG-469 \
  | jq 'select(.comments | length > 0)'

# Transition IDs and names only
jira-transitions TAG-469 \
  | jq '.transitions[] | {id, name}'

# Create result — just the key and URL
jira-create TAG Task "My ticket" \
  | jq '{key, url}'

# Raw search with custom fields via jira-curl (note: raw curl uses .issues[])
jira-curl search \
  -G \
  --data-urlencode 'jql=assignee = currentUser()' \
  --data-urlencode 'fields=summary,status,comment' \
  | jq '.issues[] | {key, status: .fields.status.name, comment_count: (.fields.comment.total // 0)}'
```

## Chaining Examples

```bash
# Print comments on every open ticket
jira-query 'assignee = currentUser() AND status != Done' \
  | jq -r '.[] | .key' \
  | xargs -I{} bash -c 'jira-comments {}'

# Show only tickets that have comments
jira-query 'assignee = currentUser()' \
  | jq -r '.[] | .key' \
  | xargs -I{} bash -c 'jira-comments {}' \
  | jq 'select(.comments | length > 0)'

# Transition all In Progress tickets to Done
jira-query 'assignee = currentUser() AND status = "In Progress"' \
  | jq -r '.[] | .key' \
  | xargs -I{} bash -c 'jira-transition {} done'

# Post a comment on every Committed epic
jira-query 'assignee = currentUser() AND status = Committed AND issuetype = Epic' \
  | jq -r '.[] | .key' \
  | xargs -I{} bash -c 'jira-comment {} "Still on track."'
```

## Changing Ticket Status

Fetch valid transitions first — available transitions depend on the issue's
current status.

```bash
# Step 1 — see what transitions are available
jira-transitions TAG-469 | jq '.transitions[] | {id, name}'

# Step 2 — apply by name (jira-transition does the ID lookup for you)
jira-transition TAG-469 "done"

# Or apply by ID directly via jira-curl
jira-curl -X POST -d '{"transition": {"id": "71"}}' issue/TAG-469/transitions

# Verify the change took effect
jira-issue TAG-469 | jq '{key, status}'
```

## Creating Issues

```bash
# Using the helper (simple cases)
jira-create TAG Task "My new task" "Optional description"

# Via jira-curl (when you need fields jira-create doesn't expose)
jira-curl -X POST \
  -d "$(jq -n \
    --arg proj "TAG" \
    --arg type "Bug" \
    --arg summary "Something is broken" \
    --arg desc "Steps to reproduce..." \
    '{fields: {project: {key: $proj}, issuetype: {name: $type}, summary: $summary, description: $desc}}')" \
  issue \
  | jq '{key, url: ("'"$JIRA_URL"'/browse/" + .key)}'
```

## Issue Hierarchy: Epics, Tasks, Sub-tasks

Jira uses two distinct parent-child mechanisms — use the right one:

| Relationship | Field to set | Value |
|---|---|---|
| Task/Story belongs to an Epic | `customfield_10100` (Epic Link) | Epic key string, e.g. `"ISL-1181"` |
| Sub-task belongs to a Task | `parent` | `{key: "ISL-1459"}` object |

```bash
# Create a Task linked to an epic
jira-curl -X POST -d "$(jq -n \
  --arg proj "ISL" --arg summary "My task" --arg epic "ISL-100" \
  '{fields: {project: {key: $proj}, issuetype: {id: "10003"},
    summary: $summary, customfield_10100: $epic}}')" issue \
  | jq '{key}'

# Create a Sub-task under a Task (use issuetype id "10004", set parent key)
jira-curl -X POST -d "$(jq -n \
  --arg proj "ISL" --arg parent "ISL-200" --arg summary "My subtask" \
  '{fields: {project: {key: $proj}, issuetype: {id: "10004"},
    parent: {key: $parent}, summary: $summary}}')" issue \
  | jq '{key}'
```

**Required custom fields:** Some projects require extra fields on Tasks (e.g. "Code Tier")
that are NOT required on Sub-tasks. If a Task creation returns
`{"errors":{"customfield_XXXXX":"Field X is required"}}`, probe an existing issue
from that project to find the value to use:

```bash
# Find the required field value from an existing issue
jira-curl issue/ISL-1181 | jq '.fields.customfield_52408'
# Then add it to your creation payload:
# customfield_52408: {id: "84852"}
```

Sub-task creation typically does not require project-specific custom fields.

## Summarizing Action Items from Comments

Fetch open tickets, filter to those with comments, then read each body for
requests, blockers, open questions, or explicit asks directed at the assignee.
Group by ticket key and present as a concise bullet list.

```bash
jira-query 'assignee = currentUser() AND status != Done' \
  | jq -r '.[] | .key' \
  | xargs -I{} bash -c 'jira-comments {}' \
  | jq 'select(.comments | length > 0)'
```

## Common Mistakes to Avoid

- **Never use `ORDER BY` in JQL** — this instance returns 400, use `jq` for sorting
- **Never omit `jq` filtering** — raw API responses are thousands of lines
- **Don't guess transition IDs** — always call `jira-transitions <KEY>` first
- **Don't use `-p` flag with `jira` CLI** — use `jira-query` with JQL instead;
  the CLI scopes to a single project and lacks cross-project support
- **Always verify status changes** — follow a `jira-transition` call with
  `jira-issue <KEY> | jq '{key, status}'` to confirm
- **Don't use raw `curl` directly** — use `jira-curl` so auth and the base URL
  are handled consistently
- **jira-query / jira-mine return `.[]` not `.issues[]`** — the helpers normalise
  the Jira envelope; only raw `jira-curl search` output needs `.issues[]`
