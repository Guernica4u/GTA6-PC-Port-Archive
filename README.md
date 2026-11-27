# hyphenn GitHub export

Issues, pull requests and comments from `fxceless/victory-fork` (called `victory-gta6` until 24 Nov 2026; archived here as hyphenn), exported from the GitHub REST API on 2026-11-27.

Each file in `data/` is gzipped JSON Lines: one API object per line, unchanged.

| File | Contents | Items |
|---|---|---|
| `issues_and_prs.jsonl.gz` | every issue and PR (title, body, labels, state, author) | 318 |
| `pulls.jsonl.gz` | PR details (head and base branch, merge state) | 1,131 |
| `issue_comments.jsonl.gz` | conversation comments on issues and PRs | 3,068 |
| `review_comments.jsonl.gz` | inline code-review comments | 68 |
| `labels.jsonl.gz` | labels | 10 |

Not included: PR approval reviews and images attached to comments (only their links).

`export.py` made these files (`python -I export.py`, needs the `gh` CLI signed in).
