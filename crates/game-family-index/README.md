# Family identity index

Provides deterministic dense and sparse physical and family identity joins.

Validated sorted rows can be borrowed without rebuilding a reference table. A contiguous suffix uses direct indexing; other IDs use binary search, so gaps remain supported.
