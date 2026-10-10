# game-workshop

`game-workshop` models production at a site. A recipe names its input stock and amount, required tool, output stock and amount, and work duration. Starting requires an able adult with site access, usable unreserved input, an available tool, and output space. It returns the admitted job and input consumption for the host to apply together.

Jobs preserve their rates and fractional progress across pauses; work advances only while active and equipped. A completed job produces only while equipped and when output space remains available. Guarded commit updates the actual output stock, clears the job, and records producer attribution once.
