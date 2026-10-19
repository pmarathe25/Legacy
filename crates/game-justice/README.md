# game-justice

Voluntary case prosecution (`Prosecute`), personally heard evidence, precedent-based judgments, and fallible enforcement.

**Why it matters:** Law in Legacy is not an omniscient police script. Contested transfers only become cases if a claimant chooses to prosecute with their own evidence, judges weigh cases through personal values and precedents, and enforcement can fail if the target resists or the goods are gone.

## Responsibilities

- **Incident Memory & Prosecution (`Prosecute`):** Records locally witnessed contested transfers for claimants and lets them voluntarily file a case (`Prosecute`) from their personal evidence.
- **Hearings & Precedents:** Tracks personally heard cases, testimony transfers (`game-testimony`), and prior judgments as precedents.
- **Value-Weighted Judgments:** Evaluates heard evidence against a judge's personal norms and precedents to issue a ruling—without automatically moving any goods.
- **Attempted Enforcement:** Attempts physical recovery or restitution orders, which can fail if the respondent refuses, flees, or no longer holds the disputed items.

## Boundaries

- **No Automatic Crime Detection:** An un-witnessed theft or an institutional claim never creates a complaint, crime record, or sentence on its own.
- **Judgments Move No Goods:** Issuing a ruling is a social act; physical recovery requires separate enforcement and `game-cargo` / `game-property` transfers.

## How It Works

- **Evidence-Gated Filing:** Listens to contested property use events (`game-property`) witnessed by local observers. A claimant can later bring that remembered incident to a recognized judge or transmit the case via `game-testimony`.
- **Fallible Restitution:** Enforcement requests check co-location, actor capability, and physical inventory availability, escalating or failing cleanly when compliance is refused.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers incident memories, court case dockets, precedent logs, and `Prosecute` / judgment / enforcement handlers in `game-assembly`.
- **Requests & Events:** Exposes `Prosecute`, case hearing, judgment, and enforcement actions via `game-choice`.
- **Callers:** Triggered by `game-property` contested transfers and `game-choice` decision cycles; coordinates with `game-testimony` and `game-institutions`.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-choice`, `game-objects`, `game-cargo`, `game-property`, `game-transport`, `game-physiology`, `game-vitality`, `game-traits`, `game-knowledge`, `game-testimony`, `game-communication`, `game-relationships`, `game-institutions`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-justice`) cover witnessed vs. unwitnessed incidents, voluntary filing, testimony transmission, judgment precedents, failed enforcement, and save/load consistency.
