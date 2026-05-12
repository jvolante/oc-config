---
description: Resolve unresolved review comments on the PR for the current branch
---

Load the `open-prs` skill now, before doing anything else.

## 1. Find the PR

Get the current branch name:
```
!`git branch --show-current`
```

Use `open-prs --json` to find the open PR whose `headRefName` matches the current branch. Extract the PR number, repo (`repository.nameWithOwner`), and GHE hostname from the PR URL.

Also extract the list of reviewers who have `CHANGES_REQUESTED` reviews — you will need them for step 9:

```bash
open-prs --json | jq '[.[] | select(.headRefName == "<branch>") |
  { number, url,
    nameWithOwner: .repository.nameWithOwner,
    changes_requested_by: ([.reviews.nodes[].author.login] | unique) }]'
```

If no matching PR is found, report that and stop.

## 2. Fetch review threads and top-level review comments

Run both fetches in parallel.

**Inline review threads** (GraphQL):

```
gh api graphql --hostname <hostname> -f query='
{
  repository(owner: "<org>", name: "<repo>") {
    pullRequest(number: <number>) {
      reviewThreads(first: 50) {
        nodes {
          id
          isResolved
          isOutdated
          comments(first: 10) {
            nodes {
              databaseId
              body
              path
              line
              author { login }
            }
          }
        }
      }
    }
  }
}'
```

Filter to threads where **both** `isResolved: false` and `isOutdated: false`. These are the active, unresolved inline threads.

**Top-level review body comments** (REST):

```bash
gh api repos/<org>/<repo>/pulls/<number>/reviews --hostname <hostname> \
  | jq '[.[] | select(.body != "" and .body != null) | {id, state, login: .user.login, body}]'
```

Include reviews in any state (`CHANGES_REQUESTED`, `COMMENTED`, `APPROVED`) that have a non-empty body — these are actionable review-level comments that do not appear in `reviewThreads`. Treat each as an item to classify alongside the inline threads. Note that top-level review comments have no `path`/`line` and cannot be "resolved" via GraphQL — they are replied to and tracked, but the resolve mutation does not apply.

## 3. Get the GitHub login for TODO attribution

```bash
gh api user --hostname <hostname> --jq .login
```

Use the returned login (e.g. `jvolante`) as the TODO owner: `TODO(jvolante):`. Do **not** use `git config user.name` — that returns a display name, not a username.

## 4. Classify each item

Read the relevant source files to understand context. Classify every active thread and every top-level review comment as one of:

- **Actionable** — a concrete code change is required (fix, refactor, rename, restructure, etc.)
- **TODO** — informational or future-work; should be tracked with a `TODO(<username>):` comment in the source at the relevant location
- **Skip** — purely conversational, emoji-only, or otherwise requires no change

## 5. Present the plan and wait for confirmation

Show a table summarizing every item (inline threads and top-level review comments together):

| # | ID | File | Comment (truncated) | Classification | Proposed action |
|---|----|------|---------------------|----------------|-----------------|
| 1 | abc123 (thread) | foo.cc | "move these common..." | Actionable | Move X to Y |
| 2 | 1746592 (review) | — | "unit tests for..." | Actionable | Add tests |
| 3 | def456 (thread) | bar.py | "2026 :)" | Skip | No change needed |

**Stop here and wait for the user to confirm or amend the plan before making any changes.**

## 6. Implement

Once the user confirms:

- For **Actionable** items: dispatch parallel `@software-engineer` subagents for changes that are independent of each other. Provide each with the file path, the comment body, and the proposed change.
- For **TODO** items: add `TODO(<username>): <rationale>` comments directly in the source at the relevant location.
- For **Skip** items: do nothing.

## 7. Commit and push

After all changes are applied, commit with a descriptive message and push to the current branch.

## 8. Reply and resolve each item

For every item in the plan:

**Reply** (Actionable and TODO items only — skip for Skip items):

- For **inline threads**, reply using the `databaseId` of the first comment:

```bash
gh api repos/<org>/<repo>/pulls/<number>/comments \
  --hostname <hostname> -X POST \
  -f body="<reply text>" \
  -F in_reply_to=<first_comment_database_id>
```

- For **top-level review comments**, reply as a PR issue comment:

```bash
gh api repos/<org>/<repo>/issues/<number>/comments \
  --hostname <hostname> -X POST \
  -f body="<reply text>"
```

The reply should be concise — one or two sentences, e.g. "Fixed in <commit>: moved X to Y." or "Added TODO(username): at <file>:<line>."

**Resolve** (inline threads only — Actionable, TODO, and Skip):

```
gh api graphql --hostname <hostname> -f query='
mutation {
  resolveReviewThread(input: {threadId: "<thread_id>"}) {
    thread { id isResolved }
  }
}'
```

Confirm each mutation returns `isResolved: true`. Top-level review comments cannot be resolved via this mutation — skip it for those.

## 9. Re-request review

Re-request review from every reviewer who had `CHANGES_REQUESTED` (collected in step 1):

```bash
gh api repos/<org>/<repo>/pulls/<number>/requested_reviewers \
  --hostname <hostname> -X POST \
  -f 'reviewers[]=<login>'
```
