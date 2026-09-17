import subprocess
import sys
import unittest

import calc


class CalculatorTests(unittest.TestCase):
    def test_core_operations(self):
        self.assertEqual(calc.calculate("add", 2, 3), 5)
        self.assertEqual(calc.calculate("subtract", 8, 3), 5)
        self.assertEqual(calc.calculate("multiply", 4, 5), 20)
        self.assertEqual(calc.calculate("divide", 10, 2), 5)

    def test_division_by_zero_is_rejected(self):
        with self.assertRaises(ValueError):
            calc.calculate("divide", 10, 0)

    def test_cli_outputs_script_friendly_result(self):
        result = subprocess.run(
            [sys.executable, "calc.py", "add", "2", "3"],
            check=True,
            capture_output=True,
            text=True,
        )

        self.assertEqual(result.stdout.strip(), "5")

    def test_cli_rejects_invalid_input(self):
        result = subprocess.run(
            [sys.executable, "calc.py", "divide", "10", "0"],
            capture_output=True,
            text=True,
        )

        self.assertNotEqual(result.returncode, 0)
        self.assertIn("division by zero", result.stderr)


if __name__ == "__main__":
    unittest.main()
