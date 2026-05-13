---
description: perform regex find-and-replace across files in a directory tree using the replace-in-files script. Load this skill when the user wants to rename symbols, update strings, or do bulk text substitution across source files.
name: replace-in-files
---

# replace-in-files

Perform regex find-and-replace across an entire directory tree. Uses ripgrep
for file discovery (binary files and `.gitignore` entries are skipped
automatically) and `sd` for replacement (Rust regex, full PCRE-like syntax).
Files are processed in parallel.

## Script Location

```
/home/jvolante/jvscripts/replace-in-files
```

## Usage

```
replace-in-files [OPTIONS] <pattern> <replacement> <directories...>
```

Pattern and replacement use **Rust regex** syntax — `\d`, `\w`, `\b`, `\s`,
`(?:...)`, lookaheads, and named captures all work. Capture groups are
referenced in the replacement as `$1`, `$2`, or `${name}`.

## Options

| Flag | Description |
|---|---|
| `-t TYPE` / `--type TYPE` | Filter to a specific file type (repeatable). Uses ripgrep type names: `py`, `js`, `ts`, `rs`, `cpp`, `sh`, `md`, etc. Run `rg --type-list` to see all types. |
| `-j NUM` / `--jobs NUM` | Max parallel jobs (default: `nproc`) |
| `-b EXT` / `--backup EXT` | Copy each file to `<file><EXT>` before modifying (e.g. `.bak`) |
| `-d` / `--dry-run` | Show a unified diff of what would change without modifying any files |
| `-h` / `--help` | Show usage |

## Examples

```bash
# Rename a symbol in all Python files
replace-in-files -t py 'old_name' 'new_name' src/

# Word boundary — avoids partial matches (e.g. won't touch 'old_names')
replace-in-files '\bold_name\b' 'new_name' src/

# Capture group — rename issue prefix
replace-in-files 'ISL-(\d+)' 'PROJ-$1' .

# Named capture
replace-in-files '(?P<prefix>ISL)-(\d+)' 'PROJ-$2' .

# Dry-run first to review changes
replace-in-files -d 'foo' 'bar' src/ lib/

# Multiple file types
replace-in-files -t cpp -t hpp 'OldClass' 'NewClass' src/

# All text files under current directory
replace-in-files '2024' '2025' ./

# With backup files
replace-in-files -b .bak 'old' 'new' src/

# Limit parallelism
replace-in-files -j 4 'foo' 'bar' src/
```

## Workflow

1. **Always dry-run first** for broad or risky changes:
   ```bash
   replace-in-files -d '<pattern>' '<replacement>' <dirs>
   ```
   Outputs a unified diff per modified file. All paths are relative to `$PWD`.

2. Review the output, then run without `-d` to apply.

3. For extra safety, add `-b .bak` to keep backups of every modified file.

### Patch file workflow

Dry-run output is valid unified diff — pipe it to a patch file and apply later
or share for review:

```bash
# Generate patch (informational messages go to stderr, not into the file)
replace-in-files -d 'ISL-(\d+)' 'PROJ-$1' src/ > changes.patch

# Review — use the Read tool to inspect changes.patch, not cat

# Apply from the same directory the patch was generated in
patch -p0 < changes.patch

# Or reverse it
patch -p0 -R < changes.patch
```

## Regex Tips

- `\b` — word boundary (avoids partial identifier matches)
- `\d+` — one or more digits
- `\w+` — word characters (letters, digits, underscore)
- `(?i)` — case-insensitive flag at the start of the pattern
- Alternate delimiter not needed — pattern and replacement are separate args,
  no `s/.../.../{flags}` wrapper required
- To match a literal `.` or `(`, escape it: `\\.`, `\\(`

## Common Mistakes to Avoid

- **Always dry-run first** — in-place changes have no built-in undo
- **Quote both arguments** — prevents the shell from expanding `$1`, `*`, `\d`, etc.
- **Use `\b` for identifier renames** — bare `'Foo'` will also match `FooBar`
- **Use `-t` to narrow scope** — avoids accidentally modifying generated files or lock files
- **`$1` in the replacement must be single-quoted or escaped** — `"PROJ-$1"` would expand `$1` as a shell variable before `sd` sees it
