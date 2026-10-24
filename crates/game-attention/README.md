# game-attention

Serializable attention rates and a coalescing review clock. This crate owns no
plugin or resource. Character scheduling supplies its own callback, handles urgent
interruptions, and writes its own mind state. Existing clock wire fields are
preserved when moving the type out of the personal policy crate.

`Rates` separates ordinary completion/planning from evidence-triggered reviews.
Both are minimum intervals after the last full review; they do not require a full
review at every interval. Evidence can pull an existing ordinary deadline forward.
Urgent paths bypass both floors. Valid rates satisfy
`0 < evidence <= ordinary <= DAY` and use simulation time, never wall-clock time.
