# game-simple-machines

Defines elementary quasi-static mechanisms and linear/rotary port compatibility.
`game-physics` composes them into finite work graphs; `game-invention` searches
combinations rather than assuming every component is a lever.

| Mechanism | Dimensions | Ports | Ideal effort gain |
| --- | --- | --- | --- |
| Lever | input arm, output arm | linear → linear | input / output |
| Gear pair | driving teeth, driven teeth | rotary → rotary | driven / driving |
| Pulley tackle | supporting strands, 1 | linear → linear | strands |
| Wheel and axle | wheel radius, axle radius | linear → linear | wheel / axle |
| Inclined plane | slope length, rise | linear → linear | length / rise |
| Wedge | travel length, thickness | linear → linear | length / thickness |
| Screw | 6 (approximate radians per turn), lead | rotary → linear | 6 / lead |
| Crank | arm, 1 | linear → rotary | arm |
| Drum | 1, radius | rotary → linear | 1 / radius |

Rotary ports carry normalized torque and angle. Cranks and drums connect them to
linear force and travel. Effort gain reduces travel; material friction, finite
supports, overload, and branch sharing remain the physical owner's responsibility.
The model does not yet resolve spatial gear contact, rotation direction, inertia,
fluids, feedback loops, or continuous power sources.

Tests verify dimensions and ports; physics tests exercise a crank–gear–drum–tackle
assembly, energy conservation, missing rope, and incompatible connections.
