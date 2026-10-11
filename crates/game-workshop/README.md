# game-workshop

`game-workshop` models production at a site. A recipe names its input stock and amount, required tool, output stock and amount, and work duration. Starting requires an able adult with site access, usable unreserved input, an available tool, and output space. It returns the admitted job and input consumption for the host to apply together.

Jobs preserve their rates and fractional progress across pauses; work advances only while active and equipped. A completed job produces only while equipped and when output space remains available. Guarded commit updates the actual output stock, clears the job, and records producer attribution once.

The crate registers plain `Site` and `Job` components in the host's shared store; it owns no second store and exposes no engine-specific types. Typed `SiteId` access and insertion keep recipe declarations on site entities. Site validation checks its own identifiers, duration, quantities, and access identifiers; the host checks stock references, areas, and permission facts. Each worker's `Job` retains paused progress. Completion validates the exact job, site, and output before removing the job component and publishing production.
