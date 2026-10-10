# game-health

`game-health` adds bodily needs, illness, care, and shelter rules to the game. It tracks thirst and cold exposure, accumulates enteric and respiratory infection, and models symptom timing, recovery, and treatment. Water lots retain contamination through collection, transfer, consumption, and purification; shelters track insulation, capacity, occupancy, and acknowledged access.

The crate owns health rules, domain state, and calculations. The simulation adapter supplies environmental and contact facts, checks actual people and inventories, and coordinates actions such as drinking, boiling, building, and treatment. Health mechanics do not fetch supplies or perform those actions themselves.
