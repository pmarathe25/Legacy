# game-frame-tuning

Saved decision-frame configuration shared by scheduling and choice owners.
The experimental full-review interval also uses a per-person `FrameTier` deadline,
urgent generation and bounded same-time retry budget. The owning character state
persists those values; tuning itself does not schedule or commit decisions.

`activity_epochs` enables persistent owner-defined activity admission ahead of
broad deliberation. Its first adapter gives eligible recovery rest priority over
discretionary providers. It requires `batch_decisions`, `urgent_immediate`, and a
positive `commit_batch`; it is a behavior-changing experiment, not an offer cache.
