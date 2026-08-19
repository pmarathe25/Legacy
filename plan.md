# Rust Backend Prototype Plan

## Goal

Build a small, playable settlement simulation in Rust.

The prototype should contain:

- one settlement;
- three or four named areas;
- the ruler;
- two guardians;
- about twelve ordinary citizens;
- food and timber;
- eating, resting, farming, wood gathering, and travel;
- direct orders for the ruler and guardians;
- autonomous decisions for citizens;
- a simple text interface;
- deterministic simulation results.

Use only the Rust standard library. Keep the backend separate from the text interface so a graphical frontend can replace it later.

Do not build combat, graphics, pathfinding, networking, procedural terrain, a full government system, or a general game engine yet.

## Architecture

Use concrete domain types rather than a general ECS.

```text
World
├── Characters
├── Areas
├── Settlements
├── Resources
├── Tasks
├── Projects
├── Enterprises
├── Agreements
└── Clock
```

Process the world through ordered systems:

```text
apply commands
update needs
update projects and tasks
choose actions
resolve actions
update resources
update agreements
record events
advance time
```

Characters, areas, projects, and other records should have typed IDs backed by `u64`.

```rust
struct CharacterId(u64);
struct AreaId(u64);
struct ProjectId(u64);
```

Use one world-wide counter. Start at `1`, increase it for every new record, and never reuse an ID.

## Step 1: Create the project

Create one Cargo package with a library and a small executable.

```text
src/
  lib.rs
  main.rs
  id.rs
  time.rs
  world.rs
  character.rs
  area.rs
  resource.rs
  action.rs
  command.rs
  simulation.rs
```

Keep game rules in the library. `main.rs` should only read commands, call the library, and print results.

Do not add dependencies.

### Completion check

The project builds with:

```text
cargo build
cargo test
```

## Step 2: Add IDs and time

Create the global ID generator.

```rust
struct IdGenerator {
    next: u64,
}
```

Add a game clock stored as an integer number of minutes.

```rust
struct GameTime {
    minute: u64,
}
```

Use a fixed simulation step, such as one minute or five minutes. Keep action rates expressed per hour or per day so the step size can change later.

### Completion check

A test advances the clock through several days and produces the expected date and time.

## Step 3: Add the first world

Create:

- one settlement;
- Village;
- Farmland;
- Forest;
- Mine or Riverbank, if useful;
- one ruler;
- two guardians;
- twelve citizens.

Each area should contain:

- an ID;
- a name;
- links to nearby areas;
- travel time for each link;
- basic production properties.

Each character should contain:

- an ID;
- a name;
- whether they are controlled or autonomous;
- their current area;
- hunger;
- fatigue;
- farming skill;
- woodcutting skill;
- a small personality record;
- their current action.

Start with a plain `Vec<Character>`. Add lookup maps only when they become useful.

### Completion check

The CLI prints every area and the characters currently present there.

## Step 4: Add owned resources

Start with two fungible resources:

- food;
- timber.

Store resources under a specific owner and area.

```rust
struct ResourceHolding {
    owner: OwnerId,
    area: AreaId,
    kind: ResourceKind,
    quantity: i64,
}
```

Use integer units. For example, `1,000` internal units may represent one visible unit.

The settlement government should be a valid owner. Characters can also own resources.

Do not add individual item objects yet.

### Completion check

The CLI can show total resources, public resources, and resources owned by each character.

## Step 5: Add basic needs

Hunger and fatigue should increase as time passes.

Eating lowers hunger and consumes food. Resting lowers fatigue.

Set clear emergency thresholds. A starving or exhausted character may interrupt other work.

Keep the first formulas simple and visible in the code.

### Completion check

A character who does nothing eventually becomes hungry and tired. A character with food and time to rest can recover.

## Step 6: Add actions and abstract travel

Implement:

- idle;
- travel;
- eat;
- rest;
- farm;
- gather wood.

Travel should use area links and remaining travel time. There is no pathfinding. For the first version, require areas to be directly connected or use a simple graph search.

Work should produce progress using skill and elapsed time.

```text
output = base rate × skill modifier × time
```

Farming adds food. Wood gathering adds timber.

### Completion check

A character can travel from the Village to the Farmland, farm for several hours, and return.

## Step 7: Add direct commands

The player directly controls the ruler and guardians.

Add commands such as:

```text
order ruler farm farmland
order guardian-1 gather forest
order guardian-2 rest village
clear-order guardian-1
```

A direct order should remain active until:

- it is completed;
- the player cancels it;
- the action becomes impossible;
- an emergency need temporarily interrupts it.

Ordinary citizens should reject direct orders at this stage. They will use autonomous decisions instead.

### Completion check

The ruler and guardians follow commands across several simulation steps without requiring repeated input.

## Step 8: Add autonomous citizen decisions

Give each citizen a small personality record:

```rust
struct Personality {
    discipline: i16,
    ambition: i16,
    sociability: i16,
    risk_tolerance: i16,
    hedonism: i16,
}
```

Start with a small set of possible choices:

- eat;
- rest;
- farm;
- gather wood;
- remain idle.

Score each choice.

```text
utility =
  need pressure
  + settlement priority
  + skill fit
  + personality effect
  - travel cost
  - fatigue cost
```

Add a small amount of seeded randomness only when needed to prevent identical characters from acting in lockstep.

Characters should usually continue their current action unless another option is clearly better. This prevents constant switching.

### Completion check

Citizens keep themselves alive and respond to changing food and timber priorities without direct orders.

## Step 9: Add settlement directives

Allow the player to set high-level priorities.

```text
priority food high
priority timber low
```

These priorities change citizen utility scores. They do not force citizens to obey.

A disciplined or loyal citizen may respond strongly. Another citizen may ignore the request when tired, unhappy, poorly suited to the work, or focused on a personal goal.

### Completion check

Raising the food priority increases farming activity, but does not guarantee that every citizen farms.

## Step 10: Add tasks and work producers

Create explicit tasks.

```rust
struct Task {
    id: TaskId,
    kind: TaskKind,
    area: AreaId,
    requested_workers: u16,
    priority: i16,
    source: TaskSource,
}
```

Farms and forests should expose work through tasks rather than through hard-coded citizen choices.

Examples:

- farm work available;
- wood gathering available;
- construction work available later.

Characters evaluate tasks and choose one that fits their needs, skills, plans, and personality.

### Completion check

Removing a farm task stops citizens from farming even when the settlement wants more food.

## Step 11: Add dreams, goals, and short-term plans

Add three planning levels:

```text
Dream
  broad aspiration

Goal
  long-term result

Plan
  current approach
```

Use them as utility modifiers.

Example:

```text
Dream: Become wealthy
Goal: Own a farm
Plan: Improve farming skill
```

This character should place more value on farming than another citizen with the same basic skill.

Do not build a general planning language. Use a few enum variants and direct rules.

### Completion check

Two characters with similar needs and skills make different choices because their personalities and plans differ.

## Step 12: Add one project

Implement one proactive project:

```text
Establish Farm
```

A suitable citizen may create this project when:

- they want to own a farm;
- they have enough farming skill;
- the settlement needs more food;
- no existing farm meets their goal;
- their ambition and risk tolerance are high enough.

The project should track requirements:

- land-use rights;
- seed;
- timber;
- startup labor.

The project should expose a next step, such as submitting a proposal to the government.

### Completion check

At least one citizen can independently decide to pursue a farm instead of waiting for a scripted event.

## Step 13: Add proposals and agreements

Support a small set of agreement terms:

```rust
enum Term {
    TransferResource,
    GrantUseRight,
    ProvideLabor,
    ShareOutput,
}
```

The farm proposal might request:

- land rights;
- seed;
- timber.

It might offer:

- a share of future food production;
- public labor during emergencies;
- repayment after the farm begins operating.

The player can accept or reject the proposal. Counteroffers can wait until the basic exchange works.

Acceptance should create a persistent agreement. It should also transfer any immediate resources and rights.

### Completion check

A citizen receives government support, completes the farm project, and later gives the agreed share of production to the government.

## Step 14: Add the first enterprise

When the farm project succeeds, create a farm enterprise.

The enterprise should contain:

- an owner;
- an operating area;
- resource rights;
- stored resources;
- production tasks;
- active agreements.

The farm generates recurring tasks for planting, tending, and harvesting. These may be simplified into one farming task at first.

### Completion check

The full chain works:

```text
personal goal
→ farm project
→ proposal
→ agreement
→ new enterprise
→ work tasks
→ food production
→ agreed output share
```

## Step 15: Improve testing and inspection

Add deterministic scenario tests.

Useful tests include:

- citizens survive thirty days with enough farmland;
- citizens starve when food production is too low;
- guardians follow direct orders;
- settlement priorities change citizen behavior;
- identical seeds and commands produce identical results;
- agreements transfer the correct amount;
- rejected projects do not receive public resources;
- IDs are never reused.

Add an event log for important changes:

```text
Day 3, 09:20: Mara began farming.
Day 5, 14:10: Food reserves fell below the warning level.
Day 8, 11:30: Mara proposed a new farm.
```

The event log should be produced by the backend. The CLI only displays it.

## Suggested CLI commands

```text
status
people
person mara
areas
resources
tasks
projects
agreements
events
run 1h
run 1d
order guardian-1 farm farmland
clear-order guardian-1
priority food high
accept proposal-12
reject proposal-12
quit
```

The CLI is a testing and debugging tool. It does not need polished menus.

## Stop point

Stop after the first enterprise loop works.

Run the simulation many times and inspect:

- whether citizens make believable choices;
- whether survival is too easy or too hard;
- whether direct control is useful;
- whether projects arise at reasonable times;
- whether agreements create interesting decisions;
- which systems are awkward to change or test.

Use those results to choose the next feature. Good candidates are construction, housing, storage, households, apprenticeships, tools, mayors, or a simple expedition system.
