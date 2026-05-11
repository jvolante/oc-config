---
description: query, trigger, and rerun CircleCI pipelines and workflows via the REST API. Load this skill when the user asks about CI builds, pipelines, workflows, build failures, CircleCI, or wants to trigger or rerun a build.
name: circleci
---

# /circleci

Interact with CircleCI using the REST API via `cci-curl` and `jq`. All operations
read credentials from environment variables — never hardcode them.

## Project Slugs

All project-scoped commands take a slug in the format `github/<org>/<repo>`.

**Always run `cci-projects` first** to discover the correct slug for the repo you're working in — do not guess or hardcode slugs. The slug for the current git repo can also be resolved automatically with `cci-current`.

## Helper Functions (preferred)

```bash
# Current user info
cci-me

# List all followed projects
cci-projects

# Latest pipeline + workflow statuses for the current git branch (infers slug and branch automatically)
cci-current

# Recent builds across all projects
cci-recent [limit]                              # default 25

# Builds for a specific project
cci-builds <project-slug> [branch] [limit]

# Full workflow detail with all jobs
cci-workflow <workflow-id>

# Build log output for a job (falls back to presigned URLs on self-hosted CCI instances)
cci-log <project-slug> <build-num>

# Rerun a workflow from failed jobs
cci-rerun <workflow-id>

# Trigger a new pipeline
cci-trigger <project-slug> [branch]             # default branch: main
```

## Raw API Access via `cci-curl`

`cci-curl` handles auth and base url.
CircleCI has two API versions — use the appropriate one:

- **v2** — pipelines, workflows, jobs (preferred for new queries)
- **v1.1** — build logs, project builds, recent builds (richer build metadata)

```bash
# GET
cci-curl v2/<endpoint> | jq '<filter>'

# GET with query params
cci-curl v1.1/<endpoint> -G --data-urlencode 'param=value' | jq '<filter>'

# POST
cci-curl -X POST v2/<endpoint> -d '<json>' | jq '<filter>'
```

Examples:

```bash
# Get pipeline detail
cci-curl v2/pipeline/6d2abdb4-b725-4ff0-9ae3-b8af425c618d | jq '{id, number, state}'

# Get all workflows for a pipeline
cci-curl v2/pipeline/6d2abdb4-b725-4ff0-9ae3-b8af425c618d/workflow \
  | jq '.items[] | {id, name, status}'

# Cancel a running workflow
cci-curl -X POST v2/workflow/<id>/cancel | jq '.'

# Get artifacts for a build
cci-curl v1.1/project/github/my-org/my-repo/3001/artifacts \
  | jq '.[] | {path, url}'
```

## Key API Endpoints

| Purpose | Version | Endpoint |
|---|---|---|
| Current user | v2 | `v2/me` |
| Recent builds (all projects) | v1.1 | `v1.1/recent-builds?limit=N` |
| Project builds | v1.1 | `v1.1/project/<slug>?limit=N` |
| Branch builds | v1.1 | `v1.1/project/<slug>/tree/<branch>` |
| Build log | v1.1 | `v1.1/project/<slug>/<build-num>/output` |
| Build artifacts | v1.1 | `v1.1/project/<slug>/<build-num>/artifacts` |
| Trigger pipeline | v2 | POST `v2/project/<slug>/pipeline` |
| Workflow detail | v2 | `v2/workflow/<id>` |
| Workflow jobs | v2 | `v2/workflow/<id>/job` |
| Rerun from failed | v2 | POST `v2/workflow/<id>/rerun` |
| Cancel workflow | v2 | POST `v2/workflow/<id>/cancel` |
| Followed projects | v1.1 | `v1.1/projects` |

## jq Filters — Extract Only What You Need

```bash
# Failed builds only
cci-recent 50 | jq 'select(.status == "failed")'

# Unique workflow IDs from recent builds
cci-builds github/my-org/my-repo \
  | jq -r '.workflow_id' | sort -u

# Job names and statuses from a workflow
cci-workflow <id> | jq '.jobs[] | {name, status}'

# Only failed jobs
cci-workflow <id> | jq '.jobs[] | select(.status == "failed") | {name, job_number}'

# Build URL for quick navigation
cci-builds github/my-org/my-repo 2 \
  | jq -r '.url'
```

## Chaining Examples

```bash
# Get workflow details for all recent builds on a project
cci-builds github/my-org/my-repo \
  | jq -r '.workflow_id' | sort -u \
  | xargs -I{} bash -c 'cci-workflow {}' \
  | jq '{name, status, jobs: [.jobs[] | {name, status}]}'

# Rerun all failed workflows on a project
cci-builds github/my-org/my-repo \
  | jq 'select(.status == "failed") | .workflow_id' | sort -u \
  | xargs -I{} bash -c 'cci-rerun {}'

# Get logs for all failed jobs in a workflow
cci-workflow <workflow-id> \
  | jq -r '.jobs[] | select(.status == "failed") | .job_number | tostring' \
  | xargs -I{} bash -c 'cci-log github/my-org/my-repo {}'
```

## Common Mistakes to Avoid

- **Never omit `jq` filtering** — build log output is extremely verbose
- **Don't mix up v1.1 and v2** — logs and build metadata are v1.1 only; pipeline/workflow control is v2
- **Project slugs must be `github/<org>/<repo>`** — always discover with `cci-projects`, never hardcode
- **`cci-rerun` reruns from failed jobs** — use `cci-trigger` to start a fresh pipeline from scratch
- **Don't use raw `curl` directly** — use `cci-curl` so auth and the base URL are handled consistently
- **Workflow IDs are UUIDs** — get them from `cci-builds` output (`.workflow_id` field)
- **Branch names with `/` must be URL-encoded** — `cci-builds` handles this automatically; when using `cci-curl` directly, encode manually (e.g. `jvolante%2Fmy-branch`)
- **`cci-log` falls back to presigned URLs** — on self-hosted CCI instances the direct `/output` endpoint may return 404; `cci-log` automatically fetches presigned URLs from the v1.1 job detail instead
