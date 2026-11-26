"""Bounded aggregate statistics across multiple simultaneous shader requests."""
from concurrent.futures import ThreadPoolExecutor
import unittest

from project_tuxbox.runtime_shaders import ShaderRefused
from project_tuxbox.shader_profile import ShaderProfile, timed, count_reuse


class ProfileTests(unittest.TestCase):
    def test_concurrent_requests_have_independent_profiles(self):
        first, second = ShaderProfile(), ShaderProfile()

        def run():
            timed("xshader", lambda: None)
            count_reuse("content")
            return True

        with ThreadPoolExecutor(max_workers=8) as pool:
            jobs = [pool.submit(profile.request, run, ShaderRefused)
                    for profile in (first, second) for _ in range(100)]
            self.assertTrue(all(job.result() for job in jobs))
        # Neither an unprofiled request nor a previous context leaks into these.
        run()
        for profile in (first, second):
            result = profile.summary()
            self.assertEqual(result["requests"], {"compiled": 100})
            self.assertEqual(result["reuse"], {"content": 100})
            self.assertEqual(result["stages"]["xshader"]["calls"], 100)
            self.assertEqual(result["stages"]["request_total"]["calls"], 100)
            self.assertGreaterEqual(result["stages"]["request_total"]["total_ms"],
                                    result["stages"]["xshader"]["total_ms"])
