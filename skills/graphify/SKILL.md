---
name: graphify
description: Use for architectural questions, codebase navigation, dependency tracing, shortest paths, node explanations, and reverse-impact queries when a Graphify knowledge graph may contain the answer. This skill only searches and queries graphs; graph creation and updates belong to the graph-builder agent.
---

# Graphify Queries

Use Graphify as a read-only navigation layer over an existing `graphify-out/graph.json`.

## Scope

- Use this skill for graph queries, shortest paths, node explanations, and affected-node traversal.
- Do not build, update, cluster, label, or otherwise mutate a graph from this skill.
- For a missing or stale graph, dispatch the `graph-builder` agent with the project path.
- For cross-repository queries, use the target repository's graph explicitly.
- Treat graph evidence as authoritative only for the nodes and edges returned. Do not invent missing relationships.

## Commands

Run from the project root containing `graphify-out/graph.json`:

```bash
graphify-smart query "QUESTION" [--dfs] [--budget N] [--graph PATH]
graphify-smart path "NODE_A" "NODE_B" [--graph PATH]
graphify-smart explain "NODE_NAME" [--graph PATH]
graphify affected "NODE_NAME" [--relation RELATION] [--depth N] [--graph PATH]
```

`graphify-smart` is preferred for `query`, `path`, and `explain` because it performs its configured staleness check and supports native path/explain handling. Use the installed `graphify` binary for `affected` and `save-result`.

Before querying, check that the graph exists:

```bash
test -f graphify-out/graph.json || {
  printf 'ERROR: no Graphify graph found.\n' >&2
  exit 1
}
```

If the check fails, stop and dispatch `graph-builder`; do not run an extraction command yourself.

## Query Selection

- Use BFS for “what is connected to this?” and broad neighborhood questions.
- Use DFS for a specific dependency or call chain.
- Use `path` for the shortest relationship chain between two named nodes.
- Use `explain` for a node and all direct incoming and outgoing connections.
- Use `affected` for reverse dependencies and likely impact analysis. Add `--relation` when only calls, imports, references, or another relation matters.

## Evidence

Query output contains `NODE` and `EDGE` records with labels, relations, confidence, source files, and locations. Cite the returned `src`, `loc`, or source-file metadata when making a codebase claim.

If a result is empty or the graph lacks the required relationship, say so and fall back to ordinary source inspection only when appropriate. A graph query is not a substitute for reading the source when correctness depends on implementation details.

## Cross-Repository Queries

When the current directory is not the target repository, always provide the graph path:

```bash
graphify-smart query "QUESTION" \
  --graph /path/to/repository/graphify-out/graph.json
```

## Surgical Inspection

For questions that need exact graph data, use `jq` without modifying the graph:

```bash
jq -r '[.links[].relation] | unique[]' graphify-out/graph.json
jq -r --arg file "src/foo.cpp" \
  '.nodes[] | select(.source_file == $file) | .label' \
  graphify-out/graph.json
```

Current Graphify JSON uses NetworkX node-link data, so relationships are normally under `.links`. If an older graph uses `.edges`, inspect that key instead.

## Query Memory

After answering a query, persist the answer only when it is grounded in graph nodes you can name:

```bash
graphify save-result \
  --question "QUESTION" \
  --answer "ANSWER" \
  --type query \
  --nodes NODE1 NODE2
```

Use `--type path_query` for a path explanation and `--type explain` for a node explanation.
