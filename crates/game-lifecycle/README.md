# game-lifecycle

`game-lifecycle` owns biological aging and reproduction. It tracks life stages, pregnancy, parents, and configurable developmental deadlines. Conception checks adulthood, compatible reproductive roles, physical proximity, and mutual consent from explicit body and choice facts.

Prepared transitions decide conception completion, pregnancy loss, birth, aging, and death, rejecting stale records before writing. Birth effects describe an actual newborn and its location; the world creates its body, genes, and inventory. Caregiving and social arrangements follow people's separate choices rather than a prescribed family model.

`Runtime` retains only lifecycle rules. Personal `Life` and `Pregnancy` are plain components in the host's shared store, registered under `lifecycle/life` and `lifecycle/pregnancy`. Typed ports access those columns, and intrinsic validation checks component structure. Save the runtime with the shared store snapshot, restore the composed schema, then call `validate_runtime` and `Runtime::validate_with_facts` for current time, parent references, bodies, and scheduled deadlines.

Runtime row methods take the store explicitly. `PreparedLifecycle` captures its rules and exact rows, including the relevant active gestation membership. Commit rechecks those inputs and publishes all lifecycle edits through one typed batch before returning phase effects. A rejected birth cannot publish only the child or only the pregnancy record, and their stable entity IDs must differ.

The `heredity` module interprets configured allele combinations as reproductive roles. Genetics only transmits those combinations; lifecycle retains their biological meaning. This integration is available through the optional `genetics` feature.

`prepare_lifespan_change` evaluates an explicit accepted lifespan penalty and physical survival fact. Its guarded response retains the opening life row, rules, and clock; final composition rejects foreign life-row changes before publishing the cost. A living person yields death or its next age deadline. An already dead body yields no response and retains the permanent penalty.
