# Workspace replacement

The workspace uses the new `game-sim` host and feature-owned requests throughout. The former assembly kernels, ECS adapter path, port adapters, monolithic society crate, were removed. Authored content now lives in the restored `content/` store using the current feature schemas. No compatibility layer remains.

Relationships, knowledge, institutions, justice, guardians, confinement, object definitions, trait definitions, and scenario definitions have separate crates. `game-assembly` contains only plugin registration, API binding, and the Simulation facade. [The crate index](crates/README.md) documents the current dependencies.

The pre-rewrite working tree, including uncommitted and untracked crate files, is preserved at `/tmp/legacy-before-rewrite-1791992308.tar.gz`. Git history also retains the previously committed implementation. The design documents and original modularization proposal were preserved.

This replacement establishes independently testable mechanics and headless fixtures. It does not preserve every earlier gameplay behavior or complete the campaign design. Existing saves cannot be loaded into the new schema. [The README](README.md) describes the current contracts and limitations; executable tests establish implemented behavior.
