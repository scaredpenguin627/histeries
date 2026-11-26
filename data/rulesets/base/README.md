# Base ruleset

The base ruleset contains original units, technology and civic progression DAGs, ages, policy options, districts, buildings, and modifiers. All entries use the validated schema and designed provenance.

The optional `[[ages]]` tables follow progression order. The first age has no prerequisites. Each later age requires all entries in its `required_technologies` and `required_civics` lists. An age's `unlocks` list grants permanent capabilities such as `orbital_access`. Set `reset_science` or `reset_culture` to clear the corresponding unspent balance on entry. Both balances carry over by default. A policy's optional `required_age` sets the first age in which a seat may adopt it.

The `[mapgen]` table sets the continent count, target land percentage, and river source percentage. Each `[mapgen.land.<terrain>]` table gives a positive relative weight and optional `features` and `resources` tables. For example, `features = { forest = 12, marsh = 4 }` puts forest on 12% and marsh on 4% of eligible tiles on average. The remaining tiles have no feature. Water depth follows the coastline.

Pass a ruleset to `histeries new --map-ruleset PATH --output world.hst` to generate a world with its map profile. The simulation stores the resulting tiles in the save and replay. See [map generation](../../../specs/map-generation.md) for terrain eligibility, continent spacing and river drainage.
