from pathlib import Path
from tempfile import TemporaryDirectory
from threading import Barrier
from types import SimpleNamespace
import unittest

from project_tuxbox.prefix_pool import PrefixPool, moved


class PrefixPools(unittest.TestCase):
    def test_every_value_naming_the_prefix_follows_it(self):
        env = {"WINEPREFIX": "/a/p", "CACHE": "/a/p/shadercache", "OTHER": "/a/q"}
        self.assertEqual(moved(env, "/a/p", "/w/prefix0"),
                         {"WINEPREFIX": "/w/prefix0", "CACHE": "/w/prefix0/shadercache", "OTHER": "/a/q"})

    def test_first_task_uses_the_title_prefix_and_the_rest_use_separate_copies(self):
        with TemporaryDirectory() as temp:
            root = Path(temp)
            source = root / "title"
            (source / "shadercache").mkdir(parents=True)
            (source / "user.reg").write_text("registry")
            (root / "work").mkdir()
            host = SimpleNamespace(env={"WINEPREFIX": str(source)})
            together = Barrier(2, timeout=10)

            def task(number):
                def run(own):
                    if number:
                        together.wait()  # two tasks hold prefixes at once
                    prefix = Path(own.env["WINEPREFIX"])
                    return number, prefix.name, (prefix / "user.reg").read_text(), (prefix / "shadercache").exists()
                return host, run

            pool = PrefixPool(root / "work", 2)
            results = pool.run(task(number) for number in range(3))
            self.assertEqual([row[0] for row in results], [0, 1, 2])
            self.assertEqual(results[0][1:], ("title", "registry", True))
            self.assertEqual(sorted(row[1] for row in results[1:]), ["prefix0", "prefix1"])
            self.assertTrue(all(row[2:] == ("registry", False) for row in results[1:]))
            self.assertEqual(host.env, {"WINEPREFIX": str(source)})


if __name__ == "__main__":
    unittest.main()
