# Leonida Continued GitHub export

Issues, pull requests and comments from `ubiquitarianism/leonida`, exported from the GitHub REST API on 2026-11-28.

Each file in `data/` is gzipped JSON Lines: one API object per line, unchanged.

| File | Contents | Items |
|---|---|---|
| `issues_and_prs.jsonl.gz` | every issue and PR (title, body, labels, state, author) | 607 |
| `pulls.jsonl.gz` | PR details (head and base branch, merge state) | 2,503 |
| `issue_comments.jsonl.gz` | conversation comments on issues and PRs | 9,894 |
| `review_comments.jsonl.gz` | inline code-review comments | 128 |
| `labels.jsonl.gz` | labels | 12 |

Not included: PR approval reviews and images attached to comments (only their links).

`export.py` made these files (`python -I export.py`, needs the `gh` CLI signed in).
