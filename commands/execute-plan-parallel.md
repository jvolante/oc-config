---
description: Split current plan into parallel stories and dispatch software-engineer agents
agent: build
---

Split the plan into stories that can be implemented independently and delegate @software-engineer for each, working in parallel when feasible. After each batch of work, dispatch @code-reviewer to ensure code quality, correctness, and make sure the implementation aligns with technical and architectural requirements.

Wait until review of a batch is done before dispatching the next batch, flow should be: impl batch -> review batch -> correct issues batch -> repeat flow for next batch

You can batch agents within a step, but each step needs to complete before the next starts.
