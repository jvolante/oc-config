---
description: Diagnose and fix flake dependency pin drift in C++ nix repos. Load this skill when a build fails due to missing symbols or headers that should come from a pinned dependency, when you need to locate a header file in a nix-built project, or when you need to temporarily or permanently update a flake input pin.
name: nix-dep-investigation
---

# /nix-dep-investigation

Procedures for diagnosing dependency drift in flake-based C++ repos and
locating headers/symbols without brute-force nix store scans.

---

## Finding a header file

Work down this list in order — each step is faster than the next.

### Step 1 — `search-header` (fastest; requires compile_commands.json)

```bash
search-header -C <build>/compile_commands.json <header_name>
# Examples:
search-header -C build/compile_commands.json orin_ftc_recording.pb.h
search-header -C build/compile_commands.json cuda_runtime.h
search-header -C build/compile_commands.json nlohmann/json.hpp
```

`search-header` collects include paths from four sources automatically:
1. Inline `-I`/`-isystem` flags in each compile command (standard C/C++ TUs).
2. `--options-file *.rsp` response files (CUDA/nvcc TUs store includes here).
3. `clang++ -print-resource-dir` and `nvcc --dryrun` (compiler builtin/CUDA toolkit headers).
4. CMakeCache `*_INCLUDE_DIR` entries (nix CUDA merged packages found via `find_package`).

**Limitation:** only finds headers from packages already linked to a compiled TU.
If the build is broken and a TU never compiled, its include paths won't appear.
Fall through to Step 2 in that case.

### Step 2 — `nix eval` to get a known package's store path (no scanning)

If you know which nix package provides the header, get its store path directly:

```bash
# From inside a dev shell — packages are available by name
nix eval --raw '.#packages.<system>.<pkg-name>'

# Or query via a flake reference
nix eval --raw 'git+file:///path/to/overlay#packages.x86_64-linux.<pkg-name>'

# Then inspect the store path directly
ls /nix/store/<hash>-<pkg>-dev/include/
```

### Step 3 — `nix-tree` (interactive; no store scan)

Browse the full dependency closure interactively. Press `/` to search by name,
`w` to open a why-depends modal for any selected node:

```bash
nix-tree /nix/store/<hash>-<drv>
# Or from a flake attr:
nix-tree '.#devShells.x86_64-linux.default'
```

### Step 4 — `nix why-depends` (targeted; explains a specific dep)

Show why a package pulls in another, with the file that references it:

```bash
nix why-depends <package-flake-ref> <dependency-flake-ref>
# Example: confirm an myapis follows override actually took effect
nix why-depends '.#packages.x86_64-linux.my-app' \
  '/nix/store/<hash>-myapis-...'
```

### Avoid `find /nix/store`

Scanning the whole store is slow (can take minutes). Only use it as a last
resort after Steps 1–4 have failed.

---

## Diagnosing dependency pin drift

When a build fails with "no member named X" or "file not found" on a header
that should come from a dependency, the dep is likely pinned to a stale rev.

### Check what rev is currently locked

```bash
# Single input
check-flake-input-version <input-name>       # prints bare rev

# All inputs at once (useful for auditing)
check-flake-input-version                    # prints "input: rev" sorted
```

`check-flake-input-version` is not installed everywhere (e.g. remote HITL
boxes). Portable fallback with jq directly against `flake.lock`:

```bash
jq -r '.nodes["<input-name>"].locked.rev' flake.lock
```

### Check if origin/master already has the fix

Before creating branches or PRs, verify whether the fix is already on master:

```bash
# Is a specific rev already in origin/master?
git -C /path/to/repo fetch origin master
git -C /path/to/repo merge-base --is-ancestor <rev> origin/master \
  && echo "already on master" || echo "NOT on master"

# Does origin/master have a specific file/symbol at this rev?
git -C /path/to/repo ls-tree -r --name-only origin/master | grep <pattern>
git -C /path/to/repo show origin/master:<path/to/file> | grep <symbol>
```

This often reveals the fix is already on master and only the lock is stale —
no new branch or PR needed, just a lock refresh.

### Refresh a stale lock

```bash
# Refresh one input to its current ref target
nix flake lock --update-input <input-name>

# Refresh multiple inputs
nix flake lock --update-input <input-a> --update-input <input-b>
```

---

## Diagnosing architecture-specific build-flag failures

Symptom: a build fails with an error like
`clang++: error: unsupported option '-mavx2' for target 'aarch64-unknown-linux-gnu'`
— x86 SIMD flags reaching an aarch64 (Orin) target compiler.

### Confirm native vs cross, and read the actual flags

```bash
# The failing derivation's build log (shows the exact compile command + flags)
nix log /nix/store/<hash>-<pkg>.drv

# Is the derivation targeting aarch64 natively, or cross-built from x86?
nix derivation show /nix/store/<hash>-<pkg>.drv | jq -r '.[].system'
```

Root cause is usually a CMake that gates SIMD flags on
`CMAKE_SYSTEM_PROCESSOR`, which under some nix cmake setup-hook / stdenv
configurations reports the build host (x86_64) even for a native aarch64
target compiler — so the x86 branch is taken and `-mavx2 -mfma` is emitted.

### Distinguish pin-drift from a genuine build bug

Check whether the SAME package ever built successfully in this store. If a
prior output exists, the current pin drifted onto a broken rev; if none exists,
it's a real build bug in the pinned source.

```bash
# Prior successful outputs of the package (built = output dir exists)
ls -d /nix/store/*-<pkg> 2>/dev/null
```

Trace a prior output back to its derivation and source to inspect its
CMakeLists — see [Tracing a store path back to its source rev](#tracing-a-store-path-back-to-its-source-rev).

### Fix pattern

Change the dependency's CMake to probe the compiler with
`check_cxx_compiler_flag` instead of branching on `CMAKE_SYSTEM_PROCESSOR`, so
only flags the actual target compiler accepts are added. (See the CMake note in
the global AGENTS.md.) Then commit+push the dep fix and bump the consuming
flake's lock.

---

## Override patterns

### Local-path override (dev-only, no file changes)

Test against a local clone without modifying any flake.nix:

```bash
nix develop --override-input <input-name> path:/path/to/local/clone
nix build  --override-input <input-name> path:/path/to/local/clone
```

Use this during investigation before committing to a pin change.

### Temporary explicit pin (committed, with TODO marker)

When you need a pin change that isn't yet ready to be permanent:

```nix
# TODO(temp): bumped to <rev> for <reason>. Revert when <condition>.
<input>.url = "git+ssh://...?ref=refs/heads/master&rev=<rev>";
<input>.flake = false;
```

Update the lock after editing:
```bash
nix flake lock --update-input <input-name>
```

Remove the TODO and move to the permanent form once the upstream change lands.

### Overriding a nested input (`follows`)

When a dep (e.g. `my-nixpkgs`) uses its own internal pin for a package
and you need it to use yours instead:

```nix
# In your flake.nix inputs block:
my-nixpkgs.inputs.myapis.follows = "myapis";
```

This makes `my-nixpkgs`'s `myapis` resolve to your flake's
`myapis` input rather than `my-nixpkgs`'s own pin. Useful when
a downstream dep is built from an older source and you need a newer one.

The `follows` line must come immediately after the input declaration it applies to.
It only works when both inputs are present in the same flake.

---

## Tracing a store path back to its source rev

When you have a store path and need to know which source it was built from:

```bash
# Find the derivation that produced a store path
nix-store --query --deriver /nix/store/<hash>-<pkg>

# Find the source inputs of a derivation
nix-store --query --references /nix/store/<hash>-<pkg>.drv | grep source

# Inspect the source directory for a file
ls /nix/store/<hash>-source/path/to/check/
```

---

## Deciding whether a PR is needed

| Situation | Action |
|---|---|
| Lock is stale; fix is already on origin/master | `nix flake lock --update-input <name>` — no PR |
| Fix is on a branch, not yet merged | Create a minimal PR to master, then update pin |
| Fix is on a long-lived team branch (intentional) | Pin to that branch explicitly; document why |
| Nested dep uses wrong source via its own pin | Use `follows` override; document it's permanent if the branches diverge by design |
