"""Run independent guest checks side by side, each worker with its own Wine prefix.

Guests launched together in one prefix hang or fail to create a device, so
every worker gets a copy.
"""
from concurrent.futures import ThreadPoolExecutor
import copy
from pathlib import Path
from queue import Queue
import shutil

# Caches a copy does without: they are large and each prefix fills its own.
CACHES = ("shadercache", "tuxbox_shader_cache")


def moved(env, old, new):
    """`env` with every value that names the prefix `old` naming `new` instead."""
    return {key: value.replace(old, new) for key, value in env.items()}


class PrefixPool:
    def __init__(self, directory, jobs):
        self.directory, self.jobs, self.free = Path(directory), jobs, None

    def run(self, tasks):
        """The results, in order, of `(host, function)` pairs, calling `function(host)`.

        The first task of a pool runs alone in the title's own prefix, which
        creates the prefix when it is new, before it is copied.
        """
        tasks = list(tasks)
        results = []
        if tasks and self.free is None:
            host, function = tasks.pop(0)
            results.append(function(host))
            source = host.env["WINEPREFIX"]
            self.free = Queue()
            for index in range(self.jobs):
                prefix = self.directory / f"prefix{index}"
                shutil.copytree(source, prefix, symlinks=True, ignore=shutil.ignore_patterns(*CACHES))
                self.free.put((source, str(prefix)))

        def isolated(task):
            host, function = task
            source, prefix = self.free.get()
            try:
                own = copy.copy(host)
                own.env = moved(host.env, source, prefix)
                return function(own)
            finally:
                self.free.put((source, prefix))

        with ThreadPoolExecutor(self.jobs) as pool:
            results.extend(pool.map(isolated, tasks))
        return results
