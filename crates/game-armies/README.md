# game-armies

`game-armies` owns coordinated command relationships among existing missions, local communication queues, personal army knowledge, autonomy, reinforcement approaches, and their validation. Each mission retains its named leader and members. Leaders choose whether to join, accept orders, retreat, or defect; distant knowledge changes only through dated communication.

`register_components` installs `Army`, personal `Knowledge`, `Talk`, `Queue`, and `Approach` in the shared component store. `World` retains only rules and review timing. `Kernel` borrows canonical rows for prepared relationship changes. `PreparedChange::stage` and cancellation staging return generic ECS read guards for composed transactions.

`Systems` exposes owner behavior independently of the assembly. Its `Ports` supply borrowed native mission, body, location, route and encounter facts, and sequence prepared Army and mission effects in one transaction. Army selection and progression stay in this crate; mission directives and departures use mission-owned preparation. The assembly registers schemas, builds fixtures, supplies factual ports, and binds public commands and views.
