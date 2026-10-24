#!/usr/bin/env python3
"""Compare query plans over real personal beliefs, outside the simulation.

Both paths use SQLite: this compares algorithms, not Python with Rust. Run only
after other benchmarks/builds finish. No authoritative state or rules are edited.
"""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import platform
import sqlite3
import statistics
import time


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def unsigned(value):
    return type(value) is int and 0 <= value < 2**64


def number_key(key):
    return key.isascii() and key.removeprefix('+').isdigit() and int(key) < 2**64


def project(world):
    """The stock/location join used by care::known_supplies, including aliases.

    Return only recorded evidence. Actual cargo contents are deliberately absent.
    SQLite's signed IDs restrict this experiment; production supports all u64 IDs.
    """
    resources = world['resources']
    archive = resources['knowledge.facts']['archive']
    keys, values = archive['keys'], archive['values']
    stores, stocks, actors = [], [], []
    bodies = resources['physiology.bodies']['people']
    for person, rows in archive['people'].items():
        person = int(person)
        if not 0 <= person < 2**63:
            raise ValueError('experiment requires signed 63-bit IDs')
        facts = {keys[row[0]]: values[row[1]] for row in rows}
        inventories = {int(k[6:]) for k in facts
                       if k.startswith('store:') and number_key(k[6:])}
        for inventory in sorted(inventories):
            location = facts.get(f'store-location:{inventory}')
            stock = facts.get(f'store:{inventory}')
            if (not isinstance(location, dict)
                    or any(not unsigned(location.get(k)) for k in ['area', 'capacity', 'mass'])
                    or (location.get('carrier') is not None and not unsigned(location['carrier']))
                    or not isinstance(stock, dict) or 'absent_from' in stock):
                continue
            # Mirrors typed Stock deserialization rather than accepting partial rows.
            if any(not number_key(k) or not unsigned(q) for k, q in stock.items()):
                continue
            area, carrier = location['area'], location.get('carrier')
            if any(x is not None and not 0 <= x < 2**63
                   for x in [inventory, area, carrier]):
                raise ValueError('experiment requires signed 63-bit IDs')
            stores.append((person, inventory, area, carrier))
            # Typed numeric maps collapse noncanonical JSON number aliases.
            stock = {int(k): v for k, v in sorted(stock.items())}
            for good, quantity in sorted(stock.items()):
                if int(good) >= 2**63 or quantity >= 2**63:
                    raise ValueError('experiment requires signed 63-bit stocks')
                stocks.append((person, inventory, good, quantity))
        position = resources['transport.positions'].get(str(person), {})
        if bodies.get(str(person), {}).get('alive') and 'At' in position:
            actors.append((person, position['At']))
    return stores, stocks, actors


SCHEMA = '''
CREATE TABLE stores(person INTEGER, inventory INTEGER, area INTEGER, carrier INTEGER,
                    PRIMARY KEY(person, inventory)) WITHOUT ROWID;
CREATE TABLE stocks(person INTEGER, inventory INTEGER, good INTEGER, quantity INTEGER,
                    PRIMARY KEY(person, inventory, good)) WITHOUT ROWID;
CREATE TABLE query_batch(ordinal INTEGER PRIMARY KEY, person INTEGER, area INTEGER,
                         good INTEGER, quantity INTEGER);
'''
PLANS = {
    # Scan only this person's store index, then probe the typed stock map per store.
    'store_scan': '''SELECT s.inventory FROM stores s
        WHERE s.person=:person AND (s.area=:area OR s.carrier=:person)
        AND EXISTS(SELECT 1 FROM stocks b WHERE b.person=s.person
            AND b.inventory=s.inventory AND b.good=:good AND b.quantity>=:quantity)
        ORDER BY s.inventory''',
    # Predicate pushdown: only examine stock facts for this person and requested good.
    'good_index': '''SELECT s.inventory FROM stocks b INDEXED BY stock_by_good
        CROSS JOIN stores s ON s.person=b.person AND s.inventory=b.inventory
        WHERE b.person=:person AND b.good=:good AND b.quantity>=:quantity
        AND (s.area=:area OR s.carrier=:person) ORDER BY s.inventory''',
    # A read projection, incrementally maintained when stock/location payloads change.
    'materialized_join': '''SELECT inventory FROM remembered
        WHERE person=:person AND good=:good AND quantity>=:quantity
        AND (area=:area OR carrier=:person) ORDER BY inventory''',
    'batched_join': '''SELECT q.ordinal,b.inventory FROM query_batch q
        CROSS JOIN remembered b ON b.person=q.person AND b.good=q.good
        WHERE b.quantity>=q.quantity AND (b.area=q.area OR b.carrier=q.person)
        ORDER BY q.ordinal,b.inventory''',
}


def build(stores, stocks):
    started = time.perf_counter()
    db = sqlite3.connect(':memory:')
    db.executescript(SCHEMA)
    db.executemany('INSERT INTO stores VALUES(?,?,?,?)', stores)
    db.executemany('INSERT INTO stocks VALUES(?,?,?,?)', stocks)
    db.commit()
    loaded = time.perf_counter()
    storage = {'normalized': logical_bytes(db)}
    db.execute('CREATE INDEX stock_by_good ON stocks(person,good,quantity,inventory)')
    indexed = time.perf_counter()
    storage['with_good_index'] = logical_bytes(db)
    db.executescript('''CREATE TABLE remembered(person INTEGER, good INTEGER,
        inventory INTEGER, area INTEGER, carrier INTEGER, quantity INTEGER,
        PRIMARY KEY(person,good,inventory)) WITHOUT ROWID;
        INSERT INTO remembered SELECT b.person,b.good,b.inventory,s.area,s.carrier,b.quantity
        FROM stocks b JOIN stores s ON s.person=b.person AND s.inventory=b.inventory;''')
    materialized = time.perf_counter()
    storage['with_good_index_and_join'] = logical_bytes(db)
    db.execute('ANALYZE')
    ready = time.perf_counter()
    return db, {'normalized_load_ms': (loaded-started)*1000,
                'good_index_build_ms': (indexed-loaded)*1000,
                'materialized_join_build_ms': (materialized-indexed)*1000,
                'analyze_ms': (ready-materialized)*1000,
                'all_views_build_ms': (ready-started)*1000,
                'logical_bytes': storage}


def logical_bytes(db):
    return db.execute('PRAGMA page_count').fetchone()[0] * db.execute('PRAGMA page_size').fetchone()[0]


def evaluate(db, plan, queries):
    if plan == 'batched_join':
        result = [[] for _ in queries]
        for ordinal, inventory in db.execute(PLANS[plan]):
            result[ordinal].append(inventory)
        return [tuple(row) for row in result]
    return [tuple(row[0] for row in db.execute(PLANS[plan], q)) for q in queries]


def batch(db, queries):
    started = time.perf_counter()
    db.execute('DELETE FROM query_batch')
    db.executemany('INSERT INTO query_batch VALUES(?,?,?,?,?)',
        ((i, q['person'], q['area'], q['good'], q['quantity']) for i, q in enumerate(queries)))
    db.commit()
    return (time.perf_counter()-started)*1000


def reference(stores, stocks, queries):
    by_person, stock = {}, {}
    for p, i, area, carrier in stores:
        by_person.setdefault(p, []).append((i, area, carrier))
    for rows in by_person.values():
        rows.sort()
    for p, i, good, quantity in stocks:
        stock[p, i, good] = quantity
    return [tuple(i for i, area, carrier in by_person.get(q['person'], [])
                  if (area == q['area'] or carrier == q['person'])
                  and stock.get((q['person'], i, q['good']), 0) >= q['quantity'])
            for q in queries]


def update(db, row, quantity):
    person, inventory, good, _ = row
    # One transaction maintains the normalized and materialized read projections.
    db.execute('UPDATE stocks SET quantity=? WHERE person=? AND inventory=? AND good=?',
               (quantity, person, inventory, good))
    db.execute('UPDATE remembered SET quantity=? WHERE person=? AND inventory=? AND good=?',
               (quantity, person, inventory, good))


def study(checkpoint, rounds, passes):
    input_hash, driver_hash = digest(checkpoint), digest(Path(__file__))
    started = time.perf_counter()
    world = json.loads(checkpoint.read_text())
    decoded = time.perf_counter()
    stores, stocks, actors = project(world)
    projected = time.perf_counter()
    db, construction = build(stores, stocks)
    rules = world['resources']['health.rules']
    if rules['supply'] < 1:
        raise ValueError('experiment requires a positive supply quantity')
    goods = sorted({1, rules['supply_object']})
    queries = [dict(person=p, area=a, good=g, quantity=rules['supply'] if g == rules['supply_object'] else 1)
               for p, a in actors for g in goods]
    if not queries:
        raise ValueError('checkpoint has no stationary living actors')
    batch_ms = batch(db, queries)
    expected = reference(stores, stocks, queries)
    report = {'checkpoint': str(checkpoint.resolve()), 'checkpoint_sha256': input_hash,
              'driver_sha256': driver_hash, 'sqlite': sqlite3.sqlite_version,
              'platform': platform.platform(), 'rounds': rounds, 'passes': passes,
              'people': len(actors), 'remembered_stores': len(stores),
              'remembered_stock_rows': len(stocks), 'queries_per_pass': len(queries),
              'output_rows_per_pass': sum(map(len, expected)), 'goods': goods,
              'json_decode_ms': (decoded-started)*1000,
              'belief_projection_ms': (projected-decoded)*1000,
              'construction': construction, 'runs_ms': {name: [] for name in PLANS},
              'batch_parameter_load_ms': batch_ms,
              'query_plans': {name: list(db.execute('EXPLAIN QUERY PLAN '+sql,
                              {} if name == 'batched_join' else queries[0]))
                              for name, sql in PLANS.items()},
              'scope': 'SQLite query plans over immutable real beliefs, not Rust end-to-end simulation speed'}
    store_counts = Counter(p for p, *_ in stores)
    good_counts = Counter((p, g) for p, _, g, _ in stocks)
    thresholds = {g: rules['supply'] if g == rules['supply_object'] else 1 for g in goods}
    available_counts = Counter((p, g) for p, _, g, n in stocks if g in thresholds and n >= thresholds[g])
    report['candidate_rows_by_good'] = {g: {
        'person_store_rows': sum(store_counts[p] for p, _ in actors),
        'person_good_stock_rows': sum(good_counts[p, g] for p, _ in actors),
        'qualifying_stock_rows': sum(available_counts[p, g] for p, _ in actors),
    } for g in goods}
    for name in PLANS:
        assert evaluate(db, name, queries) == expected
    names = list(PLANS)
    for round_ in range(rounds):
        for name in names if round_ % 2 == 0 else reversed(names):
            begin = time.perf_counter()
            for _ in range(passes):
                actual = evaluate(db, name, queries)
            elapsed = (time.perf_counter()-begin)*1000/passes
            assert actual == expected
            report['runs_ms'][name].append(elapsed)
    report['median_ms_per_pass'] = {k: statistics.median(v) for k, v in report['runs_ms'].items()}
    report['fresh_batch_estimate_ms'] = report['median_ms_per_pass']['batched_join'] + batch_ms
    # Exercise an actual queried positive fact so zeroing it changes the result.
    chosen = next((r for r in stocks if r[3] > 0 and any(
        q['person'] == r[0] and q['good'] == r[2] and r[1] in output
        for q, output in zip(queries, expected))), None)
    if chosen:
        db.execute('SAVEPOINT update_probe')
        begin = time.perf_counter()
        update(db, chosen, 0)
        report['single_stock_update_ms'] = (time.perf_counter()-begin)*1000
        changed = reference(stores, [r if r != chosen else (*r[:3], 0) for r in stocks], queries)
        assert changed != expected
        for name in PLANS:
            assert evaluate(db, name, queries) == changed
        db.execute('ROLLBACK TO update_probe')
        for name in PLANS:
            assert evaluate(db, name, queries) == expected
        db.execute('RELEASE update_probe')
    report['logical_database_bytes'] = logical_bytes(db)
    report['exact_rows_and_update_rollback'] = True
    assert digest(checkpoint) == input_hash and digest(Path(__file__)) == driver_hash
    db.close()
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--checkpoint', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--rounds', type=int, default=5)
    parser.add_argument('--passes', type=int, default=10)
    args = parser.parse_args()
    if args.rounds < 1 or args.passes < 1:
        parser.error('rounds and passes must be positive')
    if args.output.exists():
        parser.error('output must be new')
    result = study(args.checkpoint, args.rounds, args.passes)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps({k: result[k] for k in ['people', 'median_ms_per_pass', 'construction']}, indent=2))


if __name__ == '__main__':
    main()
