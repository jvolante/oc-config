---
description: query, trigger, and rerun CircleCI pipelines and workflows via the REST API. Load this skill when the user asks about CI builds, pipelines, workflows, build failures, CircleCI, or wants to trigger or rerun a build.
name: circleci
---

# /circleci

Interact with CircleCI using the REST API via `cci-curl` and `jq`. All operations
read credentials from environment variables — never hardcode them.

## Project Slugs

Project-scoped commands take a slug. The canonical form may be `gh/<org>/<repo>` or
`github/<org>/<repo>` depending on the instance — **do not hardcode it**.

**For the current git repo, use `cci-slug`** (or `cci-current` / `cci-pipeline-status`,
which call it internally). It parses the origin remote — handling SSH and HTTPS, with or
without a `.git` suffix, including self-hosted GHE remotes like
`git@ghe.anduril.dev:org/repo` — and resolves the canonical slug via the API.

Use `cci-projects` only to discover slugs for *other* repos you're not checked out in. Its
output is a JSON **array**, so consume it directly:
`cci-projects | jq -r '.[] | select(.url | test("my-repo")) | .slug'`.

## Multi-workflow repos

A single commit usually triggers **several workflows** (e.g. `build-and-test`,
`build-release`, `build-debug`, `sheath-scan-and-upload`), each with its own jobs and build
numbers. The same commit subject appears on all of them, so a flat list of builds is
ambiguous. Two consequences:

- Use `cci-pipeline-status [sha]` to see the full pipeline → workflow → job tree for a
  commit and map a failing build number back to its workflow.
- When you only care about the PR-gating workflow, scope log fetching with
  `cci-failed-logs --workflow build-and-test` so unrelated workflows (sheath, debug builds)
  don't shadow the failure you want.

## Helper Functions (preferred)

```bash
# Current user info
cci-me

# List all followed projects
cci-projects

# Resolve the canonical project slug for the current git repo (handles SSH/HTTPS, GHE)
cci-slug

# Latest pipeline + workflow statuses for the current git branch (infers slug and branch automatically)
cci-current

# Full pipeline -> workflow -> job tree for a commit (default: current branch remote HEAD)
cci-pipeline-status                             # remote HEAD of current branch
cci-pipeline-status <sha>                       # specific commit
cci-pipeline-status <sha> <project-slug> [branch]

# Fetch logs for the most recently failed job on the current branch (infers slug and branch automatically)
cci-failed-logs                                 # uses current git repo and branch
cci-failed-logs --workflow build-and-test       # scope to one workflow (ignore sheath/debug noise)
cci-failed-logs <project-slug>                  # specific project, current branch
cci-failed-logs <project-slug> <branch>         # specific project and branch

# Wait for all CI jobs on the current remote HEAD to finish; prints failure logs automatically.
# Exits 0 on full success, non-zero if any job failed. Ignores stale pipelines from prior pushes
# by matching the pipeline to the current remote tracking SHA. _Do not_ use your own commands to wait
# on jobs, use this one.
cci-wait-on-jobs                                # infer slug + branch from git
cci-wait-on-jobs <project-slug> [branch]        # explicit slug/branch
cci-wait-on-jobs --timeout <seconds>            # give up after N seconds (default: 36000)
cci-wait-on-jobs --no-logs                      # don't automatically print logs for failed jobs on completion

# Build log output for a job (falls back to presigned URLs on self-hosted CCI instances)
cci-log <project-slug> <build-num>

# It's best to dispatch @build-test-summarizer to run cci-failed-logs, cci-log, or cci-wait-on-jobs for you so it will
# summarize the output.
#
# When dispatching the summarizer: give it the EXACT build numbers and the expected workflow
# name (get them from cci-pipeline-status first). Ask it to report verbatim error lines plus
# the commit SHA it observed, and to NOT synthesize a root cause. On multi-workflow repos it
# can otherwise grab logs from the wrong workflow (e.g. sheath) and misreport which jobs failed.

# Recent builds across all projects
cci-recent [limit]                              # default 25

# Builds for a specific project
cci-builds <project-slug> [branch] [limit]

# Full workflow detail with all jobs
cci-workflow <workflow-id>

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
- **Project slugs are not always `github/<org>/<repo>`** — the canonical form may be `gh/<org>/<repo>`; resolve with `cci-slug` (current repo) or `cci-projects` (other repos), never hardcode
- **On multi-workflow repos, verify a build's workflow before trusting it** — one commit triggers several workflows with the same subject; use `cci-pipeline-status` and `cci-failed-logs --workflow <name>` to scope correctly
- **`cci-rerun` reruns from failed jobs** — use `cci-trigger` to start a fresh pipeline from scratch
- **Don't use raw `curl` directly** — use `cci-curl` so auth and the base URL are handled consistently
- **Workflow IDs are UUIDs** — get them from `cci-builds` output (`.workflow_id` field)
- **Branch names with `/` must be URL-encoded** — `cci-builds` handles this automatically; when using `cci-curl` directly, encode manually (e.g. `jvolante%2Fmy-branch`)
- **`cci-log` falls back to presigned URLs** — on self-hosted CCI instances the direct `/output` endpoint may return 404; `cci-log` automatically fetches presigned URLs from the v1.1 job detail instead
