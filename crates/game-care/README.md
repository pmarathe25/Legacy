# game-care

**Purpose:** Personal concern can cause someone to treat a nearby injured person using [health's](../game-health/README.md) real medicine, time, and healing rules.

`Enable` activates `Consider` during `IdleReview`. A capable, idle person checks permitted local medicine and considers their own wounds plus up to 64 locally enrolled neighbors in a rotating search. Expressed altruism and empathy, learned concern, and that person's own affection or grievance determine willingness to help; other people's beliefs and private inventories are not consulted. The current player receives no automatic caregiver action.

`Care` tracks caregiver–patient assignments so automatic caregivers do not duplicate treatment. Completion or cancellation releases the assignment. Successful treatment can improve the patient's own impression of the caregiver and strengthen the caregiver's learned value for care. Death, patient movement, canceled work, and exhausted supplies retain the health owner's consequences. Treatment occupies only the caregiver; a patient can leave.

This owner adds no scheduler or damage rules. Health remains usable directly without it. Generated history enables automatic care; explicit successful treatment also produces personal learning through the health completion event. Consent negotiations for treatment, diagnosis, diseases, and medical establishments remain separate mechanics.

**Verification:** `cargo test -p game-care` checks physical costs, local concern, grievance, player control, cancellation, gratitude, and save/load across workers.

Local adults can notice a dependent and their deceased caregiver in the same place. This requires local physical evidence; distant deaths remain unknown. Personal attachment and empathy compete with hunger, fatigue, and existing care responsibilities; they can decline. A willing adult asks households to accept responsibility through `Foster`, then ordinary household feeding, transport, and enrollment apply. No distant orphan registry or automatic population replacement supplies candidates.

Food care is composed through [game-nurture](../game-nurture/README.md) within this owner. `Assist` starts from co-located need; household-owned intentions track physical acquisition and delivery. Travel and harvest time enter source comparison, so ready food can be preferred over another harvest. Recipient assent, finite quantities, and ordinary action costs still apply. Action completion prompts reconsideration; concern may lose to other needs or commitments.
