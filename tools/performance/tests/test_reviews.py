import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('reviews', Path(__file__).parents[1]/'reviews.py')
reviews = importlib.util.module_from_spec(spec)
spec.loader.exec_module(reviews)


class ReviewTests(unittest.TestCase):
    def test_attempts_causes_and_equal_numeric_inputs_are_distinguished(self):
        wake = {'type': 'wake', 'suppressed': False, 'during_review': 9,
                'stack': [['event', 'fed'], ['listener', 'characters.fed']]}
        first = {'type': 'review', 'person': 1, 'at': 30, 'success': True,
                 'candidates': {'count': 3, 'numeric_hash': 2}, 'causes': [wake],
                 'before': {'chosen': 'care'}, 'after': {'chosen': 'care'}}
        deferred = {'type': 'review', 'person': 2, 'at': 30}
        result = reviews.summarize([wake, first, first, deferred])
        self.assertEqual(result['counts']['review_attempts'], 3)
        self.assertEqual(result['counts']['same_clock_repeated_reviews'], 1)
        self.assertEqual(result['counts']['same_numeric_candidates_and_observed_state'], 1)
        self.assertEqual(result['counts']['deferred_or_early_exit_attempts'], 1)
        self.assertEqual(result['review_causes'][0]['count'], 2)
        self.assertEqual(result['counts']['wakes_during_review'], 1)

    def test_empty_stream(self):
        self.assertEqual(reviews.summarize([])['people'], 0)
