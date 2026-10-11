# Ecology

`game-ecology` models finite natural deposits, undeveloped plots, and gathering. Resources and plots are plain components in the host's shared store; `Runtime` retains rules and the consumed day. Optional personal `Gathering`, `Knowledge`, and `Requests` components are registered under `ecology/gathering`, `ecology/knowledge`, and `ecology/requests`. Knowledge holds dated resource and plot views plus inspected areas; requests hold that person's resource cooldowns. Typed ports read and change these columns without replacing other owners' rows.

Daily systems match each resource or plot with its area-local weather. They renew finite stocks, track moisture and water contamination, and recover undeveloped soil. Missing weather leaves those rows unchanged; gathering remains available.

Preparation produces stable ordered row changes that a supplied executor can evaluate in parallel. Commit checks opening rows and membership, then admits resource and soil changes in one typed batch before updating the consumed day. It never replaces gathering, knowledge, requests, or foreign components. Gathering uses explicit body, capacity, equipment, permission, and contact facts and returns material effects for their owners to admit. Acquisition and exploration read the person's dated knowledge; current resource state does not manufacture remote observations.

Plots contain land-use metadata; their soil is the shared `game-land` component on the same entity. Daily recovery joins Plot, Soil, and local weather, then changes only Soil. Removing Plot retains the soil for cultivation. Deposit moisture remains part of each natural resource.
