# game-crop-cohorts

`game-crop-cohorts` provides pure physical calculations for mixed-strain seed cohorts growing together in a single cultivated plot.

**Why it matters:** When a farmer sows seed drawn from multiple plant strains, each constituent strain responds differently to heat, cold, and rain; computing survival and offspring weights per constituent preserves genetic selection without creating separate plots per seed.

## Responsibilities

- **Mixed cohort state (`Cohort`):** Tracks constituent strain IDs, seed counts, and accumulated suitable/stressful weather exposure inside a planting.
- **Maturation & harvest math:** Computes when a mixed planting finishes growing (paced by the slowest surviving constituent) and allocates total harvest yield proportionally across surviving strains' offspring weights.

## Boundaries

- **Pure cohort math:** Owns no `game-sim` plugin or plot state; [`game-agriculture`](../game-agriculture/README.md) owns plots and field labor, and [`game-cultivars`](../game-cultivars/README.md) owns strain genotypes and cargo lot tracking.

## How It Works

- **Constituent weather integration:** Evaluates seasonal warmth and rain against each constituent strain's expressed `game-cultivars` tolerances so poorly adapted strains suffer higher mortality or lower yield share.
- **Exact yield conservation:** Distributes integer harvest quantities deterministically across surviving offspring strains without rounding loss.

## Game Integration

- **Plugin / Entry Point:** Pure helper library; registers no `game-sim` plugin.
- **Requests & Events:** None.
- **Callers:** Used by [`game-agriculture`](../game-agriculture/README.md) during sowing, seasonal growth updates, and harvesting.

## Dependencies & Testing

- **Dependencies:** [`game-core`](../game-core/README.md), [`game-sim`](../game-sim/README.md), [`game-objects`](../game-objects/README.md), [`game-environment`](../game-environment/README.md), [`game-crops`](../game-crops/README.md), [`game-cultivars`](../game-cultivars/README.md).
- **Testing:** Exercised by `cargo test -p game-agriculture -p game-cultivars -p game-botany`.
