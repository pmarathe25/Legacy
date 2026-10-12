# Ecology

`game-ecology` models finite natural deposits, undeveloped plots, and gathering. Resources and plots are plain components in the host's shared store; `Runtime` retains rules and the consumed day. Optional personal `Gathering`, `Knowledge`, and `Requests` components are registered under `ecology/gathering`, `ecology/knowledge`, and `ecology/requests`. Knowledge holds dated resource and plot views plus inspected areas; requests hold that person's resource cooldowns. Typed ports read and change these columns without replacing other owners' rows.

Daily systems match each resource or plot with its area-local weather. They renew finite stocks, track moisture and water contamination, and recover undeveloped soil. Missing weather leaves those rows unchanged; gathering remains available.

Preparation produces stable ordered row changes that a supplied executor can evaluate in parallel. Commit checks opening rows and membership, then admits resource and soil changes in one typed batch before updating the consumed day. It never replaces gathering, knowledge, requests, or foreign components. Gathering uses explicit body, capacity, equipment, permission, and contact facts and returns material effects for their owners to admit. Acquisition and exploration read the person's dated knowledge; current resource state does not manufacture remote observations.

Plots contain land-use metadata; their soil is the shared `game-land` component on the same entity. Daily recovery joins Plot, Soil, and local weather, then changes only Soil. Removing Plot retains the soil for cultivation. Deposit moisture remains part of each natural resource.

Local observation groups supplied actor-area facts and forms personal resource and plot views from actual local rows and soil. It retains dated remote memories and the latest sixteen inspected areas. Prepared observations guard local land and each observer's knowledge, then publish one atomic batch; the assembly supplies physical eligibility and location facts.

Local opportunity systems select the first sixteen sources and validate suggested gathering amounts against real stock, workspace, tools, and time. Narrow callbacks supply quantity suggestions, custodian locations, and route reachability. Autonomous choices use personal source knowledge and remembered remote stock; discovering an access request remains separate from its execution cooldown.

Ecology supplies typed material acquisition offers to `game-logistics`, including required equipment, access requests and exploration. Local gathering uses physical admission facts; remote yields remain dated estimates. Logistics chooses acquisition and delivery steps through that interface without importing the playable assembly.

Prepared access transitions validate local custody and recipient facts, guard resources and request cooldowns, and return permission events. Collection completion reads committed source quality, guards its source and job, and atomically ends the job with any deposit reduction. Other owners admit material, water, and publication effects before commit.

Collection finishing prepares resource depletion and job removal without mutating them. The assembly combines these writes with admitted cargo, water, and activity effects; a full receiving compartment interrupts collection without debiting the natural source. Closing collection still uses the source's current quality after renewal.
