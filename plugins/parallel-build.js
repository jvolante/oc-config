// Inject parallelism flags into build tool invocations, forcing 8 jobs.
// Each tool has its own flag syntax.
//
// Supported tools:
//   make          -j 8        (also gmake, bmake)
//   ninja         -j 8
//   cmake --build --parallel 8
//   cargo         -j 8        (build, test, check, clippy, bench, run)
//   bazel         --jobs=8    (build, test, run, coverage, query)
//   scons         -j 8
//   buck2         -j 8        (build, test, run)
//   gprbuild      -j 8

/**
 * @typedef {{ match: RegExp, alreadySet: RegExp, replaceExisting: (line: string) => string, inject: (m: RegExpMatchArray) => string }} Rule
 */

// Most tools share the same "already has -j" check
const MAX_JOBS = 8
const HAS_J = /(?:^|\s)-j\d*(?=\s|=|$)/
const JOB_VALUE = "(?:\\d+|\\s*=\\s*\\d+|\\s+\\d+)?"

/** @param {string} line @param {string} flag */
function replaceJobs(line, flag) {
  return line.replace(
    new RegExp(`(^|\\s)(?:-j${JOB_VALUE}|--jobs${JOB_VALUE}|--parallel${JOB_VALUE})(?=\\s|$)`, "g"),
    `$1${flag}`,
  )
}

/** @type {Rule[]} */
const RULES = [
  // make / gmake / bmake — also guard against --jobs and -l (load average)
  {
    match: /^(g?make|bmake)(\s|$)/,
    alreadySet: /(?:^|\s)(?:-j\d*|--jobs\d*|-l)(?=\s|=|$)/,
    replaceExisting: (line) => replaceJobs(line, `-j ${MAX_JOBS}`),
    inject: (m) => m[0].trimEnd() + ` -j ${MAX_JOBS}`,
  },
  // cmake --build — uses --parallel; also accept -j as already-set
  {
    match: /^cmake\s+--build\s+\S+/,
    alreadySet: /(?:^|\s)(?:--parallel\d*|-j\d*)(?=\s|=|$)/,
    replaceExisting: (line) => replaceJobs(line, `--parallel ${MAX_JOBS}`),
    inject: (m) => m[0].trimEnd() + ` --parallel ${MAX_JOBS}`,
  },
  // cargo — also guard against --jobs
  {
    match: /^cargo\s+(?:build|test|check|clippy|bench|run)(\s|$)/,
    alreadySet: /(?:^|\s)(?:-j\d*|--jobs\d*)(?=\s|=|$)/,
    replaceExisting: (line) => replaceJobs(line, `-j ${MAX_JOBS}`),
    inject: (m) => m[0].trimEnd() + ` -j ${MAX_JOBS}`,
  },
  // bazel — uses --jobs=N
  {
    match: /^bazel\s+(?:build|test|run|coverage|query)(\s|$)/,
    alreadySet: /(?:^|\s)--jobs(?:=|\s|$)/,
    replaceExisting: (line) => replaceJobs(line, `--jobs=${MAX_JOBS}`),
    inject: (m) => m[0].trimEnd() + ` --jobs=${MAX_JOBS}`,
  },
  // everything else uses plain -j 8
  { match: /^ninja(\s|$)/,                      alreadySet: HAS_J, replaceExisting: (line) => replaceJobs(line, `-j ${MAX_JOBS}`), inject: (m) => m[0].trimEnd() + ` -j ${MAX_JOBS}` },
  { match: /^scons(\s|$)/,                      alreadySet: HAS_J, replaceExisting: (line) => replaceJobs(line, `-j ${MAX_JOBS}`), inject: (m) => m[0].trimEnd() + ` -j ${MAX_JOBS}` },
  { match: /^buck2\s+(?:build|test|run)(\s|$)/, alreadySet: HAS_J, replaceExisting: (line) => replaceJobs(line, `-j ${MAX_JOBS}`), inject: (m) => m[0].trimEnd() + ` -j ${MAX_JOBS}` },
  { match: /^gprbuild(\s|$)/,                   alreadySet: HAS_J, replaceExisting: (line) => replaceJobs(line, `-j ${MAX_JOBS}`), inject: (m) => m[0].trimEnd() + ` -j ${MAX_JOBS}` },
]

/** @param {string} line */
function injectJobs(line) {
  const trimmed = line.trimStart()
  const leadingWs = line.slice(0, line.length - trimmed.length)

  for (const rule of RULES) {
    const m = trimmed.match(rule.match)
    if (!m) continue
    if (rule.alreadySet.test(trimmed)) {
      return leadingWs + rule.replaceExisting(trimmed)
    }
    // Replace only the matched prefix, preserving the rest of the line
    const prefix = rule.inject(m)
    const suffix = trimmed.slice(m[0].length)
    const separator = suffix.length > 0 ? " " : ""
    return leadingWs + trimmed.replace(rule.match, prefix.replace(/\$/g, "$$$$") + separator)
  }
  return line
}

export const ParallelBuild = async () => {
  return {
    "tool.execute.before": (_input, output) => {
      if (output.tool !== "bash") return
      const original = output.args?.command
      if (typeof original !== "string") return
      output.args.command = original.split("\n").map(injectJobs).join("\n")
    },
  }
}
