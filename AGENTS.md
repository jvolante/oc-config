
When using make, prefer `make -C /path/to/build/dir` instead of changing directories.

Practice defensive programming, use asserts and errors to make assumptions explicit
Avoid magic numbers or other ambiguous values, make constants to make code more readable

When making a plan, if the user suggests a modification to the plan it's not necessary to restate
the entire plan. Acknowledge the changes and only state what modifications will be made to the existing plan.

It's not necessary to provide a detailed breakdown of the work completed unless asked.

When running the Bash tool always pass a timeout of at least 20 minutes

# Proactive Behavior

Don't wait to be asked:
- Use an agent when exploration is needed before implementing
- Use an agent when building code or running unit tests to summarize failures
- Use parallel approaches when you see independent subtasks

# Unit Tests

- Strongly prefer many small tests over few large ones.
- Make sure to thoroughly test all code paths, especially error handling.
- Assert `cudaSuccess` for relevant CUDA API calls in unit tests
- If a function returns a status code _always_ verify it
- If using random values in tests _always_ set the seed.
- Use the build-and-test-summarizer agent to run tests and summarize failures, test output can be very verbose

# Additional Programs

Here are some additional programs in the environment beyond what's installed on a typical system, use them when running bash commands or writing scripts to get more focused results more easily.

- `ast-grep` : sophisticated grep over an abstract syntax tree
- `shellcheck` : linter and formatter for shell scripts
- `jq` : Query engine for JSON
- `taplo` : utility for working with toml files, provides a linter and formatter as well as a search function similar to `jq`
- `jaq` : Query engine for YAML, TOML, XML, and CBOR similar to `jq`
- `sage` : SageMath symbolic math toolkit

# Language Specific Guidance
## Python Guidance

- *IMPORTANT*: when making a Python script or application that could use PyQt or PySide you must ALWAYS use PySide.
- Prefer using `Path` over `os.path` and strings
- Don't relative import when using python
- Use `python3` as your python command
- Prefer `pyproject.toml` over other methods of package creation and dependency enumeration
- prefer `ruff` for linting and formatting

## Shell Scripting Guidance

- Prefer `printf` over `echo` in shell scripts
  - Use escape sequences for printing variables instead of expanding inside the format string
- Avoid using GNU parallel
- Prefer using `/usr/bin/env` in the shebang instead of a fixed path

## Nix Guidance

- When making a `devShell` with `mkShell`, prefer to include package dependencies using `inputsFrom` rather than copying the `buildInputs` from the packages into the `buildInputs` of the shell. This follows the DRY principle.

## C++ Guidance

- Use `const` and `constexpr` wherever possible
- prefer `cstdint` types
