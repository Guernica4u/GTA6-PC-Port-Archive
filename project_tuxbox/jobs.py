"""How many isolated GPU checks run at once."""
import os


def check_jobs(environ=os.environ, cpus=None):
    """`TUXBOX_CHECK_JOBS`, else half the processor threads, at most eight.

    Each check runs its own Wine server and compiles its own shaders, so one per
    thread starves them into their time limits. `1` runs them one at a time.
    """
    value = environ.get("TUXBOX_CHECK_JOBS")
    if value:
        jobs = int(value)
        if jobs < 1:
            raise ValueError("TUXBOX_CHECK_JOBS must be 1 or more")
        return jobs
    return max(1, min(8, (cpus or os.cpu_count() or 2) // 2))
