# VIctory-Belobug-AMD GitHub export

Issues, pull requests and comments from `Belobug/VIctory-Belobug-AMD`, exported from the GitHub REST API on 2026-11-27.

Each file in `data/` is gzipped JSON Lines: one API object per line, unchanged.

| File | Contents | Items |
|---|---|---|
| `issues_and_prs.jsonl.gz` | every issue and PR (title, body, labels, state, author) | 273 |
| `pulls.jsonl.gz` | PR details (head and base branch, merge state) | 326 |
| `issue_comments.jsonl.gz` | conversation comments on issues and PRs | 780 |
| `review_comments.jsonl.gz` | inline code-review comments | 59 |
| `labels.jsonl.gz` | labels | 10 |

Not included: PR approval reviews and images attached to comments (only their links).

`export.py` made these files (`python -I export.py`, needs the `gh` CLI signed in).
