# game-sentiment

Personal emotional attachments to specific objects, people, cultivated plots, and wild plant stands.

**Why it matters:** A crafted tool, a gift from a friend, or a tended garden plot is not interchangeable with an anonymous commodity. `game-sentiment` gives characters personal keepsakes and hated objects that affect trade, gifting, and grief.

## Responsibilities

- **Subject Attachment Ledger:** Stores positive and negative sentimental values per person toward individual durable items (`game-items`), people, cultivated plots (`game-horticulture`), and representative wild plant stands (`game-botany`).
- **Experience-Driven Formation:** Creates or updates attachments when characters craft durable goods (`game-workshop`), tend plants, or receive recognizable gifts (`game-relationships`).
- **Witnessed Loss Distress:** Applies psychological distress via `game-vitality` when a character personally witnesses the destruction of a cherished object.
- **Identity-Aware Valuation:** Supplies per-item sentimental adjustments to `game-exchange` so traders keep treasured keepsakes while trading away plain duplicates.

## Boundaries

- **No Physical Lock:** Sentimental value never magically blocks theft, consumption, transfer, or destruction—it only changes how characters value and react to those events.
- **No Unwitnessed Telepathy:** Remote destruction of a keepsake out of sight does not trigger immediate mental distress.
- **No Choice Policy:** Autonomous gift-giving, keepsake recovery, and storytelling choices live in `game-attachments`.

## How It Works

- **Persistent Subject Memories:** Sentimental records survive item transfer, loss of possession, and even the death or destruction of the subject, allowing later experiences to strengthen or reverse the memory.
- **Sender-Weighted Gifts:** Receiving a recognized individual item scales its initial sentimental value by the recipient's directed relationship with the giver.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers sentiment tables and experience listeners in `game-assembly`.
- **Requests & Events:** Exposes sentiment query and mutation hooks; listens to crafting, horticulture, transfer, and destruction events.
- **Callers:** Queried by `game-attachments`, `game-exchange`, and `game-wellbeing`.

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-objects`, `game-cargo`, `game-items`, `game-transport`, `game-physiology`, `game-vitality`, `game-relationships`, `game-workshop`, `game-botany`, `game-horticulture`.
- **Testing:** Integration tests in `tests/` (`cargo test -p game-sentiment`) cover crafting attachments, gift reactions, witnessed vs. unwitnessed destruction, and keepsake retention during trade.
