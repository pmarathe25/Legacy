# game-anticipation

Advisory preparation across a bounded frontier of scheduled work. This library
owns the driver cursor and `Prepare`/`Preview` contract; feature owners own input
capture, pure computation, dependency validation, and result caches.

`Window::prepare<R>` inspects at most 128 queued jobs with the first due job's
timestamp and priority. A typed caller supplies the actor mapping. It deduplicates
actors and queries `Prepare` at that logical time. The cursor avoids preparing
the same captured frontier before every canonical commit. New earlier-priority
work still executes through the scheduler and can trigger a fresh preparation
window when the original phase resumes. Cancellation consumes no prepared effects.

Subscribers must treat preparation as advisory: they may populate bounded,
validated process caches, but cannot commit observations, allocations, notices,
actions, or schedules. The query holds a read-only view; all workers join before
it returns. Retain focused inputs or weak dependency versions, never whole mutable
world snapshots across commits. A failed query or panic increments runtime
statistics and leaves ordinary dispatch available. Owners must revalidate every
prepared result when used, including inputs changed by earlier observations.

`register` adds the request under the calling plugin's owner. It adds no resource,
plugin manifest entry, saved cursor, or alternate dispatch engine. `Window::new(0)`
is disabled. The history runner's `--prepare-window 32` enables the initial
livelihood planning pilot for `game-characters::Resume`; `Host::advance` itself does not initiate advisory preparation. One worker exercises the same preparation and validation path.

Tests compare complete saves across worker counts with inserted earlier phases,
cancellation, invalidated inputs, advisory errors/panics, read-only writes,
future logical time, disabled windows, and save/load binding.

`Window::advance` integrates preparation with the committing host loop while
preserving work budgets, stop checks and returned progress. The main
`game-assembly::Simulation::advance` API uses this adapter; the history runner no
longer maintains a separate preparation loop. Disabled windows delegate directly
to `Host::advance`.
