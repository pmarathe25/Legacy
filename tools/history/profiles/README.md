# Physical starting conditions for history experiments

These overlays define geography, abundance, founding population, and clocks. They
create no relationships, institutions, roles, replacement births, or protected
characters. Apply one with `--mod tools/history/profiles/<name>`.

| Overlay | Founders | Areas | Environment |
| --- | ---: | ---: | --- |
| `32-people` | 32 | 4 | Shipping biome mixture |
| `warm-32` | 32 | 2 | Warm agricultural regions |
| `warm-96` | 96 | 6 | Warm agricultural regions |
| `rainforest-32` | 32 | 6 | Rainforest |
| `rainforest-96` | 96 | 6 | Rainforest |
| `coastal-foraging-64` | 64 | 4 | Dry tidal headlands, limited shellfish renewal |
| `coastal-foraging-64-renew-10` | 64 | 4 | Same headlands, shellfish renewal 10 instead of 3 |
| `coastal-foraging-64-renew-30` | 64 | 4 | Same headlands, renewal 30 |
| `coastal-foraging-192` | 192 | 12 | Larger founding pool, same initial density and renewal 10 |
| `coastal-diverse-192` | 192 | 12 | Headlands with kelp and nettles for additional food, textiles, and care |
| `rainforest-96-quarterly` | 96 | 6 | Renewal 10, crop growth at least 90 days |
| `nut-forest-96` | 96 | 3 | Warm nut forest with craft, crop, and care resources |

The coastal condition uses ordinary food gathering, needs, production, household
decisions, and inheritance. Rain is zero, so available grain cannot establish a
rain-fed farm. Wild shellfish replenish at three units per fixed founding
multiplier daily, rather than the default 400. Timber, bark, resin, and healing
herbs support some craft and care chains. Characters can consume, preserve,
exchange, hoard, or waste supplies. There is no imposed population limit.

Coastal seeds 3, 4, and 5 were selected before running this condition. After the
first decades exposed severe losses at renewal 3, the same three seeds were
started at renewal 10. Keep both conditions' outcomes; the second condition was
chosen in response to early evidence. Each runs
until pedigree depth 20, extinction, or 1,000 biological years. Report failed
worlds alongside any continuing lineage. Dryness is an experimental crop
constraint: thirst, detailed soils, marine reproduction, and tidal movement are
not modeled. This condition does not validate a complete coastal ecology.

The renewal-30 and larger-pool conditions follow the renewal-10 extinctions.
They distinguish more food from a larger initial pool of potential partners;
they do not assign partners or guarantee surviving descendants. Seeds 3, 4,
and 5 are retained for each condition. These are exploratory, adaptive experiments.

The larger coastal pools also became extinct. Follow-up conditions add diverse
coastal resources, or constrain rainforest food production with quarterly crop
growth. Crop timings are an experimental resource regime, not a validated plant
model. The rainforest condition retains the broad crop/material catalog and
allows cultivation; finite renewal and slower harvests address the abundant
world's escalating population without imposing a population ceiling.

The supervised queue tries rainforest seeds 4, 3, 5, then diverse coastal seeds
4, 3, 5, then coastal renewal-10 seeds 1, 2, 6–10. It stops after the first verified
depth-20 result or queue exhaustion. Remaining queued worlds are recorded as not
started, not extinctions. This exploratory first-success selection cannot estimate
the probability that an arbitrary founding population will persist.

Earlier small mixed-biome pilots preceded implementation fixes. Do not combine
them with the corrected runs as a controlled comparison.

The fresh chunked study uses `nut-forest-96` first, then an ordinary mixed-biome
world. The forest's main food contains more fat and protein than fruit, and the
smaller area count increases opportunities to meet. These are deliberate physical
conditions, not a controlled comparison with the retired runs. Characters still
choose whether to form relationships or have children. No population is guaranteed
to persist; see the study's progress record for actual ancestry depth.

`nut-forest-96-limited` retains the 96-founder warm forest and material resources, lowers wild food renewal to 10 per fixed founding multiplier, and applies the existing quarterly crop-growth overlays. This exploratory habitat follows the abundant forest’s population growth and scaling costs. It prescribes neither institutions nor surviving population size. Compare outcomes with their distinct resource conditions; this is not a controlled test of a single behavioral change.
