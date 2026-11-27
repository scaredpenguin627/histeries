//! Shared topology-to-display projection and visible tile lookup.
use histeries_hex::{Embedding, TileId};
use histeries_sim::{BoardObservation, Observation, VisibleTileObservation};

pub(crate) const HALF_WIDTH: f32 = 0.866_025_4;

pub(crate) fn world(point: Embedding) -> emath::Vec2 {
    emath::vec2(point.x as f32 * HALF_WIDTH, point.y as f32 * 0.5)
}

pub(crate) fn visible(view: &BoardObservation, id: TileId) -> Option<&VisibleTileObservation> {
    let tiles = view.visible_tiles();
    tiles
        .binary_search_by_key(&id, |tile| tile.tile)
        .ok()
        .map(|i| &tiles[i])
}

pub(crate) fn home(view: &Observation) -> Option<TileId> {
    view.cities()
        .first()
        .map(|c| c.tile)
        .or_else(|| view.units().first().map(|u| u.tile))
        .or_else(|| view.visible_tiles().first().map(|t| t.tile))
}
