# game-transport

This crate defines travel and carrying rules over supplied bodies, positions, and simulation times. It tracks road journeys, mid-road positions, and roadside approaches. Route selection uses personally known roads and stages real movement through endpoints, including returns and roadside destinations. Carry groups model people carrying dependent or incapacitated passengers: bearers make personal offers, and lifts require valid permission, willing bearers, shared position, and enough capacity for the passenger and their cargo.

Load is divided by each bearer's remaining capacity; overloaded groups cannot move. The rules also cover newborn attachment, stopping and resuming travel, and responses to offers of help. This crate handles physical transport of people, not general hauling for the economy.

Transport selects local, unexpired carry offers and judges them from supplied bodily needs and remembered relations. It returns physical actions or a request for more support, owns bounded response and support memories, and selects willing helpers. Selection is read-only; executing an intention still checks current permission, contact, and load capacity. Cheap admission checks avoid assembling carrying facts for irrelevant offers and requests.
