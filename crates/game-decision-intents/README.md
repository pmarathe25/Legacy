# game-decision-intents

Pure autonomous survival alternatives, continuation valuation, trait weighting and
failure scoring. The character owner registers this adapter and retains all mind
writes and physical action execution. `Proceed` keeps its existing wire key and
character-owned handler.

`FrameTuning::minimum_cohort` can leave small cohorts on live decision collection.
The cohort still observes together and validates newly involved actions, but skips
private offer construction, prepared-intent storage and merge. Larger cohorts
retain whole-actor parallel preparation. The threshold is saved and independent
of worker count. Because small-cohort providers read sequential live state, this
is a separate behavioral policy rather than an exact execution-only threshold.

The character clock skips snapshot-recovery retries for these live small groups:
their alternatives were already collected against current inputs. Ordinary
attention deadlines and external cues still apply. Prepared groups retain bounded
idle recovery and selected-proposal rechecks.

The optional whole-decision frame model runs one private offer scratch transaction
per due actor on a shared snapshot, including caregiving assessment, food planning
and final scoring. Typed sensing updates accompany the result and apply through
their original owners only when that actor resolves. Blocked effects or query
errors discard that actor's entire preparation and use ordinary live review.
Prepared alternatives survive save/load within the existing choice frame; no new
resource or plugin appears in ordinary saves. This snapshots more inputs than the
provider-only frame model and deliberately changes the model.

An intervening change to the actor's food plan or failure memory invalidates its
prepared decision. This retains live handling of newly completed plan steps and
new personal failure evidence. New joint activities defer the entire turn through
the frame's action-identity guard before deferred sensing can apply.

When parallel listeners are enabled, small cohorts can also prepare independent
providers inside each actor through the same pool. Larger cohorts use whole-actor
tasks to avoid unnecessary nested drafts. This is an execution strategy, not a
different snapshot policy; results must agree across worker counts and reloads.
Deferred effects and provider provenance merge in ordinary callback order.
