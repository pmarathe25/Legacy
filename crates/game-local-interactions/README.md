# Local interaction frames

Experimental character-owned resolution of selected local care and self-meal
offers. Food care describes and completes its own intentions through typed
requests; physiology owns serving and personal effects. Shared participant and
stock claims are admitted together, then a transactional owner batch settles the
offers. Ordinary execution handles unsupported actions, hooks and invalidation.

Resolution first collects selections across the bounded cohort, then settles the
admitted meal batch, then resumes ordinary selections, denied/refused offers,
stale replans and scheduling callbacks. Deferred selections recheck their body,
plan, failure state, action identity, player control and urgent generation before
execution. Providers are not regenerated unless explicit recheck hooks exist;
execution owners validate the retained intention, and rejection tries remaining
prepared alternatives with live barriers. New activities are preserved; ended
continuations are discarded without failure learning. Urgently invalidated deferred actors replan before
other deferred actors; ties retain cohort precedence. Preparation still runs
provider, routine and review hooks, so collection is not a pure read phase.

This is a separate behavioral model: each admitted care offer gives one serving,
and post-meal observers see the settled batch. Stock remains ordered; disjoint
personal effects use owner batches. Denial is not failure learning, while an
actual refusal retains ordinary selection failure learning. Transient review and
patient urgency watches cannot survive commit or serialization. No throughput or
fairness guarantee is implied by participant rotation within bounded groups.
