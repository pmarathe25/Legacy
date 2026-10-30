# game-history-kernel

Preserved executable settlement prototype. The primary ongoing rebuild is now
`game-parallel`; this prototype is retained rather than overwriting that work.

The kernel coordinates separately owned population, economy, and society phases.
It implements personal needs, food/care, rest, abstract travel, work credit,
social pairing, simple pregnancy/birth/parentage, finite lifespans, and idea
transmission. Staged owner results commit only after fallible phase work finishes.
Time is integer minutes; ordinary opportunity boundaries are twelve hours, with
earlier physical/activity deadlines and daily demographic checks. Actual deadline
density can be much higher than two boundaries per day.

Own-format checkpoints preserve all implemented state. Tests cover worker/reload
determinism, care contention, stock conservation, starvation/control, and rollback.
No 300-year throughput or feature-equivalence claim has been established.

Missing: realistic ecology/seasonality, routes/geography, property and trade,
institutions, warfare, language, meaningful inheritance/genetics, household rules,
and rich preferences. Food production is generic work; travel is a fixed-duration
connection between any two settlements. The demographic policy is deliberately
simplified. Full Legacy checkpoint migration is not supported.
