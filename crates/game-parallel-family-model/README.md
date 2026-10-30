# Parallel family model

Data contracts for family members, pregnancy and physical life/birth rows; family behavior and validation remain with the family owner.

Parent IDs remain a serialized list; at runtime the two-parent bound is stored inline so copying valid pedigrees does not allocate per person.
