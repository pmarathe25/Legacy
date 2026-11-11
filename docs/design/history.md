# Long-running world history

The intended long-run use includes advancing a world for centuries, for example 300 simulated years, and examining how its people and living populations change. This is a design target, not a claim that the current simulation can yet produce or analyze such a history. A scenario must eventually define the relationship between simulation time and the calendar; do not assume a year length from a benchmark.

## People and collective patterns

Preserve searchable histories for individuals, including their actions, relationships, experiences, skills, changing circumstances, family links, and deaths where those facts are modeled. Support following an individual over time and finding people who became influential, without requiring the simulation to designate them as notable in advance.

Identify leaders, organizations, corporations, political formations, and civilizations as interpretations of persistent patterns in autonomous people, interactions, shared practices, and material activity. Their membership, boundaries, names, and periods may be uncertain or disputed. These descriptions must not confer authority, create automatic employment or payroll, bind agreements, or require cooperation to be consensual. People may follow, cooperate, resist, deceive, or change their behavior for their own reasons. Expose the underlying observations and the criteria used for a historical interpretation.

## Knowledge, technology, and living populations

Track modeled observations, claims, discoveries, skills, and practices as they are created, communicated, learned, revised, forgotten, or used. Support histories of scientific and technological change when mechanisms for discovery, testing, transmission, and application exist; a newly named concept alone is not evidence of progress. Historical views should distinguish a claim from a verified outcome and show whose knowledge changed.

Track ancestry, inherited traits, births, survival, and reproduction for people and other modeled organisms. Where character choices affect breeding or selection, record the relevant decisions and outcomes separately from differential survival and reproduction without those choices. Report population-level changes alongside individual lineages, and avoid inferring causation from trait frequencies alone.

## Storage and performance

Keep simulation state needed for decisions separate from an append-oriented historical record and derived indexes. Store durable, time-stamped causal events and periodic snapshots outside hot actor loops; build timelines and population summaries in parallel or incrementally. Avoid retaining every routine update in active memory indefinitely. Long-term storage, summaries, and interpretations must not change character decisions or invent authoritative institutions.

Before treating a centuries-long run as representative, define its initial conditions, calendar, scale, observation rules, and seed; record versioned content and configuration. Test save/resume, replay or equivalent reproducibility, bounded active memory, growing-history storage, and realistic throughput over long durations. Include mixed economic, social, demographic, ecological, and conflict workloads rather than extrapolating from isolated short benchmarks.
