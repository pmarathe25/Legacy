# Legacy

Legacy is a headless simulation game built as a modular Rust 2024 workspace where small, independent crates compose into a single transactional simulation host.

**Why it matters:** Every simulation mechanic—from diploid crop genetics and metabolic hunger to spoken vocabulary and institutional voting—lives in an isolated owner crate under [`crates/`](crates/README.md). Features communicate strictly through typed requests and events, keeping the simulation deterministic, testable in isolation, and extensible via runtime TOML data in [`content/`](content/README.md).

> **Rewrite status:** This workspace is a clean-slate rewrite of an earlier prototype ([`MIGRATION.md`](MIGRATION.md)). Older saves, scenarios, and APIs are incompatible, and portions of the full multigenerational campaign in [`docs/plan.md`](docs/plan.md) and `docs/design/` remain unfinished.

---

## Quick start

Run from the repository root:

```sh
cargo build --workspace                                      # Build all crates
cargo run -p game-cli -- --list                              # List scenarios and fixtures
cargo run -p game-cli -- --seed 1                            # Start default generated world
cargo run -p game-cli -- --scenario rivermeet --seed 1       # Start a community fixture
cargo run -p game-cli -- --scenario kernel-fixture --seed 1  # Start minimal test fixture
cargo test -p game-agriculture                               # Test a single feature crate
cargo test --workspace                                       # Run all workspace tests
cargo fmt --all -- --check                                   # Check formatting
cargo clippy --workspace --all-targets -- -D warnings        # Lint all targets
cargo bench -p game-sim --bench kernel                       # Run kernel benchmarks
```

---

## How to play

- **Generated start (`generated`):** Reads [`content/worldgen/default.toml`](content/worldgen/default.toml), spawns 96 mortals and 2 immortal guardians across 12 connected areas, and runs 2 days of autonomous history before player control begins. No institutions, social bonds, or shared words are pre-scripted.
- **Authored communities:** [`rivermeet`](tools/fixtures/content/scenarios/rivermeet.toml), [`bracken-coast`](tools/fixtures/content/scenarios/bracken-coast.toml), and [`ashridge`](tools/fixtures/content/scenarios/ashridge.toml) provide populated starting settlements with distinct climates, local stores, gardens, and directed relationships.
- **Content & mods:** Pass `--content <dir>` to select a content pack and repeated `--mod <dir>` flags to apply overlays without recompiling (see [`content/README.md`](content/README.md)).
- **Scripted or interactive CLI:** [`game-cli`](crates/game-cli/README.md) reads from `stdin` or `--script <path>`. `run` accepts seconds; internal simulation time uses integer milliseconds:
  ```text
  status
  move 2
  run 120
  status
  quit
  ```
- **Core CLI commands:**
  - **Session & time:** `status`, `run <seconds>`, `save <path>`, `load <path>`, `debug`, `quit`
  - **Perception & social:** `look`, `observe <inventory>`, `observe-patch <patch>`, `tell <person> <fact-key>`, `talk <person>`, `read-work <work>`, `show-work <work> <person>`
  - **Survival & work:** `move <area>`, `eat <inventory>`, `rest`, `plant <crop> <inventory>`, `sow <plot> <inventory>`, `harvest <plot> <inventory>`, `craft <recipe> <inventory>`, `pursue <activity> <inventory>`, `cancel`
  - **Invocation & full API:** `consent <step> yes|no`, `dismiss`, or `command <JSON>` for any [`game_api::Command`](crates/game-api/src/lib.rs)

---

## How it works

- **One transactional host ([`game-sim`](crates/game-sim/README.md)):** Stores typed resources, schedules due requests on one clock, enforces one active action per actor, and validates saves. Every request executes atomically; errors or panics roll back writes, IDs, notices, and scheduled work. Nested `attempt` scopes provide lightweight rollback for recoverable choices.
- **Strict feature ownership ([`crates/game-*`](crates/README.md)):** Each crate owns its state, requests, cancellation rules, content schemas, and save validation. Hosted crates export `plugin(&mut Builder)`; pure utilities (`game-spatial`, `game-genetics`, `game-body`, `game-utility`) stay stateless. Plugins may mutate only their own resources.
- **Acyclic interaction:** Callers submit the receiving owner's typed `Request`. Lower features emit typed `Event`s or internal `notify` signals that upper features subscribe to. Read-only queries borrow committed state and reject mutations.
- **Data-driven catalogs ([`content/`](content/README.md)):** Versioned TOML files define 321 goods, 67 crops, 219 recipes, 18 biomes, 16 climates, 24 pastimes, traits, and rule profiles. [`game-content`](crates/game-content/README.md) loads and fingerprints files; feature owners validate and snapshot them into saves.
- **Ruler-bound assembly ([`game-assembly`](crates/game-assembly/README.md)):** Installs plugins in dependency order, binds public `Command`s to the active ruler from [`game-lineage`](crates/game-lineage/README.md), and builds subjective `PlayerView` snapshots from the ruler's own memories and witnessed events.
- **Enforced budgets:** [Architecture tests](crates/game-assembly/tests/architecture.rs) enforce dependency direction, forbid cross-owner mutations, and cap feature crates at ~600 lines of `src/` code.

---

## What's simulated

- **World & logistics:** Scenario loading and worldgen ([`game-scenarios`](crates/game-scenarios/README.md), [`game-scenario-assets`](crates/game-scenario-assets/README.md), [`game-worldgen`](crates/game-worldgen/README.md), [`game-campaign`](crates/game-campaign/README.md), [`game-openings`](crates/game-openings/README.md)), areas and roads ([`game-land`](crates/game-land/README.md), [`game-spatial`](crates/game-spatial/README.md), [`game-transport`](crates/game-transport/README.md)), seasonal climates ([`game-environment`](crates/game-environment/README.md)), inventories and tracked items ([`game-objects`](crates/game-objects/README.md), [`game-cargo`](crates/game-cargo/README.md), [`game-items`](crates/game-items/README.md)), ownership and ground caches ([`game-property`](crates/game-property/README.md), [`game-stockpiles`](crates/game-stockpiles/README.md)), hauling and construction ([`game-logistics`](crates/game-logistics/README.md), [`game-projects`](crates/game-projects/README.md)), and clothing, indoor heat, comfort, and thermal exposure ([`game-equipment`](crates/game-equipment/README.md), [`game-shelter`](crates/game-shelter/README.md), [`game-comfort`](crates/game-comfort/README.md), [`game-exposure`](crates/game-exposure/README.md)).
- **Biology & lifecycle:** Diploid human genetics and traits ([`game-genetics`](crates/game-genetics/README.md), [`game-traits`](crates/game-traits/README.md), [`game-heredity`](crates/game-heredity/README.md)), childhood development, aging, skills, and wellbeing ([`game-vitality`](crates/game-vitality/README.md), [`game-skills`](crates/game-skills/README.md), [`game-wellbeing`](crates/game-wellbeing/README.md)), metabolism, digestion, diet, and bodily state ([`game-body`](crates/game-body/README.md), [`game-metabolism`](crates/game-metabolism/README.md), [`game-soma`](crates/game-soma/README.md), [`game-physiology`](crates/game-physiology/README.md), [`game-digestion`](crates/game-digestion/README.md), [`game-nutrition`](crates/game-nutrition/README.md)), healing and dependent care ([`game-health`](crates/game-health/README.md), [`game-care`](crates/game-care/README.md), [`game-nurture`](crates/game-nurture/README.md)), and reproduction, households, and family bonds ([`game-lifecycle`](crates/game-lifecycle/README.md), [`game-parenthood`](crates/game-parenthood/README.md), [`game-households`](crates/game-households/README.md), [`game-family-life`](crates/game-family-life/README.md)).
- **Ecology, food & production:** Plant strains, crop cohorts, wild stands, animal populations, and gathering ([`game-crops`](crates/game-crops/README.md), [`game-crop-cohorts`](crates/game-crop-cohorts/README.md), [`game-cultivars`](crates/game-cultivars/README.md), [`game-botany`](crates/game-botany/README.md), [`game-animal-species`](crates/game-animal-species/README.md), [`game-fauna`](crates/game-fauna/README.md), [`game-ecology`](crates/game-ecology/README.md)), farming ([`game-agriculture`](crates/game-agriculture/README.md), [`game-horticulture`](crates/game-horticulture/README.md)), spoilage, cellars, curing/fermentation, and food experiments ([`game-preservation`](crates/game-preservation/README.md), [`game-food-storage`](crates/game-food-storage/README.md), [`game-maturation`](crates/game-maturation/README.md), [`game-food-inquiry`](crates/game-food-inquiry/README.md)), crafting and ingredient planning ([`game-workshop`](crates/game-workshop/README.md), [`game-craft-planning`](crates/game-craft-planning/README.md), [`game-livelihoods`](crates/game-livelihoods/README.md)), and foraging/meal plans ([`game-food-memory`](crates/game-food-memory/README.md), [`game-provisioning`](crates/game-provisioning/README.md), [`game-food-actions`](crates/game-food-actions/README.md)).
- **Mind, language & culture:** Utility scoring, proposal collection, and autonomous minds ([`game-utility`](crates/game-utility/README.md), [`game-choice`](crates/game-choice/README.md), [`game-characters`](crates/game-characters/README.md), [`game-wayfaring`](crates/game-wayfaring/README.md)), shared scenes, subjective knowledge, testimony, and empirical inquiry ([`game-facts`](crates/game-facts/README.md), [`game-knowledge`](crates/game-knowledge/README.md), [`game-testimony`](crates/game-testimony/README.md), [`game-reasoning`](crates/game-reasoning/README.md), [`game-inquiry`](crates/game-inquiry/README.md), [`game-observations`](crates/game-observations/README.md)), phoneme lexicons, nonverbal signals, and invented scripts ([`game-communication`](crates/game-communication/README.md), [`game-signals`](crates/game-signals/README.md), [`game-literacy`](crates/game-literacy/README.md)), personal names ([`game-names`](crates/game-names/README.md), [`game-naming`](crates/game-naming/README.md)), relationships, companionship, conversation, and advocacy ([`game-relationships`](crates/game-relationships/README.md), [`game-social-life`](crates/game-social-life/README.md), [`game-companionship`](crates/game-companionship/README.md), [`game-discourse`](crates/game-discourse/README.md)), sentiment, attachments, demand, credit, and barter ([`game-sentiment`](crates/game-sentiment/README.md), [`game-attachments`](crates/game-attachments/README.md), [`game-demand`](crates/game-demand/README.md), [`game-credit`](crates/game-credit/README.md), [`game-exchange`](crates/game-exchange/README.md)), and leisure and creative works ([`game-pastimes`](crates/game-pastimes/README.md), [`game-works`](crates/game-works/README.md), [`game-poetics`](crates/game-poetics/README.md)).
- **Society, conflict & lineage:** Agreements ([`game-agreements`](crates/game-agreements/README.md)), institutions, communities, and membership ([`game-institutions`](crates/game-institutions/README.md), [`game-communities`](crates/game-communities/README.md), [`game-affiliation`](crates/game-affiliation/README.md), [`game-realm`](crates/game-realm/README.md)), grievances, courts, coercion, combat, confinement, missions, and armies ([`game-grievances`](crates/game-grievances/README.md), [`game-justice`](crates/game-justice/README.md), [`game-pressure`](crates/game-pressure/README.md), [`game-conflict`](crates/game-conflict/README.md), [`game-confinement`](crates/game-confinement/README.md), [`game-missions`](crates/game-missions/README.md), [`game-armies`](crates/game-armies/README.md)), and succession, guardians, and past-life invocation ([`game-lineage`](crates/game-lineage/README.md), [`game-guardians`](crates/game-guardians/README.md), [`game-invocation`](crates/game-invocation/README.md)).

**Current limits:** Full autonomous enterprises, complete institutional invention, regional trade networks, final tactical combat rules, and the complete past-life progression in [`docs/plan.md`](docs/plan.md) are not yet implemented.

---

## Go deeper

- **Crate index:** [`crates/README.md`](crates/README.md) — Responsibilities and dependencies for all 110 crates.
- **Feature ownership:** [`docs/design/feature-ownership.md`](docs/design/feature-ownership.md) — Domain boundaries across overlapping concepts.
- **Empirical learning & writing:** [`docs/design/learning.md`](docs/design/learning.md) — Observation contrasts, conditional deduction, teaching, and invented scripts.
- **Content & modding:** [`content/README.md`](content/README.md) — TOML schemas, catalogs, and overlay rules.
- **Performance benchmarks:** [`tools/performance/README.md`](tools/performance/README.md) — Storage, row-journal, and full-simulation benchmarks.
- **Compute experiments:** [`tools/compute/README.md`](tools/compute/README.md) — Packed CPU, AVX2 column, and Vulkan GPU utility evaluation.
- **History runner:** [`tools/history/README.md`](tools/history/README.md) — Unattended multigenerational runs and journals (outputs belong under `target/history/`).
