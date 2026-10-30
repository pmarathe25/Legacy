# Parallel block CLI



Run `cargo run -p game-parallel-block-cli -- content/parallel/autowork.toml`; `help` lists the administrative commands. Saves contain the committed editable world and are recompiled on load. This experimental client does not yet provide the full ruler-bound game interface.



Run with `--player content/parallel/guardians.toml` for the restricted player view. `observe` displays the ruler's current location and remembered reports; `spectate-guardian ID` explicitly follows a guardian searching for the ruler. Player commands may only control the current ruler. Administrative inspection and edits are unavailable in this mode. The administrative mode remains useful for fixtures and diagnostics.



The administrative `disease` command inspects infections and current events when disease is enabled. In player mode, `observe` reports noticeable illness symptoms only for the ruler.



The `agreements`, `agreement-propose`, `agreement-decide`, and `agreement-deliver` commands manage delivery promises and their physical fulfillment. Player mode limits these commands to the current ruler.



`opinions PERSON` shows that personâ€™s directed trust records; player mode restricts this inspection to the ruler.


The ruler can use `mission-invite PERSON MISSION TARGET` to ask an available local person to join a ready expedition. The person's social willingness, condition, and personality determine acceptance; an accepted invitation creates a real companion expedition and links the party. Invitations are immediate setup decisions and do not themselves advance the simulation tick.





The `invoked-skills` scenario demonstrates borrowing a past life's production ability. `observe` shows only skills from active invocations, and dismissal removes those temporary abilities.


`teach PERSON TARGET RECIPE` offers a lesson through a local social encounter. An invoked past life can supply the teacher's temporary skill; the student retains the lesson after invocation ends. The `teaching` scenario demonstrates this path.

`farm-status PERSON` and `farm-policy PERSON enable|disable` inspect and change voluntary farm participation when configured. `mission-meet PERSON ID TARGET` sets a destination meeting goal; `--player --scenario meeting-expedition` demonstrates the player initiating the meeting.

Try a material-backed store in the administrative CLI with `cargo run -p game-parallel-block-cli -- content/parallel/store_building.toml`, then `pickup-good 1 1 3`, `store-build 1 10 7`, and `food-stores 1`.

People with a configured construction intention can change it with `store-building PERSON enable|disable`. They use their own held material, and no store appears if they lack it. `store-claim PERSON STORE assert|withdraw`, `store-objection PERSON STORE enable|disable`, and `deposit-claim PERSON none|accept|object` manage that person's individual claims and tendencies.

`reproduction PERSON enable|disable` changes that person’s intention to conceive. Either member can use `leave-partnership PERSON` without the other person’s permission. A previously established pregnancy continues independently.

`partnership-preference PERSON enable|disable` changes an individual's tendency to reconsider a reproductive partnership when it no longer matches their goals.

In administrative mode, `catalog-goods` and `catalog-recipes` inspect the numeric production definitions imported from content.

`eat-good PERSON GOOD` consumes an authored portion of an edible good held by that person. Autonomous people can also eat such held goods when hungry, subject to their existing automatic eating preferences. Authored health effects occur on the next tick.
