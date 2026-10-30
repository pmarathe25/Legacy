# Bounded combat-order overlays

Validated order commands disable their original outgoing edge through copy-on-write bit pages and store at most 4096 actor-keyed replacement orders. Target defense patches affect incoming base and replacement strikes. The next round evaluates both against the same committed physical and tactical state; target damage saturates, while lazy receipts preserve every attacker in canonical order.

Casualties filter both generations, and old prepared rounds retain their original masks, damage pages and replacement events. Filling the overlay requests a full topology rebuild at next preparation. Encounter admission also rebuilds. The source world remains authoritative for save/reload; overlays are disposable caches.

## Measured fused command rounds

Release, one million participants, five measured repetitions, complete validated command plus next-round preparation/publication; initial import excluded and separately measured at 0.48–0.77 seconds:

| Workload | 1 worker | 2 workers | 4 workers | 6 workers |
| --- | ---: | ---: | ---: | ---: |
| One retarget + spread round | 34.497 ms | 22.317 ms | 16.148 ms | 14.787 ms |
| One retarget + two hot targets | 11.955 ms | 6.537 ms | 4.326 ms | 4.095 ms |

Previous full-rebuild command rounds were 214.14→137.48 ms spread and 135.67→111.23 ms hot at one→six workers. These stable-membership samples exercise persistent retarget overlays; they do not amortize or hide a threshold compaction. Bounded-compaction correctness is tested separately. Large overlays still have bounded serial bookkeeping; this is not near-linear end-to-end scaling.
