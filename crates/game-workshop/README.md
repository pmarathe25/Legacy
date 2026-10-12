# game-workshop

Runs production from real input stocks, tools, workers, and output space. Sites select a recipe ID; the saved recipe catalog supplies its input, output, and work duration. Normal and accelerated fixture catalogs remain separate.

Work starts by consuming the selected inputs. Jobs accumulate progress, pause when requirements fail, and publish output through guarded component and stock changes. Recipe definitions can be edited or added in TOML without changing production logic.

Workshop chooses a resumed tool, the site's declared tool, or a nearby alternate in that order. Small factual ports supply physical availability, nearby inventory IDs and access; Cargo admits the choice against other workers and any recipe consumption of tool stock.
