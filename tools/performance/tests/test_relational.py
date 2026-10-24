import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location('relational', Path(__file__).parents[1]/'relational.py')
relational = importlib.util.module_from_spec(spec)
spec.loader.exec_module(relational)


class RelationalTests(unittest.TestCase):
    def test_aliases_stale_beliefs_and_absence_use_no_actual_cargo(self):
        facts = {
            'store:07': {'9': 100},  # Alias uses canonical evidence, not this payload.
            'store:7': {'9': 2},
            'store-location:7': {'area': 3, 'carrier': None, 'capacity': 100, 'mass': 2},
            'store:8': {'absent_from': 3},
            'store-location:8': {'area': 3, 'capacity': 100, 'mass': 0},
            'store:9': {'9': 2, 'bad': 1},  # Invalid typed stock is wholly rejected.
            'store-location:9': {'area': 3, 'capacity': 100, 'mass': 2},
            'store:10': {'9': 1},
            'store-location:10': {'area': 4, 'carrier': 1, 'capacity': 100, 'mass': 1},
        }
        world = {'resources': {
            'knowledge.facts': {'archive': {
                'keys': list(facts), 'values': list(facts.values()),
                'people': {'1': [[i, i, 1, 0, 0, 100] for i in range(len(facts))]}}},
            'physiology.bodies': {'people': {'1': {'alive': True}}},
            'transport.positions': {'1': {'At': 3}},
            'cargo.inventories': {'7': {'goods': {'9': 999}}},
        }}
        stores, stocks, actors = relational.project(world)
        self.assertEqual(actors, [(1, 3)])
        self.assertEqual(stocks, [(1, 7, 9, 2), (1, 10, 9, 1)])
        queries = [dict(person=1, area=3, good=9, quantity=1),
                   dict(person=1, area=3, good=9, quantity=3)]
        db, _ = relational.build(stores, stocks)
        self.addCleanup(db.close)
        relational.batch(db, queries)
        expected = [(7, 10), ()]
        self.assertEqual(relational.reference(stores, stocks, queries), expected)
        for name in relational.PLANS:
            self.assertEqual(relational.evaluate(db, name, queries), expected)

    def test_incremental_update_and_rollback_match_all_plans(self):
        stores = [(2, 7, 3, None), (1, 7, 3, None)]
        stocks = [(1, 7, 9, 2), (2, 7, 9, 5)]
        queries = [dict(person=p, area=3, good=9, quantity=1) for p in [1, 2]]
        db, _ = relational.build(stores, stocks)
        self.addCleanup(db.close)
        relational.batch(db, queries)
        db.execute('SAVEPOINT probe')
        relational.update(db, stocks[0], 0)
        for name in relational.PLANS:
            self.assertEqual(relational.evaluate(db, name, queries), [(), (7,)])
        db.execute('ROLLBACK TO probe')
        for name in relational.PLANS:
            self.assertEqual(relational.evaluate(db, name, queries), [(7,), (7,)])


if __name__ == '__main__':
    unittest.main()
