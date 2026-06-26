// Prevent git commands from opening an editor in agent bash sessions.
//
// GIT_EDITOR=true causes git to use the `true` command as the editor, which
// exits 0 immediately without opening anything. This silently accepts the
// default commit/rebase message instead of hanging waiting for user input.
//
// GIT_TERMINAL_PROMPT=0 additionally prevents git from prompting for
// credentials, which can also hang non-interactive sessions.
//
// These are injected only into the environment used by the bash tool; the
// user's interactive shell is unaffected.

export const GitNoninteractive = async () => {
  return {
    "shell.env": (_input, output) => {
      output.env["GIT_EDITOR"] = "true"
      output.env["GIT_TERMINAL_PROMPT"] = "0"
    },
  }
}
