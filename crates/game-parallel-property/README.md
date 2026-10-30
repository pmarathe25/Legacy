# Parallel property

An optional property owner assigns a regional nonfood stock to a person and can delegate use to other local people. Unclaimed stocks remain public. Production inputs and outputs and local goods transfers require the worker or holder to have access. Permission checks read an immutable index in parallel; reservations already accepted remain valid if permissions later change.

A title transfer requires an owner proposal and a separate recipient decision while both are living and local. Acceptance changes ownership and clears delegated access. Without an inheritance policy, a deceased owner retains the claim; titles do not pass to a reincarnation automatically. This is a limited stock-rights model, not a complete land, inheritance, legal, or political system.

The fused host also supports `steal-good PERSON GOOD QUANTITY`: an unauthorized actor can physically take a claimed stock without receiving permission. The stock owner remains unchanged. A living owner in the same region witnesses the incident; with configured `theft_loss`, that owner loses directed trust in the actor. The actor sees their own taking, while an absent owner does not receive a witnessed incident.

`work-unpermitted ID PERSON REGION RECIPE UNITS` similarly lets a person use claimed materials for production without permission. The material is physically reserved, the claim remains, and a present owner can witness unauthorized input use and update their opinion. The action is still subject to ordinary work, age, capacity, and material constraints; it does not imply agreement or physical enforcement by an institution.

Optional `[property.inheritance]` rules specify `minimum_age` and `prefer_partner`. A living eligible partner at the death boundary takes priority when configured; otherwise the lowest-ID living eligible child inherits. An estate stays with the deceased if no eligible heir exists and is reconsidered as children mature. Inheritance clears delegated access and pending title offers, preserves the physical stock, and records the transfer for the heir.
