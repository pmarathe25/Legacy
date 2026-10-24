# game-attraction

Owns subjective attraction and learning about a conversation partner's interests.
Inherited `taste-size`, `taste-build`, and `aesthetic-sensitivity` predispose tastes.
Personally learned `beauty:size` and `beauty:strength` standards can shift them;
favorable encounters and discourse provide ways for those standards to change.

Attraction combines coarse locally perceived build, affection, trust, admiration,
fear, grievance, and compatibility with personally heard interests and convictions.
Private values and another person's genome are not compatibility oracles. Privacy
can discourage disclosure; coerced testimony does not establish compatibility.
`companionship` excludes romantic physical preferences, including when considering
care. `game-family-life` owns mutual bond decisions and `game-parenthood` owns
reproductive intentions; physiological reproductive drive is one incentive.

Campaign tests compare conflicting genetic tastes, learned beauty standards,
private versus heard opinions, refusal, and save/load.

An opt-in compatibility prototype uses a bounded incremental join of one observer's value row, beliefs,
and personally heard opinions. Weak storage revision tokens invalidate it on staged
edits, replacement, and rollback without retaining old worlds. Relationship emotion
is read afresh. Physical appraisal separately reuses numeric tastes and apparent
build, checked against observer values, trait/vitality revisions, and the exact clock;
visibility still uses current positions. Neither view changes observations, provider
order, action timing, or saved state. Each thread owns bounded disposable views.

The original formulas remain the default. `LEGACY_APPRAISAL_CACHE=1` opts into
the prototype views; `0` explicitly disables them for paired whole-history checks
in the same executable. `tools/history/appraisals.py` alternates both settings,
checks complete outputs, and separately checks reloads and four-worker replays.
