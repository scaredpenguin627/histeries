# Prompt: every later work session

One task per session. Start a task with **Work in > Cloud**, the Histeries environment, branch `main`, and the model for the task's class (AGENTS.md section 11: A → `gpt-6-astra`; B and C → `gpt-6.1-sol`). Fill in the brackets and paste.

```text
Histeries session: [TASK ID AND TITLE from docs/ROADMAP.md, or "the next unchecked task in docs/ROADMAP.md"].
This is a fresh workspace in the published Histeries environment. Follow AGENTS.md section 0: run the session-start skill first; jj only, never git.
Spec: specs/[FILE].md[; new fixtures in fixtures/oracle/[PATH]]. Model for this task: [gpt-6-astra | gpt-6.1-sol] (class [A | B | C]).
Push is not enabled.
Finish with the session-finish skill: gates, docs, session report, closing `jj new`, and the final message it describes.
```

When it ends: read the final message, press the PR button, then start the review task (`docs/prompts/review.md`) with the other model. Merge only after APPROVE.
