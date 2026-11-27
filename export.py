"""Export 4lwayz/portwatch-vi issues, PRs and comments to gzipped JSONL via the gh CLI.
Resumable: a finished endpoint is skipped; pauses for the rate-limit reset when low."""
import gzip, json, os, subprocess, sys, time

REPO = "4lwayz/portwatch-vi"
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "data")
ENDPOINTS = [
    ("issues_and_prs", f"repos/{REPO}/issues?state=all&sort=created&direction=asc"),
    ("pulls", f"repos/{REPO}/pulls?state=all&sort=created&direction=asc"),
    # windowed by `since`: this list stops at page 300 (30,000 items) per query
    ("issue_comments", f"repos/{REPO}/issues/comments?sort=updated&direction=asc"),
    ("review_comments", f"repos/{REPO}/pulls/comments?sort=created&direction=asc"),
    ("labels", f"repos/{REPO}/labels"),
    ("milestones", f"repos/{REPO}/milestones?state=all"),
    ("releases", f"repos/{REPO}/releases"),
]


def gh(path):
    for attempt in range(5):
        p = subprocess.run(["gh", "api", path], capture_output=True, text=True, encoding="utf-8")
        if p.returncode == 0:
            return json.loads(p.stdout)
        if "rate limit" in p.stderr.lower():
            wait_for_reset(force=True)
            continue
        time.sleep(5 * (attempt + 1))
    raise RuntimeError(f"gh api {path} failed: {p.stderr.strip()}")


def wait_for_reset(force=False):
    core = json.loads(subprocess.run(["gh", "api", "rate_limit"], capture_output=True, text=True).stdout)["resources"]["core"]
    if force or core["remaining"] < 100:
        delay = max(0, core["reset"] - time.time()) + 30
        print(f"rate limit: {core['remaining']} left, sleeping {int(delay)}s", flush=True)
        time.sleep(delay)


def export(name, path):
    done, part = os.path.join(OUT, name + ".done"), os.path.join(OUT, name + ".jsonl.gz.part")
    if os.path.exists(done):
        print(f"{name}: already done", flush=True)
        return
    windowed = name == "issue_comments"
    count, page, calls, since, seen = 0, 1, 0, None, set()
    with gzip.open(part, "wt", encoding="utf-8", newline="\n") as f:
        while True:
            calls += 1
            if calls % 50 == 0:
                wait_for_reset()
            q = f"{path}{'&' if '?' in path else '?'}per_page=100&page={page}" + (f"&since={since}" if since else "")
            items = gh(q)
            if not items:
                break
            for it in items:
                if it.get("id") in seen:
                    continue
                seen.add(it.get("id"))
                f.write(json.dumps(it, ensure_ascii=False) + "\n")
                count += 1
            if calls % 20 == 0:
                print(f"{name}: {calls} calls, {count} items", flush=True)
            if len(items) < 100:
                break
            if windowed and page == 300:
                # next window starts at the last updated_at; same-second items repeat and are skipped by id
                new_since = items[-1]["updated_at"]
                if new_since == since:
                    raise RuntimeError(f"{name}: over 30,000 items share updated_at {since}")
                since, page = new_since, 1
                continue
            page += 1
    os.replace(part, os.path.join(OUT, name + ".jsonl.gz"))
    with open(done, "w", encoding="utf-8", newline="\n") as d:
        d.write(str(count))
    print(f"{name}: {count} items", flush=True)


if __name__ == "__main__":
    os.makedirs(OUT, exist_ok=True)
    for name, path in ENDPOINTS:
        export(name, path)
    print("ALL DONE", flush=True)
    sys.exit(0)
