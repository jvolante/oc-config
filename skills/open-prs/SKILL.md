---
description: Load this skill when the user wants to query, filter, or reason about their open pull requests, correlate PRs with CI status, or find CircleCI jobs linked to a PR.
name: open-prs
---
## Invocation

```bash
open-prs --json | jq '<filter>'
```

Returns a JSON array of open PRs authored by the current user, across all
authenticated `gh` hosts.

## Output Schema

Top-level: **array of PR objects**.

### PR Object

| Field | Type | Description |
|---|---|---|
| `number` | integer | PR number |
| `title` | string | PR title |
| `url` | string | HTML URL of the PR |
| `headRefName` | string | Source branch name |
| `baseRefName` | string | Target branch name |
| `repository.nameWithOwner` | string | `"<org>/<repo>"` on the GHE host |
| `commits.nodes[0].commit.statusCheckRollup.state` | string\|null | Overall CI rollup: `SUCCESS`, `FAILURE`, `PENDING`, `ERROR`, or `null` if no checks |
| `commits.nodes[0].commit.statusCheckRollup.contexts.nodes` | array\|null | Per-job check nodes (see below) |
| `reviews.nodes` | array | Reviews with `CHANGES_REQUESTED` state (may contain multiple entries per reviewer across rounds) |
| `reviews.nodes[].author.login` | string | GitHub login of the reviewer who requested changes |
| `reviews.nodes[].state` | string | Always `CHANGES_REQUESTED` for this field |

> **Note:** `reviews.nodes` only surfaces `CHANGES_REQUESTED` reviews. Top-level review body comments in `COMMENTED` or `APPROVED` state (which also carry actionable feedback) are a separate resource — fetch them with:
> ```bash
> gh api repos/<org>/<repo>/pulls/<number>/reviews --hostname <hostname> \
>   | jq '[.[] | select(.body != "" and .body != null) | {id, state, login: .user.login, body}]'
> ```
> These do not appear in `reviewThreads` and cannot be resolved via the `resolveReviewThread` GraphQL mutation.

### Check Node (StatusContext variant)

Emitted for classic commit status checks (e.g. CircleCI enterprise status hooks).

| Field | Type | Description |
|---|---|---|
| `context` | string | Job name, e.g. `"ci/circleci_enterprise: lint"` |
| `state` | string | `SUCCESS`, `FAILURE`, `PENDING`, or `ERROR` |
| `circleci_slug` | string\|null | `"github/<org>/<repo>"` if the check URL points to CCI, otherwise `null` |

### Check Node (CheckRun variant)

Emitted for GitHub Actions and other GitHub Checks API integrations.

| Field | Type | Description |
|---|---|---|
| `name` | string | Job name |
| `status` | string | `QUEUED`, `IN_PROGRESS`, or `COMPLETED` |
| `conclusion` | string\|null | `SUCCESS`, `FAILURE`, `TIMED_OUT`, `CANCELLED`, `NEUTRAL`, `SKIPPED`, or `null` if still running |
| `circleci_slug` | string\|null | `"github/<org>/<repo>"` if the details URL points to CCI, otherwise `null` |

## Common jq Recipes

```bash
# PRs with any failing CI
open-prs --json | jq '[.[] | select(.commits.nodes[0].commit.statusCheckRollup.state == "FAILURE")]'

# PRs where someone requested changes (with reviewer logins)
open-prs --json | jq '
  [.[] | select((.reviews.nodes // []) | length > 0) |
   { number, title, url,
     changes_requested_by: ([.reviews.nodes[].author.login] | unique) }]
'

# Failing check names + slug for every PR
open-prs --json | jq '
  .[] |
  { pr: .number, repo: .repository.nameWithOwner,
    failing: [
      .commits.nodes[0].commit.statusCheckRollup.contexts.nodes // [] | .[] |
      select((.state? == "FAILURE" or .state? == "ERROR")
          or (.conclusion? == "FAILURE" or .conclusion? == "TIMED_OUT"))
      | { job: (.context // .name), circleci_slug }
    ]
  } | select(.failing | length > 0)
'

# Unique CircleCI slugs across all failing PRs
open-prs --json | jq -r '
  [.[] | .commits.nodes[0].commit.statusCheckRollup.contexts.nodes // [] | .[] |
   select((.state? == "FAILURE") or (.conclusion? == "FAILURE")) |
   .circleci_slug] | unique[] | select(. != null)
'

# PR numbers and head branches for a specific repo
open-prs --json | jq -r '.[] | select(.repository.nameWithOwner == "imaging/copi-algo") | "#\(.number) \(.headRefName)"'
```

## Chaining with CircleCI

`circleci_slug` is directly usable with `cci-builds` and `cci-workflow`:

```bash
# Get recent builds for every repo with a failing PR
open-prs --json | jq -r '
  [.[] | .commits.nodes[0].commit.statusCheckRollup.contexts.nodes // [] | .[] |
   select(.state? == "FAILURE") | .circleci_slug] | unique[] | select(. != null)
' | xargs -I{} bash -c 'source cci-builds {} 1 5'

# Rerun failed workflows for a specific PR's failing CCI jobs
open-prs --json | jq -r '
  .[] | select(.number == 123) |
  .commits.nodes[0].commit.statusCheckRollup.contexts.nodes // [] | .[] |
  select(.state? == "FAILURE") | .circleci_slug
' | sort -u | xargs -I{} bash -c 'source cci-builds {} 1 1 | jq -r .workflow_id | xargs cci-rerun'
```
