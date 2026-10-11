# Ecology

`game-ecology` models finite natural deposits, undeveloped plots, and gathering. Resources and plots are plain components in the host's shared store; `Runtime` retains rules, gathering jobs, and dated personal observations.

Daily systems match each resource or plot with its area-local weather. They renew finite stocks, track moisture and water contamination, and recover undeveloped soil. Missing weather leaves those rows unchanged; gathering remains available.

Preparation produces stable ordered row changes that a supplied executor can evaluate in parallel. Commit checks opening rows and membership before updating components and the consumed day. It never replaces jobs, knowledge, or foreign components. Gathering uses explicit body, capacity, equipment, permission, and contact facts and returns material effects for their owners to admit.

Plots contain land-use metadata; their soil is the shared `game-land` component on the same entity. Daily recovery joins Plot, Soil, and local weather, then changes only Soil. Removing Plot retains the soil for cultivation. Deposit moisture remains part of each natural resource.
