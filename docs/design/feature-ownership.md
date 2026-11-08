# Feature ownership

The current workspace contains one simulation implementation. [The crate index](../../crates/README.md) identifies its composition and player interfaces; individual crate READMEs and code define their current responsibilities. The old simulation and its feature crates are available in git history, not in this workspace.

Physical custody and dated lots are owned by `game-lots` and its supporting lot crates. `game-economy` stages and composes the independent feature owners, while `game-economy-player` restricts ordinary player observations and actions. `game-social`, `game-opinions`, and `game-knowledge` respectively model encounters, directed personal feelings, and bounded personal observations and reports. Sharing information does not establish its truth, and meeting or cooperating does not imply private approval.

Feature owners maintain their own state and resolve typed requests against committed views. Physical transfers and shared-resource contention use the custody boundary. Personal arrangements, pressure, and interpretation influence individual choices; no institutional entity compels compliance. See the domain design documents for intended behavior beyond the implemented slices.
