alias oc='opencode'
alias ocm='opencode --model $OPENCODE_MEDIUM_MODEL'
alias ocs='opencode --model $OPENCODE_SMALL_MODEL'
alias occ='opencode --agent chat --model $OPENCODE_MEDIUM_MODEL'
alias fixci='opencode --model $OPENCODE_MEDIUM_MODEL --agent build run "dispatch @build-test-summarizer to run `cci-wait-on-jobs` and summarize the logs of failed jobs. If there are failed jobs, fix the issues and repeat the process. If all jobs pass, your work is done"'
export OPENCODE_EXPERIMENTAL_BASH_DEFAULT_TIMEOUT_MS=$((60 * 60 * 1000))
