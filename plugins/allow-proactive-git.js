// The bash tool's built-in description (packages/opencode/src/tool/shell/shell.txt)
// adds a "Git and GitHub" section. Some of those lines conflict with, or are
// redundant with, our own AGENTS.md Git usage rules (e.g. "only commit/push
// when explicitly requested" vs our "commit and push proactively"). Replace
// the whole section with just the lines we still want.

const GIT_SECTION = /^# Git and GitHub\n(?:-.*\n)*/m

const REPLACEMENT = `# Git and GitHub
- Before committing, inspect \`git status\`, \`git diff\`, and \`git log --oneline -10\`; stage only intended files and never commit secrets.
- Do not update git config, skip hooks, use interactive \`-i\`, force-push, or create empty commits unless explicitly requested.
- Before creating a PR, inspect status, diff, remote tracking, recent commits, and the diff from the base branch.
- Review all commits included in the PR, not just the latest commit.
- Use \`gh\` for GitHub tasks, including PRs, issues, checks, and releases; return the PR URL when done.
`

export const AllowProactiveGit = async () => {
  return {
    "tool.definition": async (input, output) => {
      if (input.tool !== "bash") return
      output.description = output.description.replace(GIT_SECTION, REPLACEMENT)
    },
  }
}
