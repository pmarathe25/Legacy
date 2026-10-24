#!/usr/bin/env python3
"""Test GEMM and row-wise arithmetic on saved, actor-specific option shortlists.

Requires NumPy. This measures scoring only, not fresh opportunity discovery or
complete choices. Run after other builds/benchmarks, with BLAS threads bounded.
"""
import argparse
import contextlib
import hashlib
import io
import json
import os
from pathlib import Path
import statistics
import time

TERMS = ['need', 'goal', 'value', 'effort', 'risk']
LOW, HIGH = -(2**63), 2**63-1


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def trunc(value, divisor):
    return (abs(value)//divisor) * (-1 if value < 0 else 1)


def saturate(value):
    return min(HIGH, max(LOW, value))


def weights(traits, person):
    result = dict.fromkeys(TERMS, 100)
    for key, value in sorted(traits['people'].get(person, {}).items()):
        definition = traits['definitions'].get(key)
        if definition is None:
            continue
        for term, coefficient in definition['weights'].items():
            if term in result:
                delta = trunc((value-definition['default'])*coefficient, 100)
                result[term] = min(1000, max(0, result[term]+delta))
    return [result[t] for t in TERMS]


def score(terms, weight):
    weighted = [trunc(saturate(v*w), 100) for v, w in zip(terms, weight)]
    total = 0
    for i, value in enumerate(weighted):
        total = saturate(total + (value if i < 3 else -value))
    return total


def study(checkpoint, rounds, passes):
    input_hash, driver_hash = digest(checkpoint), digest(Path(__file__))
    import_started = time.perf_counter()
    import numpy as np
    imported = time.perf_counter()
    configuration = io.StringIO()
    with contextlib.redirect_stdout(configuration):
        np.show_config()
    decode_started = time.perf_counter()
    world = json.loads(checkpoint.read_text())
    decoded = time.perf_counter()
    traits = world['resources']['traits.dispositions']
    raw, owner, ids, actor_weights = [], [], [], []
    for person, mind in world['resources']['characters.minds']['people'].items():
        offers = (mind.get('considered') or {}).get('offers', [])
        if not offers:
            continue
        actor = len(actor_weights)
        actor_weights.append(weights(traits, person))
        for offer in offers:
            motive = offer['motive']
            raw.append([motive[t] for t in TERMS])
            owner.append(actor)
            ids.append(motive['id'])
    if not raw:
        raise ValueError('checkpoint must contain saved option-memory shortlists')
    rows_ready = time.perf_counter()
    expected = [score(row, actor_weights[a]) for row, a in zip(raw, owner)]
    # The vector integer prototype needs a nonoverflowing range; exact saturated
    # production kernels already live in tools/compute. Never silently wrap here.
    if any(sum(abs(v*w) for v, w in zip(row, actor_weights[a])) >= HIGH
           for row, a in zip(raw, owner)):
        raise ValueError('outside nonoverflowing integer prototype range')
    array_started = time.perf_counter()
    terms = np.asarray(raw, dtype=np.int64)
    owners = np.asarray(owner, dtype=np.intp)
    weight = np.asarray(actor_weights, dtype=np.int64)
    float_terms = np.asarray(raw, dtype=np.float64)
    float_weights = np.asarray(actor_weights, dtype=np.float64)
    signs = np.asarray([1, 1, 1, -1, -1])
    signed_terms = float_terms*signs
    scaled_weights = float_weights/100
    columns = np.arange(len(raw))
    packed = time.perf_counter()
    gemm_buffer = np.empty((len(actor_weights), len(raw)), dtype=np.float64)
    gemm_buffer.fill(0)
    buffer_ready = time.perf_counter()

    def integer_rows():
        products = terms * weight[owners]
        weighted = (np.abs(products)//100) * np.sign(products)
        return (weighted*signs).sum(axis=1)

    def batched_dot():
        return np.einsum('ij,ij->i', signed_terms, scaled_weights[owners])

    def gemm():
        # BLAS also scores every other actor's ineligible candidates.
        np.matmul(scaled_weights, signed_terms.T, out=gemm_buffer)
        return gemm_buffer[owners, columns]

    def winners(scores):
        result = {}
        for i, (a, value) in enumerate(zip(owner, scores)):
            rank = (value, -ids[i])
            if a not in result or rank > result[a][0]:
                result[a] = (rank, ids[i])
        return [result[a][1] for a in range(len(actor_weights))]

    functions = {'integer_rows': integer_rows, 'batched_float_dot': batched_dot, 'dense_float_gemm': gemm}
    exact = winners(expected)
    result = {'checkpoint': str(checkpoint.resolve()),
              'checkpoint_sha256': input_hash, 'driver_sha256': driver_hash,
              'numpy': np.__version__, 'numpy_configuration': configuration.getvalue(),
              'blas_threads': {k: os.environ.get(k) for k in ['OPENBLAS_NUM_THREADS', 'OMP_NUM_THREADS', 'MKL_NUM_THREADS']},
              'actors': len(actor_weights), 'eligible_candidates': len(raw),
              'saved_lists_may_include_consumed_or_expired_options': True,
              'gemm_scores_computed': len(actor_weights)*len(raw),
              'gemm_output_bytes': len(actor_weights)*len(raw)*8,
              'gemm_reuses_output_buffer': True,
              'gemm_output_buffer_initialization_ms': (buffer_ready-packed)*1000,
              'library_import_ms': (imported-import_started)*1000,
              'snapshot_decode_ms': (decoded-decode_started)*1000,
              'candidate_projection_ms': (rows_ready-decoded+packed-array_started)*1000,
              'reference_and_range_check_ms': (array_started-rows_ready)*1000,
              'array_construction_ms': (packed-array_started)*1000,
              'rounds': rounds, 'passes': passes, 'kernels': {},
              'scope': 'Saved raw shortlist score arithmetic only; excludes fresh providers, current actions, failure costs, food and commit'}
    runs = {name: [] for name in functions}
    names = list(functions)
    for round_ in range(rounds):
        for name in names if round_ % 2 == 0 else reversed(names):
            begin = time.perf_counter()
            for _ in range(passes):
                actual = functions[name]()
            runs[name].append((time.perf_counter()-begin)*1000/passes)
    for name, function in functions.items():
        actual = function()
        result['kernels'][name] = {'runs_ms': runs[name], 'median_ms': statistics.median(runs[name]),
            'different_scores': sum(float(x) != int(y) for x, y in zip(actual, expected)),
            'different_winners': sum(x != y for x, y in zip(winners(actual), exact))}
    assert result['kernels']['integer_rows']['different_scores'] == 0
    assert digest(checkpoint) == input_hash and digest(Path(__file__)) == driver_hash
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--checkpoint', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--rounds', type=int, default=5)
    parser.add_argument('--passes', type=int, default=100)
    args = parser.parse_args()
    if args.output.exists() or min(args.rounds, args.passes) < 1:
        parser.error('output must be new; rounds and passes must be positive')
    if any(os.environ.get(k) != '1' for k in ['OPENBLAS_NUM_THREADS', 'OMP_NUM_THREADS', 'MKL_NUM_THREADS']):
        parser.error('set OPENBLAS_NUM_THREADS=1 OMP_NUM_THREADS=1 MKL_NUM_THREADS=1')
    result = study(args.checkpoint, args.rounds, args.passes)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps({k: result[k] for k in ['actors', 'eligible_candidates', 'kernels']}, indent=2))


if __name__ == '__main__':
    main()
