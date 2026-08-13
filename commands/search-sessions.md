---
description: Semantically search past sessions by description — finds the best match using LLM judgment over titles and message snippets
---

Find the session that best matches the description in `$ARGUMENTS`. If no arguments are
given, ask the user what they're looking for.

## Step 1 — Pull candidate sessions

Fetch the 200 most recent top-level sessions (no subagents):

```bash
opencode db "SELECT id, title, directory, time_updated FROM session WHERE parent_id IS NULL ORDER BY time_updated DESC LIMIT 200" --format json | jq -r '.[] | [(.time_updated / 1000 | todate), .id, .directory, .title] | @tsv'
```

If the user mentioned a specific project directory (e.g. "in kestrel"), filter it:

```bash
opencode db "SELECT id, title, directory, time_updated FROM session WHERE parent_id IS NULL AND directory LIKE '%kestrel%' ORDER BY time_updated DESC LIMIT 200" --format json | jq -r '.[] | [(.time_updated / 1000 | todate), .id, .directory, .title] | @tsv'
```

## Step 2 — Semantic shortlist

Read the full list of titles and use your judgment to pick the 5 most plausible
matches for the user's description. Titles are often terse or auto-generated — look
for conceptual overlap, not just keyword matches. Consider synonyms, related concepts,
and workflow context (e.g. "fixed the tracker" might match "Tracker changes test plan"
or "TrackBeforeDetect migration").

## Step 3 — Fetch opening messages for shortlisted sessions

For each candidate, fetch the first user message to confirm context:

```bash
opencode db "SELECT substr(p.data, 1, 500) FROM part p JOIN message m ON p.message_id = m.id WHERE m.session_id = '<id>' AND p.data NOT LIKE '%\"synthetic\":true%' ORDER BY m.time_created ASC LIMIT 1" --format json | jq -r '.[][]'
```

Extract the `text` field from the JSON blob. This reveals what the user actually asked
in that session, which is far more informative than the auto-generated title.

## Step 4 — Present results

Rank the candidates by relevance and show:
- **Title** and date
- **Project directory**
- **Opening message** snippet (1–2 sentences)
- **Resume**: `opencode --session <id>`

Highlight the single best match at the top. If multiple sessions are plausible, list
them all and ask the user to confirm.
