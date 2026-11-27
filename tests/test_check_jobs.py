import unittest

from project_tuxbox.jobs import check_jobs


class CheckJobs(unittest.TestCase):
    def test_default_is_half_the_threads_up_to_eight(self):
        self.assertEqual([check_jobs({}, cpus) for cpus in (1, 2, 8, 16, 64)], [1, 1, 4, 8, 8])

    def test_environment_overrides_the_default(self):
        self.assertEqual(check_jobs({"TUXBOX_CHECK_JOBS": "1"}, 64), 1)
        self.assertEqual(check_jobs({"TUXBOX_CHECK_JOBS": "12"}, 2), 12)
        with self.assertRaises(ValueError):
            check_jobs({"TUXBOX_CHECK_JOBS": "0"}, 8)


if __name__ == "__main__":
    unittest.main()
