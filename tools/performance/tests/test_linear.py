import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('linear', Path(__file__).parents[1]/'linear.py')
linear = importlib.util.module_from_spec(spec)
spec.loader.exec_module(linear)


class LinearTests(unittest.TestCase):
    def test_truncate_each_channel_before_sum_and_preserve_saturation_order(self):
        self.assertEqual(linear.score([1, 1, 0, 0, 0], [50]*5), 0)
        self.assertEqual(linear.score([-3, 0, 0, 0, 0], [50]*5), -1)
        self.assertEqual(linear.score([linear.HIGH, 0, 0, 0, 0], [1000]*5), linear.HIGH//100)
        self.assertEqual(linear.score([0, 0, 0, linear.LOW, 0], [1]*5), abs(linear.LOW)//100)

    def test_trait_weight_compilation_retains_definition_order_and_clamps(self):
        traits = {'definitions': {
            'a': {'default': 500, 'weights': {'need': -1000}},
            'b': {'default': 500, 'weights': {'need': 1}},
        }, 'people': {'1': {'b': 700, 'a': 700}}}
        self.assertEqual(linear.weights(traits, '1'), [2, 100, 100, 100, 100])
        self.assertEqual(linear.weights(traits, 'missing'), [100]*5)


if __name__ == '__main__':
    unittest.main()
