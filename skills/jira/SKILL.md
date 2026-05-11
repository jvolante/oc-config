---
description: query, update, and create Jira tickets via the REST API using curl and jq. Load this skill when the user references a Jira ticket key (e.g. TAG-123, ISL-456), asks about tickets, issues, sprints, or project status, or requests any Jira-related action.
name: jira
---

# /jira

Interact with Jira using the REST API directly via `curl` and `jq`. All operations
read credentials from environment variables — never hardcode them.

## Environment Variables

| Variable | Description |
|---|---|
| `$JIRA_PROJECTS` | Optional. Comma-separated project keys, e.g. `ASDF,SFTW`. When set, `jira-query` automatically appends `AND project in (...)` to every JQL query. |

## Helper Functions (preferred)

```bash
# Search across all projects — accepts any JQL
jira-query '<jql>'
jira-query '<jql>' <max_results>   # default 1000

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

All helpers emit JSON, so every output can be piped directly into `jq`.

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

Always filter curl output with `jq` to avoid overwhelming context with raw API responses.
The Jira REST API returns large JSON objects; pipe aggressively.

```bash
# Keys and statuses from a search
jira-query 'assignee = currentUser()' \
  | jq '{key, summary, status}'

# Just the keys as plain text (for xargs)
jira-query 'assignee = currentUser() AND status != Done' \
  | jq -r '.key'

# Comments — skip issues with no comments
jira-comments TAG-469 \
  | jq 'select(.comments | length > 0)'

# Transition IDs and names only
jira-transitions TAG-469 \
  | jq '.transitions[] | {id, name}'

# Create result — just the key and URL
jira-create TAG Task "My ticket" \
  | jq '{key, url}'

# Raw search with custom fields via jira-curl
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
  | jq -r '.key' \
  | xargs -I{} bash -c 'jira-comments {}'

# Show only tickets that have comments
jira-query 'assignee = currentUser()' \
  | jq -r '.key' \
  | xargs -I{} bash -c 'jira-comments {}' \
  | jq 'select(.comments | length > 0)'

# Transition all In Progress tickets to Done
jira-query 'assignee = currentUser() AND status = "In Progress"' \
  | jq -r '.key' \
  | xargs -I{} bash -c 'jira-transition {} done'

# Post a comment on every Committed epic
jira-query 'assignee = currentUser() AND status = Committed AND issuetype = Epic' \
  | jq -r '.key' \
  | xargs -I{} bash -c 'jira-comment {} "Still on track."'
```

## Changing Ticket Status

Status changes require a transition ID, not a status name. Always fetch valid
transitions first — available transitions depend on the issue's current status.

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
# Using the helper
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

## Summarizing Action Items from Comments

When asked to summarize action items from comments across multiple tickets:

1. Fetch all open tickets with `jira-query`
2. Pipe keys to `jira-comments` via `xargs`
3. Filter to only tickets with comments using `jq 'select(.comments | length > 0)'`
4. Read each comment body and identify: requests, blockers, open questions, or
   explicit asks directed at the assignee
5. Group by ticket key and present as a concise bullet list

```bash
jira-query 'assignee = currentUser() AND status != Done' \
  | jq -r '.key' \
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
