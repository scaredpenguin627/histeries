# Three-dimensional match view

- **designed** — The native desktop and WebGPU clients offer 2D and 3D views of the same game. The player can switch during play. A desktop launch with `--3d` opens the 3D view. The scenario authoring canvas retains its 2D tools.
- **designed** — Each visible hex takes its horizontal position from the observation's topology embedding. The client draws terrain relief, visible features and infrastructure, and city and unit models from the seat's observation. Height and lighting are presentation properties. Hidden tiles and pieces have no selectable or rendered geometry.
- **designed** — The 3D camera can pan, orbit, zoom and reframe the visible board. Picking a visible surface selects the tile that owns the nearest rendered triangle. An empty point has no tile. Selection and targeted orders use stable tile IDs.
- **designed** — When tiles occupy only a few pixels, the overview uses simpler geometry for terrain and pieces. Zooming in restores detailed models. Both detail levels pick from their own rendered surfaces.
- **designed** — Both views use the same inspector and submit orders to the existing seat controller. Switching views or moving the camera issues no game command. Saves, turn advancement and victory follow the existing session rules.
- **designed** — A hotseat handoff covers the scene before the next player's map is revealed. A LAN client uses its authenticated seat's view and existing permission to act.
