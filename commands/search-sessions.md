---
description: Search past OpenCode sessions by description, keywords, content, date, and project
---

Find the sessions that best match `$ARGUMENTS`. If no arguments are given, ask the user
what they're looking for. Use both semantic reasoning and database search; titles alone
are often too terse.

## Step 1 — Pull candidate sessions

First extract useful literal terms and constraints from the request: distinctive nouns,
synonyms, project names, and date ranges such as "July" or "a few weeks ago". Search
all sessions, including subagents, because research conversations are often split across
the parent and short-lived explore/general sessions.

Fetch a broad title/date/project candidate list:

```bash
opencode db "SELECT id, title, directory, parent_id, time_created, time_updated FROM session ORDER BY time_updated DESC LIMIT 2000" --format json | jq -r '.[] | [(.time_created / 1000 | todate), (.time_updated / 1000 | todate), .id, .parent_id, .directory, .title] | @tsv'
```

If the user mentioned a specific project directory (e.g. "in kestrel"), filter it:

```bash
opencode db "SELECT id, title, directory, parent_id, time_created, time_updated FROM session WHERE directory LIKE '%kestrel%' ORDER BY time_updated DESC LIMIT 2000" --format json | jq -r '.[] | [(.time_created / 1000 | todate), (.time_updated / 1000 | todate), .id, .parent_id, .directory, .title] | @tsv'
```

For distinctive terms, search the actual conversation content in `part.data`, not just
titles. Use one `LIKE` clause per term and aggregate by session so repeated mentions rank
higher. Keep the query read-only and use the database's read-only command:

```bash
opencode db "SELECT s.id, s.title, s.directory, s.parent_id, s.time_created,
  COUNT(*) AS matching_parts
  FROM part p JOIN session s ON s.id = p.session_id
  WHERE (lower(p.data) LIKE '%wire%' OR lower(p.data) LIKE '%powerline%'
         OR lower(p.data) LIKE '%ranging%' OR lower(p.data) LIKE '%depth%')
    AND s.time_created >= <start_ms> AND s.time_created < <end_ms>
  GROUP BY s.id ORDER BY matching_parts DESC, s.time_created DESC" --format json
```

Replace the terms and date bounds with the user's request. Omit the date predicate when
no date is stated. For a broad search, use the distinctive terms rather than common words
like `the`, `code`, or `test`. Run separate focused queries when there are two themes, then
look for sessions matching both themes.

## Step 2 — Semantic shortlist

Combine the title list with content-match results and use your judgment to pick the 5 most
plausible matches. Rank exact distinctive-term matches above incidental mentions. Consider
synonyms, related concepts, and workflow context (e.g. "wire detection and dense video
depth" may match separate wire/ranging and monocular-depth sessions from the same week).

## Step 3 — Fetch opening messages for shortlisted sessions

For each candidate, fetch early text parts to confirm context. The current database stores
conversation parts directly by `session_id`; do not assume the old `message` table has rows:

```bash
opencode db "SELECT substr(data, 1, 1200) FROM part WHERE session_id = '<id>' AND data NOT LIKE '%\"synthetic\":true%' ORDER BY time_created ASC LIMIT 3" --format json | jq -r '.[][]'
```

Extract the `text` field from each JSON blob. Search within the returned text for the
distinctive terms and show the shortest useful snippets. This reveals what the user
actually asked, which is more informative than the auto-generated title.

## Step 4 — Present results

Rank the candidates by relevance and show:
- **Title** and date
- **Project directory**
- **Opening message** snippet (1–2 sentences)
- **Why it matches**: the distinctive terms or topic overlap
- **Resume**: `opencode --session <id>`

Highlight the single best match at the top. If multiple sessions are plausible, list
them all and ask the user to confirm.
