---
description: Find open PRs with failing CI, diagnose failures, and fix or rerun them
agent: build
---

Load the `open-prs` and `circleci` skills now, before doing anything else.

Then follow this workflow:

## 1. Find failing PRs

Use `open-prs --json` to list all open PRs. Filter to those where the CI rollup
state is `FAILURE`. If none are found, report that and stop.

## 2. For each failing PR, fetch CircleCI logs

- Run `cci-projects` to discover the correct project slug — never guess or hardcode it
- Dispatch a @build-test-summarizer to run `cci-failed-logs <project-slug> <branch>` and summarize the output; this fetches logs for the most recently failed job on the branch
- From the summarized output, extract actionable errors (file:line references, compiler/lint errors)

## 3. Classify the failure

- **Timeout / infrastructure**: log ends with `Killed by signal`, `(cancelled)`, or
  no actionable file:line errors found → **rerun** with `cci-rerun <workflow_id>`,
  skip code fix
- **Code / lint error**: actionable file:line errors present → proceed to fix

## 4. Clone or update the branch

Default clone target directory: `/tmp` (use `$ARGUMENTS` as the target dir if provided).

- If `<target_dir>/<repo-name>` does not exist:
  `gh repo clone <host>/<org>/<repo> <target_dir>/<repo-name> -- --branch <branch>`
- If it already exists:
  `git -C <target_dir>/<repo-name> fetch origin && git -C <target_dir>/<repo-name> checkout <branch> && git -C <target_dir>/<repo-name> reset --hard origin/<branch>`

## 5. Fix code errors

Dispatch a @software-engineer for each PR that has code errors, providing:
- The absolute path to the cloned repo
- The branch name
- The exact error lines extracted from the log
- Instruction to commit the fix and push to origin
- Instruction to dispatch a @build-test-summarizer to run `cci-wait-on-jobs` to wait on the restarted pipelines and summarize any failed pipeline logs

Run all @software-engineer dispatches in parallel when there are multiple PRs to fix.

## 6. Report

For each PR summarize:
- **Fixed**: commit message and push status
- **Rerun**: new workflow ID
- **Manual**: anything that couldn't be classified automatically
