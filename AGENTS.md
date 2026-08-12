When using make, prefer `make -C /path/to/build/dir` instead of changing directories.

Practice defensive programming, use asserts and errors to make assumptions explicit
Avoid magic numbers or other ambiguous values, make constants to make code more readable

When making a plan, if the user suggests a modification to the plan it's not necessary to restate
the entire plan. Acknowledge the changes and only state what modifications will be made to the existing plan.

It's not necessary to provide a detailed breakdown of the work completed unless asked.

**IMPORTANT**: Prefer copying a file and making targeted modifications when moving or porting large pieces of code rather than
rewriting the entire file yourself. **DO NOT REWRITE ENTIRE FILES**
**IMPORTANT**: Don't ask explore agents to return the contents of an entire file. If you need that, read it yourself.
**IMPORTANT**: _Never_ write comments about what code you changed used to do, the current work plan, or reference lines in a prior implementation. Comments are for explaining design decisions of the existing system.
Avoid writing section header comments, the software syntax does a good enough job breaking up sections.
When making a PR, never reference bugs we made and fixed within the feature branch in the PR description. Only enumerate fixed bugs that come from the base branch.

Only `find` over the nix store or the whole filesystem as a last resort. It's extremely slow.

When working in systems programming languages (C/C++, Rust, Ada, etc.) it's important to try to reduce the number of memory allocations and frees the code must do **even if the surrounding code doesn't**. Software written in these languages is generally performance sensitive.
Follow this guideline in other languages as well when it doesn't compromise readability.

# Editing Process

Be efficient. The best code is the code never written.
Before writing any code, stop at the first rung that holds:

1. **Does this need to exist at all?** Speculative need = skip it, say so in one line. (YAGNI)
2. **Already in this codebase?** A helper, util, type, or pattern that already lives here → reuse it. Look before you write; re-implementing what's a few files over is the most common slop.
3. **Stdlib does it?** Use it.
4. **Native platform feature covers it?** `<input type="date">` over a picker lib, CSS over JS, DB constraint over app code.
5. **Already-installed dependency solves it?** Use it. Never add a new one for what a few lines can do.
6. **Can it be one line?** One line.
7. **Only then:** the minimum code that works.

The ladder is a reflex, not a research project — but it runs *after* you
understand the problem, not instead of it. Read the task and the code it
touches first, trace the real flow end to end, then climb. Two rungs work →
take the higher one and move on. The first lazy solution that works is the
right one — once you actually know what the change has to touch.

**Bug fix = root cause, not symptom.** A report names a symptom. Before you
edit, grep every caller of the function you're about to touch. The lazy fix IS
the root-cause fix: one guard in the shared function is a smaller diff than a
guard in every caller — and patching only the path the ticket names leaves
every sibling caller still broken. Fix it once, where all callers route through.

Rules:

- No unrequested abstractions: no interface with one implementation, no factory for one product, no config for a value that never changes.
- No boilerplate, no scaffolding "for later", later can scaffold for itself.
- Deletion over addition. Boring over clever, clever is what someone decodes at 3am.
- Fewest files possible. Shortest working diff wins — but only once you understand the problem. The smallest change in the wrong place isn't lazy, it's a second bug.
- Complex request? Ship the lazy version and question it in the same response, "Did X; Y covers it. Need full X? Say so." Never stall on an answer you can default.
- Two stdlib options, same size? Take the one that's correct on edge cases. Lazy means writing less code, not picking the flimsier algorithm.
- Mark deliberate simplifications that cut a real corner with a known ceiling (global lock, O(n²) scan, naive heuristic) with a `ponytail:` comment naming the ceiling and upgrade path (`# ponytail: global lock, per-account locks if throughput matters`).

Output:

Code first. Then at most three short lines: what was skipped, when to add it.
No essays, no feature tours, no design notes. If the explanation is longer
than the code, delete the explanation, every paragraph defending a
simplification is complexity smuggled back in as prose. Explanation the user
explicitly asked for (a report, a walkthrough, per-phase notes) is not debt,
give it in full, the rule is only against unrequested prose.

Pattern: `[code] → skipped: [X], add when [Y].`

**When NOT to be lazy**

Never simplify away: input validation at trust boundaries, error handling
that prevents data loss, security measures, accessibility basics, anything
explicitly requested. User insists on the full version → build it, no
re-arguing.

Never lazy about understanding the problem. The ladder shortens the
solution, never the reading. Trace the whole thing first — every file the
change touches, the actual flow — before picking a rung. Laziness that skips
comprehension to ship a small diff is the dangerous kind: it dresses up as
efficiency and ships a confident wrong fix. Read fully, then be lazy.

Hardware is never the ideal on paper: a real clock drifts, a real sensor
reads off, a PCA9685 runs a few percent fast. Leave the calibration knob, not
just less code, the physical world needs tuning a minimal model can't see.

Lazy code without its check is unfinished. Non-trivial logic (a branch, a
loop, a parser, a money/security path) leaves ONE runnable check behind, the
smallest thing that fails if the logic breaks: an `assert`-based
`demo()`/`__main__` self-check or one small `test_*.py`. No frameworks, no
fixtures, no per-function suites unless asked.

The shortest path to done is the right path.

# Proactive Behavior

Don't wait to be asked:
- Use a sub-agent when exploration is needed before implementing
- Use a sub-agent when building code or running unit tests to summarize failures
- Use parallel approaches when you see independent subtasks
- After completing a change to a codebase, ensure packaging, CI, and documentation are all coherent with the change

# Unit Tests

- Strongly prefer many small tests over few large ones.
- Make sure to thoroughly test all code paths, especially error handling.
- Assert `cudaSuccess` for relevant CUDA API calls in unit tests
- If a function returns a status code _always_ verify it
- If using random values in tests _always_ set the seed.
- Use the build-and-test-summarizer agent to run tests and summarize failures, test output can be very verbose

# Git usage

- Use conventional commits in commit messages
- Use feature/ fix/ docs/, etc. when creating feature branches, don't create a new branch unless asked.
- Include the Jira ticket number in the branch name
- Use conventional commits headers as PR titles, include Jira ticket number if relevant
- Commit and push proactively as work completes, without waiting for explicit
  confirmation each time. Still avoid force-push, amending others' commits,
  or rewriting shared history without being asked.

# Additional Programs

Here are some additional programs in the environment beyond what's installed on a typical system, use them when running bash commands or writing scripts to get more focused results more easily.

- `perl`: sometimes easier than `sed`
- `ast-grep` : sophisticated grep over an abstract syntax tree
- `shellcheck` : linter and formatter for shell scripts
- `jq` : Query engine for JSON
- `jaq` : Query engine for YAML, TOML, XML, and CBOR similar to `jq`
- `sage` : SageMath symbolic math toolkit
- `syspython3`: Always available python environment guaranteed to have several packages installed:
    numpy cupy scipy polars scikit-learn networkx opencv h5py sympy altair manim ortools python-sat z3-solver highspy clingo libclang

# Graphify Knowledge Graph

Many projects under have a pre-built knowledge graph at `<project-root>/graphify-out/graph.json`. This can be useful when navigating a codebase. Load the `graphify` skill for full usage instructions — it describes when and how to use it.

For cross-project work, check and query each project's graph independently from its own root directory.

# Language Specific Guidance
## Python Guidance

- **IMPORTANT**: when making a Python script or application that could use PyQt or PySide you must ALWAYS use PySide.
- Prefer using `Path` over `os.path` and strings
- Don't relative import when using python
- Use `python3` as your python command
- Prefer `pyproject.toml` over other methods of package creation and dependency enumeration
- prefer `ruff` for linting and formatting
- prefer `ty` for type checking
- If a virtual env dosen't have pip installed, it almost certainly uses `uv`

## Shell Scripting Guidance

- Prefer `printf` over `echo` in shell scripts
  - Use escape sequences for printing variables instead of expanding inside the format string
- Avoid using GNU parallel
- Prefer using `/usr/bin/env` in the shebang instead of a fixed path

## Nix Guidance

- When making a `devShell` with `mkShell`, prefer to include package dependencies using `inputsFrom` rather than copying the `buildInputs` from the packages into the `buildInputs` of the shell. This follows the DRY principle.

## C++ Guidance

- Use `const` and `constexpr` wherever possible
- prefer unique pointers over shared pointers when the ownership model supports it
- prefer `cstdint` types
- Annotate with `noexcept` and `[[nodiscard]]` and others where relevant
- Prefer `.cpp` over `.cu` for translation units that call CUDA API functions but define no device code (`__global__`, `__device__`, `<<<...>>>`). Use `.cu` only when the file contains kernel definitions or device-side code.
- Gate architecture-specific SIMD flags (`-mavx2`, `-mfma`, `-mcpu=...`) in CMake by probing the compiler with `check_cxx_compiler_flag`, NOT by branching on `CMAKE_SYSTEM_PROCESSOR`.
