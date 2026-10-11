# game-characters

`game-characters` adds personal decisions and social development to the game. It models dispositions, remembered relationships and aims, and ranks choices from information a person knows. People can form companionship, talk about family plans, offer instruction, learn, and practice; experience can shape their aspirations.

The crate owns minds, plans, choice scores, retries, memory, experience, and social choices. Providers describe expected relief, development, provision, or fulfilment, along with effort, uncertainty, and risk. Characters judge that evidence using their own dispositions and experience. An intention can continue across different actions and providers.

Executable actions are opaque values supplied with focused observations; the world adapter binds those choices to actual actions. Biological aging and reproduction live in `game-lifecycle`.

The `temperament` module blends configured inherited predispositions with character baselines and bounded experience. Definitions select the influenced traits and experience changes. The optional `genetics` feature connects those definitions to raw inherited alleles.

Temperament `Runtime` retains only definitions. `PersonTemperament` lives in the host's shared component store on each person's stable entity, under `characters/temperament`; the module owns registration, typed access, and intrinsic validation for that column. Expression and learning take the store explicitly. Save the runtime with the shared store snapshot, restore the composed schema, and run the runtime's store-aware validation for content-dependent bounds. Optional inheritance reads the opaque `AlleleSource` interface; learning never changes genomes.
