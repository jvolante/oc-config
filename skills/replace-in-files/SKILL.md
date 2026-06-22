---
description: perform regex find-and-replace across files in a directory tree using the replace-in-files script. Load this skill when the user wants to rename symbols, update strings, or do bulk text substitution across source files.
name: replace-in-files
---

# replace-in-files

Perform find-and-replace across an entire directory tree. Uses ripgrep for
file discovery (binary files and `.gitignore` entries are skipped
automatically) and `perl -pe` for substitution. Files are processed in
parallel.

## Usage

```
replace-in-files [OPTIONS] <perl-script> <directories...>
```

The `perl-script` argument is passed directly to `perl -pe`, giving access to
the full perl substitution syntax: PCRE patterns, `/e` eval, helper subs,
named captures, and more.

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
replace-in-files -t py 's/old_name/new_name/g' src/

# Word boundary — avoids partial matches (e.g. won't touch 'old_names')
replace-in-files 's/\bold_name\b/new_name/g' src/

# Case-insensitive
replace-in-files 's/\bcolour\b/color/gi' src/

# Capture group — rename issue prefix
replace-in-files 's/ISL-(\d+)/PROJ-$1/g' .

# Named capture
replace-in-files 's/(?<id>\d+)/ID_$+{id}/g' .

# Evaluate replacement as perl expression (double all integers)
replace-in-files 's/\b(\d+)\b/$1 * 2/ge' src/

# Case-preserving substitution via helper sub
replace-in-files 'sub fc { $_[0] eq uc $_[0] ? uc $_[1] : $_[0] eq ucfirst lc $_[0] ? ucfirst $_[1] : $_[1] } s/\bcolour\b/fc($&, "color")/gie' src/

# Dry-run first to review changes
replace-in-files -d 's/foo/bar/g' src/ lib/

# Multiple file types
replace-in-files -t cpp -t hpp 's/OldClass/NewClass/g' src/

# All text files under current directory
replace-in-files 's/2024/2025/g' ./

# With backup files
replace-in-files -b .bak 's/old/new/g' src/

# Limit parallelism
replace-in-files -j 4 's/foo/bar/g' src/
```

## Workflow

Dry-run output is valid unified diff — pipe it to a patch file and apply later
or share for review:

```bash
# Generate patch (informational messages go to stderr, not into the file)
replace-in-files -d 's/ISL-(\d+)/PROJ-$1/g' src/ > changes.patch

# Review — use the Read tool to inspect changes.patch

# Apply from the same directory the patch was generated in
patch -p0 < changes.patch

# Or reverse it
patch -p0 -R < changes.patch
```

## Perl Script Tips

- `s/pat/rep/g` — replace all occurrences per line
- `s/pat/rep/gi` — case-insensitive
- `s/pat/rep/ge` — evaluate `rep` as a perl expression
- `\b` — word boundary
- `$1`, `$2` — capture group backreferences
- `$&` — the full match
- `$+{name}` — named capture (`(?<name>...)`)
- Multiple statements separated by `;` — define helper subs before the substitution
- `(?i)` inline flag works too: `'s/(?i)colour/color/g'`

## Common Mistakes to Avoid

- **Always dry-run first** — in-place changes have no built-in undo
- **Single-quote the script** — prevents the shell from expanding `$1`, `$&`, `\d`, etc. before perl sees them
- **Use `\b` for identifier renames** — bare `'s/Foo/Bar/g'` will also match `FooBar`
- **Use `-t` to narrow scope** — avoids accidentally modifying generated files or lock files
- **`/g` is not implicit** — unlike `sed`, perl won't replace all occurrences without the `g` flag
