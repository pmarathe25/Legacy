# Personal conflict

This owner resolves individually chosen simultaneous strikes and each target's
own chosen defensive reduction. Targets need not consent to being attacked; a
counterstrike requires a separate personal choice. It stages attributed
potential harm and groups impacts by target in parallel, including when many
people strike one target. It does not own health, location, capability,
relationships, or decisions. The host must validate those and commit actual
health changes. Potential harm is not an attribution of actual health lost.

The interval host can execute personal attack choices and individual defense
preferences from data, apply health loss, and pause a fatally injured worker's
physical work. At the next settlement, their active work is interrupted or
preserved for succession. Optional `personal_conflict.reactions` entries give
each person independent `as_actor` and `as_target` interpretations of
`penetrated` and `blocked` strikes. Each interpretation supplies signed
`trust`, `affinity`, `fear`, and `obligation` changes; absent values have no
effect. A penetration describes a strike's potential harm, not an attribution
of actual health loss when several strikes exceed the target's health. These
changes require an enabled opinions owner.

For example, a scenario can give person 2 a positive affinity response to a
penetrating strike and a separate fear response to a blocked strike:

```toml
[[personal_conflict.reactions]]
person = 2
as_target = { penetrated = { affinity = 4 }, blocked = { fear = 2 } }
```

This is an initial slice: tactical positioning, equipment, injuries and
recovery, intervention, and richer ongoing conflict remain unimplemented.
