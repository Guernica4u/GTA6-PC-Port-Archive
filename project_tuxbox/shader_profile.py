"""Opt-in aggregate timing of the host shader pipeline; never store title identifiers."""
from collections import Counter
from contextvars import ContextVar
import json
import subprocess
from threading import Lock
from time import monotonic_ns


_active = ContextVar("shader_profile", default=None)


class ShaderProfile:
    def __init__(self):
        self._start = monotonic_ns()
        self._lock = Lock()
        self._requests = Counter()
        self._stages = {}
        self._reuse = Counter()

    def record(self, stage, elapsed, outcome):
        with self._lock:
            stats = self._stages.setdefault(stage, [0, 0, Counter()])
            stats[0] += elapsed
            stats[1] = max(stats[1], elapsed)
            stats[2][outcome] += 1

    def request(self, function, refusal_type, *args, **kwargs):
        token = _active.set(self)
        started = monotonic_ns()
        outcome = "failed"
        try:
            result = function(*args, **kwargs)
            outcome = "compiled" if result else "cached"
            return result
        except refusal_type as error:
            outcome = "cached_refusal" if error.cached else "refused"
            raise
        finally:
            _active.reset(token)
            elapsed = monotonic_ns() - started
            with self._lock:
                self._requests[outcome] += 1
            self.record("request_total", elapsed, outcome)

    def reused(self, kind):
        with self._lock:
            self._reuse[kind] += 1

    def summary(self):
        with self._lock:
            return {
                "schema": 1,
                "elapsed_ms": round((monotonic_ns() - self._start) / 1e6, 3),
                "requests": dict(self._requests),
                "reuse": dict(self._reuse),
                "stages": {
                    name: {
                        "calls": sum(counts.values()),
                        "total_ms": round(total / 1e6, 3),
                        "max_ms": round(maximum / 1e6, 3),
                        "outcomes": dict(counts),
                    }
                    for name, (total, maximum, counts) in sorted(self._stages.items())
                },
            }

    def report(self, label, emit):
        emit(f"Shader stage timings ({label}): " +
             json.dumps(self.summary(), sort_keys=True, separators=(",", ":")))


def timed(stage, function, *args, **kwargs):
    profile = _active.get()
    if profile is None:
        return function(*args, **kwargs)
    started = monotonic_ns()
    outcome = "error"
    try:
        result = function(*args, **kwargs)
        outcome = "ok"
        return result
    except (TimeoutError, subprocess.TimeoutExpired):
        outcome = "timeout"
        raise
    finally:
        profile.record(stage, monotonic_ns() - started, outcome)


def count_reuse(kind):
    profile = _active.get()
    if profile is not None:
        profile.reused(kind)
