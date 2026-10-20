# game-phonology

Numeric phoneme IDs and reversible player transcription, independent of simulated writing.
The inventory follows IAST ordering for vowels, nasals, stops, retroflexes, and sibilants,
with selected aspirates and additional `f` and `w`. It omits visarga, long syllabic liquids,
and rare aspirated retroflex and palatal stops. It is IAST-based, rather than a claim that
all simulated languages are Sanskrit. The sound IDs replace the previous inventory;
previous saves are intentionally incompatible with this change.

A middle dot separates adjacent sounds when concatenation would denote a single
phoneme (`k·h` versus `kh`, `a·i` versus `ai`). Sounds remain numeric internally.
Characters invent their own scripts through literacy; this player display convention
grants them no writing knowledge. Speech samples retain only the sounds actually used.

Symbol reference: [University of Hyderabad's IAST table](https://sanskrit.uohyd.ac.in/manual/transliterate/).
