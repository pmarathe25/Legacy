# game-characters

`game-characters` adds personal decisions and social development to the game. It models dispositions, remembered relationships and aims, and ranks choices from information a person knows. People can form companionship, talk about family plans, offer instruction, learn, and practice; experience can shape their aspirations.

The crate owns minds, plans, choice scores, retries, memory, experience, and social choices. Providers describe expected relief, development, provision, or fulfilment, along with effort, uncertainty, and risk. Characters judge that evidence using their own dispositions and experience. An intention can continue across different actions and providers.

Decision `Runtime` retains the concept registry, work concept, learning-event cursor, and switching policy. Personal `Mind<A>` is a plain shared-store component under `characters/mind`; executable actions remain opaque values supplied by composition. `register_minds<A>` and typed mind ports require only the backend-independent component contract for `A`. Intrinsic validation checks personal scalar ranges and local row structure, while composition validates current clocks, foreign catalogs, and executable action shape. Production learning changes the selected person's component directly and leaves other minds and columns intact.

Executable actions are opaque values supplied with focused observations; the world adapter binds those choices to actual actions. Biological aging and reproduction live in `game-lifecycle`.

The `temperament` module blends configured inherited predispositions with character baselines and bounded experience. Definitions select the influenced traits and experience changes. The optional `genetics` feature connects those definitions to raw inherited alleles.

Temperament `Runtime` retains only definitions. `PersonTemperament` lives in the host's shared component store on each person's stable entity, under `characters/temperament`; the module owns registration, typed access, and intrinsic validation for that column. Expression and learning take the store explicitly. Save the runtime with the shared store snapshot, restore the composed schema, and run the runtime's store-aware validation for content-dependent bounds. Optional inheritance reads the opaque `AlleleSource` interface; learning never changes genomes.

Prepared temperament systems consume declared experience IDs in order, apply the content's bounded offsets, and express the current personality through opaque allele values. Unknown IDs produce no learning. Commit validates trait, personality, allele, and definition guards, admits trait rows, and changes only the mind's personality field. Water knowledge, plans, memory, and other mind data stay with their owning systems and retain any intervening updates.

Resource responses weigh permission requests and unauthorized use from the person's personality and relationship. Physical and control owners supply readiness; this crate decides whether to grant, refuse, resist, or ignore.
