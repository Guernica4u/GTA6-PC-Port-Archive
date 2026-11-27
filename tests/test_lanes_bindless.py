"""The direct-indexed compute scenarios belong to the normal check."""
import unittest

from examples.lanes.shader_fixture import (bindless_compute_shader, bindless_sampler_shader,
                                           bindless_table_store_shader, bindless_texture_store_shader, header)
from project_tuxbox.verify import BASE_SUITES, GATE_LABELS


class BindlessCompute(unittest.TestCase):
    def test_suite_runs_in_the_normal_check_and_labels_every_gate(self):
        self.assertIn("lanes-bindless-compute", BASE_SUITES)
        for scenario in ("compute", "sampler", "table-compute", "heap-read", "texture-store", "table-store"):
            for kind in ("wide", "constrained"):
                self.assertIn(f"lanes-bindless-{scenario}-{kind}", GATE_LABELS)

    def test_guest_header_declares_both_programs(self):
        text = header()
        for name in ("bindless_compute_shader", "bindless_sampler_shader",
                     "bindless_table_compute_shader", "bindless_texture_store_shader", "bindless_table_store_shader"):
            self.assertIn(f"constexpr unsigned char {name}[]", text)
            self.assertIn(f"constexpr unsigned char {name}[]", text)
        self.assertNotEqual(bindless_compute_shader(), bindless_sampler_shader())
        self.assertNotEqual(bindless_compute_shader(), bindless_compute_shader(table=True))
        self.assertNotEqual(bindless_compute_shader(), bindless_texture_store_shader())


if __name__ == "__main__":
    unittest.main()
