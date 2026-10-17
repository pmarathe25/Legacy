# Developer fixtures

`content/` here extends the production content pack with authored starting fixtures and accelerated physiology, decision, and social timing. These files support short tests and interactive debugging. They do not set the generated game's rules.

`Store::standard()` reads production content only. `Store::fixtures()` explicitly adds this pack. The CLI enables it for `--list` and explicit fixture selection; generated starts use the production store. Custom packs can opt in with `--mod tools/fixtures/content`.

The accelerated profiles are named `fixture.toml`. Normal human-scale rules have one source per feature under the repository's `content/*/default.toml` files.
