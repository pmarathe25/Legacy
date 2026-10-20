# game-physics

Reusable force and travel budgets for rigid transmission graphs, resolved material
properties, friction losses, load feedback, and overload limits. Sources and loads
use generalized game units. Work cannot increase through a component or a branch.
The current model is quasi-static and acyclic; it does not model inertia or fluids.

`Measurement` holds the perceived ratio, travel, and load of an experiment.
`MechanicalReading` retains those values plus its input force and travel. These
numeric records let memory retain complete experimental evidence without an
expanded JSON object; unknown extension fields keep their original representation.
