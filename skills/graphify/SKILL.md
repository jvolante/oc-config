---
description: >
  ALWAYS load this skill before using Glob, Grep, or Bash to answer any
  architectural question: module structure, data flow, dependencies,
  cross-cutting concerns, or navigating an unfamiliar codebase. You MUST
  load this skill before searching files. Also load for shortest-path queries
  between concepts or to explain a node. The graph is orders of magnitude
  cheaper than file search — skipping it wastes tokens unnecessarily.
name: graphify
---

# /graphify

Query and navigate a pre-built knowledge graph for a codebase or corpus.

## Usage

```
/graphify query "<question>"                          # BFS traversal - broad context
/graphify query "<question>" --dfs                    # DFS - trace a specific path
/graphify query "<question>" --budget 1500            # cap answer at N tokens
/graphify path "ConceptA" "ConceptB"                  # shortest path between two concepts
/graphify explain "NodeName"                          # plain-language explanation of a node
```

To **build or update** a graph, dispatch the `graph-builder` subagent with the project path instead of using this skill.

## Routing

- `/graphify query` → follow the **For /graphify query** section below
- `/graphify path` → follow the **For /graphify path** section below
- `/graphify explain` → follow the **For /graphify explain** section below
- Any other invocation (build intent, path with no subcommand) → dispatch the `graph-builder` subagent with the given path

In all query sections below, run commands from the project root directory (the directory containing `graphify-out/`). Use `graphify-smart` rather than `graphify` directly — it handles incremental staleness checks automatically and provides native `path` and `explain` subcommands without requiring Python.

All commands accept an optional `--graph <path/to/graph.json>` flag to point at a graph that is not in the current directory tree.

---

## For /graphify query

Two traversal modes - choose based on the question:

| Mode | Flag | Best for |
|------|------|----------|
| BFS (default) | _(none)_ | "What is X connected to?" - broad context, nearest neighbors first |
| DFS | `--dfs` | "How does X reach Y?" - trace a specific chain or dependency path |

First check the graph exists:

```bash
test -f graphify-out/graph.json || { printf 'ERROR: No graph found.\n' >&2; exit 1; }
```

If it fails, stop and dispatch the `graph-builder` subagent with the current directory path to build one.

```bash
graphify-smart query "QUESTION" [--dfs] [--budget N] [--graph path/to/graph.json]
```

Replace `QUESTION` with the user's actual question. Add `--dfs` for chain-tracing questions, `--budget N` to cap output tokens (default 2000).

Output lines have two forms — filter them as needed:

```
NODE label [src=file.cpp loc=L42 community=3]
EDGE LabelA --relation [CONFIDENCE]--> LabelB
```

Useful filters:

```bash
# Only edges
graphify-smart query "QUESTION" | grep '^EDGE'

# Only nodes
graphify-smart query "QUESTION" | grep '^NODE'

# Filter edges by relation type (e.g. calls, inherits, contains)
graphify-smart query "QUESTION" | grep -- '--calls'

# Extract unique source files cited by the result
graphify-smart query "QUESTION" | sed -n 's/^NODE .* \[src=\([^ ]*\) .*/\1/p' | sort -u
```

Read the output — node labels, edge relations, confidence tags, source locations — then answer using **only** what the graph contains. Quote `src=` locations when citing a specific fact. If the graph lacks enough information, say so — do not hallucinate edges.

After writing the answer, save it back into the graph so it improves future queries:

```bash
graphify save-result --question "QUESTION" --answer "ANSWER" --type query --nodes NODE1 NODE2
```

Replace `QUESTION` with the question, `ANSWER` with your full answer text, `NODE1 NODE2` with the list of node labels you cited.

---

## For /graphify path

Find the shortest path between two named concepts in the graph.

First check the graph exists:

```bash
test -f graphify-out/graph.json || { printf 'ERROR: No graph found.\n' >&2; exit 1; }
```

If it fails, dispatch the `graph-builder` subagent with the current directory path to build one.

```bash
graphify-smart path "NODE_A" "NODE_B" [--graph path/to/graph.json]
```

Replace `NODE_A` and `NODE_B` with the actual concept names from the user.

Example output:

```
Shortest path (2 hops):
  _collect_concepts_from_body() --contains--> [EXTRACTED]
  gen_tracker_types.py --contains--> [EXTRACTED]
  _field_line()
```

Then explain the path in plain language — what each hop means, why it's significant.

After writing the explanation, save it back:

```bash
graphify save-result --question "Path from NODE_A to NODE_B" --answer "ANSWER" --type path_query --nodes NODE_A NODE_B
```

---

## For /graphify explain

Give a plain-language explanation of a single node — everything directly connected to it.

First check the graph exists:

```bash
test -f graphify-out/graph.json || { printf 'ERROR: No graph found.\n' >&2; exit 1; }
```

If it fails, dispatch the `graph-builder` subagent with the current directory path to build one.

```bash
graphify-smart explain "NODE_NAME" [--graph path/to/graph.json]
```

Replace `NODE_NAME` with the concept the user asked about.

Example output:

```
NODE: CopiAlgoRecipe
  source: conanfile.py
  type:   code
  loc:    L6

CONNECTIONS:
  <--contains-- conanfile.py [EXTRACTED] (conanfile.py)
  --inherits--> ConanFile [EXTRACTED] (conanfile.py)
  --method--> .configure() [EXTRACTED] (conanfile.py)
```

Outgoing edges are shown as `-->`, incoming as `<--`. Then write a 3-5 sentence explanation of what this node is, what it connects to, and why those connections are significant. Use the source locations as citations.

After writing the explanation, save it back:

```bash
graphify save-result --question "Explain NODE_NAME" --answer "ANSWER" --type explain --nodes NODE_NAME
```

---

## Raw jq recipes

For surgical queries the subcommands don't cover, run `jq` directly against `graphify-out/graph.json`.

**List all relation types present in this graph** — useful before querying so you know what edge vocabulary exists:

```bash
jq -r '[.links[].relation] | unique[]' graphify-out/graph.json
```

**All nodes extracted from a specific source file:**

```bash
jq -r --arg f "src/foo.cpp" \
  '.nodes[] | select(.source_file == $f) | .label' \
  graphify-out/graph.json
```

**All edges of a given relation type, with labels resolved** (ids are internal; labels are human-readable):

```bash
jq -r --arg rel "inherits" '
  (reduce .nodes[] as $n ({}; .[$n.id] = $n.label)) as $l |
  .links[] | select(.relation == $rel) |
  "\($l[.source]) --\(.relation)--> \($l[.target])"
' graphify-out/graph.json
```

**What module does node X belong to? List all nodes in the same community:**
Communities are the graph's clustering of related nodes — roughly equivalent to module or subsystem boundaries.

```bash
jq -r --arg label "MyClass" '
  (.nodes[] | select(.label == $label) | .community) as $c |
  .nodes[] | select(.community == $c) | "\(.label) (\(.source_file))"
' graphify-out/graph.json
```
