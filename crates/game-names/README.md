# game-names

Owns stable personal identities, original given names, chosen-name history, names heard from particular people, and each speaker's current nicknames, titles, and epithets. A nickname never changes its recipient's chosen identity or grants an office. Stable person IDs distinguish people who share a name.

`NameChild` asks a present parent or caregiver to name an unnamed child. `SuggestChild` lets a higher decision owner supply that person's choice; without it, the caregiver's own name is used. `Change` records a self-chosen name and announces it locally. Distant acquaintances retain their previous memory. `Label` records a speaker's form of address; a nickname used in the recipient's presence becomes a suggestion they can consider adopting. Each speaker retains one current address of each kind per person.

Social life owns the public name projection and emits registration and rename events. `NameFor` supplies observer-specific names to social observations, including old memories and private usage. Knowing someone's ID does not reveal their name. Saves validate identity history, heard names, and usage; invalid changes roll back atomically.

Identities share immutable records and name-history entries across transaction checkpoints. Appends validate their new suffix when the retained prefix shares the original entries; replacements and loaded worlds receive full validation. Histories retain every recorded change and serialize as ordinary arrays.

When communication is enabled, chosen names require understandable presentation before another person learns them. A silent rename can remain private, and deaf observers can retain an older name. The person choosing a child's name already knows their own choice and can present it on the child's behalf. Identity storage and personally heard names remain separate.
