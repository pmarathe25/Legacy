# game-choice

Proposal collection, executable offers, timed undertakings, and agenda helpers for competing domain mechanics.

**Why it matters:** Dozens of independent plugins propose actions for the same character at the same time. `game-choice` gathers those offers without coupling feature crates to one another. `game-characters` owns the final decision and commitment policy.

## Responsibilities

- **Proposal Collection:** Broadcasts `Collect` requests so independent feature crates can submit scored `Offer` / `Proposal` candidates for an actor.
- **Bounded Collection:** Retains up to 256 promising offers under the actor's supplied weights, with deterministic ordering and tie-breaking.
- **Execution & Reconsideration:** Dispatches the winning `Run` / `Undertake` callback and handles `Outcome::Reconsider` when an action finishes or becomes invalid.
- **Agenda Scheduling:** Provides `Schedule` and `Next` helpers to wake characters when their current task completes or needs re-evaluation.

## Boundaries

- **Domain-Agnostic:** Does not define what any action does or how domain urgency is calculated (owned by each proposing feature crate).
- **No Direct State Mutation:** Only stores per-actor choice scheduling and active proposal metadata.

## How It Works

- **Provider Registration:** Feature plugins register proposal providers (`provider`) and execution handlers during setup.
- **Scoped discovery:** Pure, nonurgent discovery can opt into `deliberative_provider`, which is skipped only in an explicitly requested fast review. Required observations or side effects must remain on ordinary providers. An opt-in experimental character policy requests this scope between full-review deadlines, including in the live fallback.
- **Collect-Then-Run Cycle:** When a character's decision timer fires (`Next`), `game-choice` gathers proposals from all registered providers, applies `game-utility` scoring and hysteresis against any active task, and invokes the winning handler.

## Game Integration

- **Plugin / Entry Point:** `Plugin` registers choice state, `Collect` arbitration, and `Schedule` / `Next` agenda handlers in `game-assembly`.
- **Requests & Events:** Exposes `Collect`, `submit`, `Schedule`, `Next`, and `Undertake` hooks.
- **Callers:** Used by almost all autonomous behavior crates (`game-food-actions`, `game-livelihoods`, `game-agriculture`, `game-care`, `game-social-life`, `game-inquiry`, `game-conflict`, etc.).

## Dependencies & Testing

- **Dependencies:** `game-core`, `game-sim`, `game-storage`, `game-utility`, `game-anticipation`.
- **Testing:** Integration tests in `tests/` verify order-independent arbitration, hysteresis task persistence, preemption by urgent needs, and reconsideration loops.

Temporary proposals use typed `Pending::immediate` calls and leave no retained
collection at transaction commit. Scheduling an undertaking validates its complete
nested payload through `Pending::new`. Bounded templates reuse immutable offers;
current motives remain independent. An undertaking completing means its callback
was attempted, rather than guaranteeing that the domain action succeeded.

Bounded recall retains a proposal continuing the actor's actual current action.
It still counts toward the ordinary offer limit and receives ordinary utility
scoring; stronger alternatives can win. Unrelated continuation identities receive
no retention preference.

`Keys` indexes append-only offer keys by numeric fingerprints. Hash collisions
still compare complete strings; motive IDs are not treated as key equality.

Provider callbacks execute in canonical order by default. Opting into
`Host::set_parallel_listeners(true)` prepares independent alternatives concurrently
on the staged world. Raw drafts merge in provider order before applying the existing 256-offer
ranking, duplicate checks and continuation preference. `serial_provider` declares
an observation-producing callback as an ordered barrier: preceding drafts merge
before it runs, and subsequent drafts read its resulting world. An unexpectedly
effectful provider retries at its own position; only subsequent speculative drafts
are discarded. Request hooks retain serial collection. Independent providers must
not use partial offers from other providers to compute their drafts.

`collect_with` finishes the complete ordered collection before running its caller's
query. Food planning therefore sees the same observations and world effects with
one or several workers, without speculative whole-collection retries.

Bounded offer retention uses an in-place min-heap after reaching the offer limit.
The priority remains active continuation, weighted score, numeric motive tie break
and full key. Weight changes rebuild the heap. Hash counts accelerate removal;
full keys still decide duplicates. Integration tests compare retained offers with
a scan oracle under changing weights, saturation, ties and continuations.

Once the collection is full, a worse incoming offer is rejected after the ordinary
duplicate check and ranking. A better offer replaces the worst heap entry directly.
This avoids inserting and immediately removing uncompetitive candidates while
preserving provider hooks, request counts, continuation retention and tie breaks.

Temporary proposals can carry `Pending::immediate` inside undertakings and other
action wrappers. Chosen work passes through the ordinary schedule/action boundary,
which validates nested payloads before commit. This avoids serializing discarded
options. Tests cover deferred serialization, scheduled completion and cross-worker
save/load.

Ordinary actions are scored through their explicit motives. Rigidity is not an
extra penalty for changing activities or care recipients. Context owners apply
rigidity when revising convictions, names, relationship preferences, or political
commitments. Attention and ongoing work costs support task persistence.

Choice registers the resource-free `game-anticipation` preview contract. Opt-in
history drivers can warm owner planning caches across queued actors. Previewing
never starts a collection or selects a winner; ordinary provider barriers and
complete scoring still execute at each review's original position.


Proposal state and ordered collection now live in `game-offers`, reexported here
under the same resource owner and wire keys. `game-decision-frames` implements the
separate opt-in phased model. `ConfigureFrames { interval: Some(MINUTE) }` enables
it; `None` disables it and cancels its clock. Ordinary saves omit frame state.
Pure proposals can read an earlier shared step snapshot and are reused until its
end; receiver validation and ordered execution still resolve contested resources.
Effectful and explicitly serial providers retain live callbacks. The current
pilot therefore phases the pure proposal pipeline, not all character behavior.
Frames, prepared payloads and disabled-provider names survive saving and loading;
worker counts do not select different model rules. Tests include shared snapshots,
refresh, contested stock, worker/reload equality, disabled save shape and rollback.

`ConfigureExecution(true)` separately permits owner-authorized goal continuation.
The `GoalReady` contract carries the person, finished action identity, chosen goal
key and an ordinary executable owner request. Choice retains only the saved mode;
the owner validates its plan and the character clock decides whether to continue.
It requires a character attention floor and cannot combine with frames. False is
the default and is omitted from ordinary saves.
