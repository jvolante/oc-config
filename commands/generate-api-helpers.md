---
description: Turn an OpenAPI or GraphQL spec into shell helper functions, a SKILL.md, and optionally a Nix shellinit package
agent: plan
---

# generate-api-helpers

Generate a new API helper integration from an OpenAPI (JSON/YAML) or GraphQL SDL
specification. Produces:

- A `<service>_helpers_rc.sh` — shell functions wrapping `api_curl` + `jq`
- A `SKILL.md` — documents the helpers for OpenCode / Claude Code agents
- Optionally: a Nix `shellinit` package that installs the module for auto-discovery

The argument `$ARGUMENTS` may be:
- A local file path  (e.g. `./petstore.yaml`)
- A URL             (e.g. `https://api.example.com/openapi.json`)
- Omitted           (you will be asked)

---

## Phase 1 — Gather context

### Step 1 — Read the spec

- If `$ARGUMENTS` is a local path, read the file directly.
- If it is a URL, fetch it: `curl -sL "$url"`.
- If nothing was provided, ask the user for a path or URL before continuing.

Parse the spec and extract:
- **Service name** — derive a short lowercase kebab-case name from the `info.title`
  (e.g. "Petstore" → `petstore`, "GitHub REST API" → `github`). Confirm with the user
  if ambiguous.
- **Base URL** — from `servers[0].url` (OpenAPI v3) or `host` + `basePath` (Swagger v2).
- **Auth scheme(s)** — bearer token, basic auth, API key header, OAuth2, etc.
- **Endpoint groups** — group all paths by tag or by the first path segment.

### Step 2 — Endpoint and workflow elicitation

Present a summary of the endpoint groups found, then ask **two questions in one message**:

> **Which operations matter most?**
> Here are the endpoint groups I found: [list groups with path counts].
> Which should become named helpers? (Say "all", name specific groups, or describe what
> you'd use it for — I'll select the right endpoints.)
>
> **Are there common multi-step workflows?**
> For example:
> - "I often search for X then fetch full details on each result"
> - "I need to poll an endpoint until a status field reaches a terminal value"
> - "Creating Y always requires looking up an ID for Z first"
> - "I want a single command that shows me the current state of [thing]"
>
> Describe any workflows you know of — these become composite helpers (functions that
> chain multiple API calls internally, like a `wait-until-done` or `get-current-status`
> helper). Skip if none come to mind.

Wait for the user's response before proceeding.

### Step 3 — Output location

Ask:

> **Where should the files be written?**
> 1. Directory for `<service>_helpers_rc.sh` — e.g. `~/myrepo/` or `~/.local/share/shellinit/`
> 2. Directory for `SKILL.md` — default: `~/.config/opencode/skills/<service>/`
>    (standard location for both OpenCode and Claude Code)
> 3. Generate a Nix `shellinit` package? (yes/no) — if yes, where is your `nix/` tree?

---

## Phase 2 — Plan the helpers

Design the full function set. Present it to the user for approval before writing anything.

### Naming conventions

| Kind | Pattern | Example |
|---|---|---|
| Auth helper | `_<service>_auth_header` | `_petstore_auth_header` |
| Curl wrapper | `<service>-curl` | `petstore-curl` |
| Simple helper | `<service>-<resource>[-<verb>]` | `petstore-pets`, `petstore-pet-create` |
| Composite helper | `<service>-<descriptive-name>` | `petstore-find-available`, `petstore-wait-for-order` |

Omit the verb for list/get operations when it's obvious from context:
- `petstore-pets` (lists pets) not `petstore-pets-list`
- `petstore-pet 42` (gets one pet by id) not `petstore-pet-get`
- `petstore-pet-create` (POST) — verb required because it mutates

### Shell conventions

**shellinit header** — every rc file begins with shellinit metadata, then an idempotency guard:
```bash
# shellinit:contexts=any
# shellinit:requires=api-curl
# shellinit:tools=curl,jq
# ---------------------------------------------------------------------------
# <Service> helpers
# ...
# ---------------------------------------------------------------------------

declare -f <service>-curl > /dev/null 2>&1 && return
```

`contexts` values (additive — a module tagged `interactive` is also loaded for `login`):
- `any` — always loaded regardless of context
- `interactive` — interactive shells only
- `login` — login shells only
- `noninteractive` — non-interactive shells only (agents, scripts)

Most API helpers should use `contexts=any` since they're useful in all contexts.

`requires` — comma-separated module names that must be sourced first.
Always include `api-curl` since every service uses `api_curl`.

`tools` — comma-separated binaries that must exist in PATH. Loader warns and skips
the module if any are missing. Always include `curl`; add `jq`, `python3` as needed.

**Argument validation** — every function that takes required args:
```bash
if [[ $# -lt N ]]; then
  printf 'Usage: <service>-<name> <args>\n' >&2
  return 1
fi
```

**Error output** — always `printf '...' >&2`, never `echo`.

**jq projections** — every function pipes through `jq` and emits only the fields
an agent or human needs. Never pass a raw API response through unfiltered.

**URL encoding** — path segments that may contain slashes or special characters:
```bash
local encoded
encoded="$(printf '%s' "$arg" | python3 -c 'import sys,urllib.parse; print(urllib.parse.quote(sys.stdin.read(), safe=""))')"
```

**Parallel fan-out** — when a helper must call the same endpoint for multiple items,
use background jobs:
```bash
local pids=()
for item in "${items[@]}"; do
  ( <service>-curl "endpoint/${item}" ) &
  pids+=($!)
done
for pid in "${pids[@]}"; do wait "$pid"; done
```

### Complete inline skeleton

Use this as the structural template. Replace all `<placeholder>` values.

```bash
# shellinit:contexts=any
# shellinit:requires=api-curl
# shellinit:tools=curl,jq
# ---------------------------------------------------------------------------
# <Service> helpers
# All functions read $<SERVICE>_URL and $<SERVICE>_TOKEN.
#
# Chaining examples:
#   <service>-<resources> | jq -r '.<id_field>' | xargs -I{} <service>-<resource> {}
#   <service>-<resource> <id> | jq '{<key_fields>}'
# ---------------------------------------------------------------------------

declare -f <service>-curl > /dev/null 2>&1 && return

_<service>_auth_header() {
  printf 'Bearer %s' "${<SERVICE>_TOKEN}"
}

# Raw curl wrapper — prepends $<SERVICE>_URL/<base_path>/ to the first non-flag argument.
# Usage: <service>-curl [curl flags...] <endpoint> [curl flags...]
<service>-curl() {
  api_curl "${<SERVICE>_URL}/<base_path>" "$(_<service>_auth_header)" "$@"
}

# List all <resources>
# Usage: <service>-<resources> [--<filter> <value>]
# Chains: <service>-<resources> | jq -r '.<id_field>' | xargs -I{} <service>-<resource> {}
<service>-<resources>() {
  <service>-curl "<endpoint>" \
  | jq '[.[] | {<id_field>, <name_field>, <status_field>}]'
}

# Get a single <resource> by id
# Usage: <service>-<resource> <id>
<service>-<resource>() {
  if [[ $# -lt 1 ]]; then
    printf 'Usage: <service>-<resource> <id>\n' >&2
    return 1
  fi
  <service>-curl "<endpoint>/$1" \
  | jq '{<id_field>, <name_field>, <status_field>, <detail_fields>}'
}

# Create a new <resource>
# Usage: <service>-<resource>-create <name> [<optional>]
<service>-<resource>-create() {
  if [[ $# -lt 1 ]]; then
    printf 'Usage: <service>-<resource>-create <name>\n' >&2
    return 1
  fi
  <service>-curl -X POST \
    -d "$(jq -n --arg name "$1" '{name: $name}')" \
    "<endpoint>" \
  | jq '{<id_field>, <name_field>}'
}

# --- Composite helper example: poll until terminal state ---
# Usage: <service>-wait-for-<resource> <id> [--interval <seconds>] [--timeout <seconds>]
<service>-wait-for-<resource>() {
  if [[ $# -lt 1 ]]; then
    printf 'Usage: <service>-wait-for-<resource> <id> [--interval N] [--timeout N]\n' >&2
    return 1
  fi
  local id="$1"; shift
  local interval=10 timeout=300 elapsed=0
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --interval) interval="$2"; shift 2 ;;
      --timeout)  timeout="$2";  shift 2 ;;
      *) printf '<service>-wait-for-<resource>: unknown option "%s"\n' "$1" >&2; return 1 ;;
    esac
  done
  local terminal_states='["<done_state>","<failed_state>","<cancelled_state>"]'
  while true; do
    local status
    status=$(<service>-<resource> "$id" | jq -r '.<status_field>')
    local done
    done=$(printf '%s' "$status" | jq -r --argjson t "$terminal_states" '. as $s | $t | contains([$s])')
    if [[ "$done" == "true" ]]; then
      printf '%s reached terminal state: %s\n' "$id" "$status"
      return 0
    fi
    elapsed=$(( elapsed + interval ))
    if [[ "$elapsed" -ge "$timeout" ]]; then
      printf '<service>-wait-for-<resource>: timed out after %ds (last status: %s)\n' \
        "$timeout" "$status" >&2
      return 1
    fi
    sleep "$interval"
  done
}
```

### Composite helper design guidance

When the user described multi-step workflows, design composite helpers that:

- **Status shortcuts** (`<service>-current`, `<service>-status`): call 2–3 endpoints
  and combine their output into a single human-readable summary. Infer context from
  environment variables or git when possible (e.g. current branch, current project).

- **Polling helpers** (`<service>-wait-for-*`): loop with configurable `--interval`
  and `--timeout`, print progress, exit non-zero on failure. See skeleton above.

- **Lookup-then-act helpers**: when creating/updating requires an ID that must first
  be resolved by name, do the lookup internally so the caller passes a human-readable
  name rather than an opaque ID.

- **Bulk helpers**: accept multiple IDs and fan out in parallel using background jobs.

### Plan presentation

Present the complete planned function list in this format before writing any files:

```
Service : <service>
Base URL: <url>
Auth    : <scheme>

Functions:
  <service>-curl              — raw curl wrapper (always generated)
  <service>-<name>            — <one-line description>  [simple|composite]
  ...

Composite helpers:
  <service>-<name>  — <what it does and why it's composite>
  ...

Files to write:
  <output_dir>/<service>_helpers_rc.sh
  <skills_dir>/<service>/SKILL.md
  [<nix_dir>/pkgs/<service>-helpers/default.nix]  (if Nix requested)
```

Ask: "Does this look right? Any functions to add, remove, or rename?"

---

## Phase 3 — Generate files

Once approved, write all files. The rc file and SKILL.md can be written in parallel;
write the Nix package only if the user confirmed it in Phase 1.

### 1. `<service>_helpers_rc.sh`

Follow the skeleton and conventions above exactly. The file must:
- Begin with the shellinit header block (contexts, requires, tools)
- Follow immediately with the idempotency guard (`declare -f <service>-curl ...`)
- Define `_<service>_auth_header` and `<service>-curl` first
- Group functions by resource, composite helpers last
- Include a header block with chaining examples drawn from the actual functions generated

### 2. `SKILL.md`

Write to `<skills_dir>/<service>/SKILL.md`. Use this structure exactly — do not
reference any external files or paths outside of what the user provided:

```markdown
---
description: <one-line trigger description — what an agent should say to load this skill>
name: <service>
---

# <service>

<one-paragraph description of what this API does>

## Environment Variables

| Variable | Description |
|---|---|
| `$<SERVICE>_URL` | Base URL — e.g. `https://api.example.com` |
| `$<SERVICE>_TOKEN` | Bearer token / personal access token |
| `$<SERVICE>_<OTHER>` | (any other vars the helpers read) |

## Setup

The helpers require `api_curl` to be available. If using shellinit:

    eval "$(shellinit noninteractive)"   # or: interactive, login

If sourcing manually:

    source /path/to/api_curl_helpers_rc.sh
    source /path/to/<service>_helpers_rc.sh

## Helper Functions (preferred)

Always use named helpers over raw `<service>-curl`. All output is JSON and pipes
cleanly into `jq`.

    # List
    <service>-<resources>

    # Get one
    <service>-<resource> <id>

    # Create / mutate
    <service>-<resource>-create <name> ["<description>"]

    # Composite
    <service>-<composite-name> [args]

(One usage block per function, with a one-line description.)

## Composite Helpers

(Dedicated section for composite helpers — explain what each one does internally,
when to use it, and what it returns.)

## Raw API Access via `<service>-curl`

For endpoints the named helpers don't cover:

    # GET
    <service>-curl <endpoint> | jq '<filter>'

    # POST
    <service>-curl -X POST -d '<json>' <endpoint>

    # GET with query params
    <service>-curl <endpoint> -G --data-urlencode 'param=value'

## Key API Endpoints

| Purpose | Method | Path |
|---|---|---|
| (one row per important endpoint) | | |

## Chaining Examples

    # (3–5 realistic pipeline examples using the helpers)

## Common Mistakes to Avoid

- (auth gotchas specific to this API)
- (pagination behavior if applicable)
- (rate limiting if mentioned in the spec)
- Never omit `jq` filtering — always project to the fields you need
```

### 3. Nix shellinit package (only if requested)

Write to `<nix_dir>/pkgs/<service>-helpers/default.nix`:

```nix
{ lib, self, makeShellInitModule }:
makeShellInitModule {
  moduleName = "<service>";
  src        = self + "/<service>_helpers_rc.sh";
  meta = with lib; {
    description = "shellinit module providing <Service> shell helpers";
    license = licenses.gpl3;
  };
}
```

`makeShellInitModule` installs the rc file to `$out/share/shellinit/<service>.sh`,
making it auto-discoverable by `shellinit` when `SHELLINIT_PATH` includes that
package's `share/shellinit/` directory.

Then add to the overlay's `jvscripts` attrset in `<nix_dir>/overlay.nix`:
```nix
<service>-helpers = final.callPackage ./pkgs/<service>-helpers { inherit self; };
```

And add to `flake.nix`'s `packages` attrset in the `perSystem` block:
```nix
<service>-helpers = pkgs'.jvscripts.<service>-helpers;
```

#### Non-Nix install

If the user doesn't use Nix, copy the rc file directly into their shellinit path:

```bash
install -Dm644 <service>_helpers_rc.sh ~/.local/share/shellinit/<service>.sh
```

Then ensure `SHELLINIT_PATH` includes `~/.local/share/shellinit` and
`shellinit` is installed somewhere in PATH.

---

## Phase 4 — Verify

Run these checks using the actual paths from Phase 1:

1. **Syntax check the rc file:**
   ```bash
   bash -n <output_dir>/<service>_helpers_rc.sh && printf 'syntax ok\n'
   ```

2. **Dry-source — confirm all functions are defined:**
   ```bash
   bash --norc -c "
     api_curl() { :; }
     source <output_dir>/<service>_helpers_rc.sh
     declare -F | grep <service>
   "
   ```

3. **Check shellinit discovers the module (if installed):**
   ```bash
   shellinit --list | grep <service>
   shellinit --dry-run noninteractive | grep <service>
   ```

4. **If Nix was requested — build and inspect:**
   ```bash
   nix build <nix_dir>#<service>-helpers
   ls result/share/shellinit/
   ```

Report results. If any check fails, fix before reporting done.
