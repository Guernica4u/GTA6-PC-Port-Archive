# Xbox2Windows GitHub export

Issues, pull requests and comments from `R0Y00/Xbox2Windows`, exported from the GitHub REST API on 2026-11-26.

Each file in `data/` is gzipped JSON Lines: one API object per line, unchanged.

| File | Contents | Items |
|---|---|---|
| `issues_and_prs.jsonl.gz` | every issue and PR (title, body, labels, state, author) | 1,452 |
| `pulls.jsonl.gz` | PR details (head and base branch, merge state) | 2,576 |
| `issue_comments.jsonl.gz` | conversation comments on issues and PRs | 6,651 |
| `review_comments.jsonl.gz` | inline code-review comments | 685 |
| `labels.jsonl.gz` | labels | 12 |
| `releases.jsonl.gz` | release notes (attached files are not archived) | 1 |

Not included: PR approval reviews, release attachments, and images attached to comments (only their links).

`export.py` made these files (`python -I export.py`, needs the `gh` CLI signed in).
