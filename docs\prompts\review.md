# Prompt: review a pull request in a fresh task

Start a **new** task (never a follow-up in the author's task): **Work in > Cloud**, the Histeries environment, the **pull-request branch** in the branch selector, and a model **different from the author's** (Astra reviews Sol's work; Sol reviews Astra's). Fill in the brackets and paste.

```text
Review task for Histeries: branch [BRANCH NAME], task [ROADMAP ID AND TITLE], author model [gpt-6-astra | gpt-6.1-sol].
You are the reviewer. Follow AGENTS.md and the review-change skill (.agents/skills/review-change/SKILL.md) exactly. Read-only: change no files and push nothing. Never run git; use jj for inspection only.
Context to read: spec [specs/FILE.md section], ADRs [LIST], and the diff against main@origin.
Begin your answer with VERDICT: APPROVE or VERDICT: REQUEST CHANGES.
```

Paste the verdict into the pull request. REQUEST CHANGES blocks merge: send the findings back as a new work task (`session.md`), then review again in a fresh task.
