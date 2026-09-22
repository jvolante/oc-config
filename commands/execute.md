---
description: execute the current plan, good for simple plans
agent: build
---

Plan looks good, break the plan into steps and dispatch @software-engineer for each step. At the end of each step make a "wip: ..." commit. Once all steps are completed dispatch @code-reviewer to review the changes for code quality and implementation correctness, then dispatch another @software-engineer to make any needed corrections. The final commit should have the appropriate conventional commit type.
