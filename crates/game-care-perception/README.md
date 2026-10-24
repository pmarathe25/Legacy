# game-care-perception

Personal candidate scope, observed food need and concern used by voluntary care.
This module registers no plugin or resource. `game-nurture` retains planning and
delivery; typed observation requests mutate the signal owner's state.

`candidates`, `local`, `observed_need` and `concern` preserve the existing
neighborhood rotation, family scope, inference and valuation formulas. Assessment
reads personal evidence and permitted local cues, rather than remote hunger.

`notice_signal` always attempts the ordinary `communication.notice` request.
By default it requests the same decision wake even when observation fails.
The opt-in changes-only mode compares food hypotheses, estimated need, cue,
learned appetite, witnessed feeding and observed area. A repeated food estimate
with a new timestamp, or a cue without a food hypothesis, does not request a new
food-care decision. Perception and forecast timestamps are still updated.
Failed observations retain the ordinary wake. The caller additionally preserves
immediate prompts for existing food deliveries.

`notice_prioritized` also classifies new or worsening urgent food/pain evidence
from the observer's nonexpired perception. It still performs the normal notice,
so coalescing a decision never suppresses perception updates. A refreshed timestamp
alone is not a new emergency. `game-nurture` uses this classification only for
busy-attention actors and sends the character owner's typed `Alert` for urgency;
ordinary observations use the existing choice-change path.

This filtering changes the model: an otherwise irrelevant signal may previously
have caused a voluntary switch or an opportunity to help. The actor now waits for
its next ordinary completion, decision deadline or meaningful interruption.
Unsignalled care can consequently be delayed for the current activity's remaining
duration; there is no five-minute upper bound. `Rules::care_signal_changes_only`
is false and omitted by default. It is independent of routine continuation.

Dependencies include the signal, physiology, vitality, relationship, household,
social-life and character-state owners. Tests compose observation owners directly
and check perception writes, changed food evidence, rollback and reload.

During explicitly enabled whole-actor preparation, local `observed_need` uses
pure signal assessment and defers its owner-applied perception update. Need
inference uses the resulting personal perception and the original forecast
formula. A hooked ordinary notice forces live fallback. Ordinary execution still
calls `Notice` immediately. Tests compare pure-plus-applied assessment with the
ordinary request and protect newer personal learning.
