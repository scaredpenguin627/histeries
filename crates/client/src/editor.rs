//! A local editor pairs the simulation draft with its independent presentation state.
use crate::presenter::editor::EditorPresenter;
use eframe::egui::Ui;
use histeries_sim::ScenarioDraft;

pub(crate) struct Editor {
    draft: ScenarioDraft,
    presenter: EditorPresenter,
}

impl Editor {
    fn new(draft: ScenarioDraft, source: String) -> Self {
        let presenter = EditorPresenter::new(draft.seats()[0], source);
        Self { draft, presenter }
    }

    pub(crate) fn blank(width: u32, height: u32, seats: u32) -> Result<Self, String> {
        Ok(Self::new(
            ScenarioDraft::blank(width, height, seats)?,
            "Blank map".into(),
        ))
    }

    pub(crate) fn open(bytes: Vec<u8>, source: String) -> Result<Self, String> {
        Ok(Self::new(ScenarioDraft::open(&bytes)?, source))
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        self.draft.bytes()
    }

    pub(crate) fn apply_package(
        &mut self,
        package: &histeries_sim::extensions::Package,
    ) -> Result<usize, String> {
        self.draft.apply_package(package, self.presenter.owner())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn set_source(&mut self, name: String) {
        self.presenter.set_source(name);
    }

    pub(crate) fn show(&mut self, root: &mut Ui, menu: impl FnOnce(&mut Ui)) {
        self.presenter.show(root, &mut self.draft, menu);
    }
}

#[cfg(test)]
mod tests {
    use super::ScenarioDraft;
    use crate::game::ClientSession;
    use histeries_hex::TileId;
    use histeries_sim::{Feature, ResourceKind, RiverDirection, ScenarioBrush, Terrain, UnitKind};

    #[test]
    fn edit_a_hidden_tile_then_open_the_same_scenario_for_play() {
        let mut draft = ScenarioDraft::blank(8, 4, 2).unwrap();
        let far = TileId::new(31);
        let seat = draft.seats()[0];
        let other = draft.seats()[1];
        assert_eq!(draft.board().visible_tiles().len(), 32);
        assert!(
            ClientSession::from_save_bytes(draft.bytes())
                .unwrap()
                .observation()
                .visible_tiles()
                .is_empty()
        );
        draft
            .apply(ScenarioBrush::Terrain(Terrain::Grassland), far, other)
            .unwrap();
        draft
            .apply(ScenarioBrush::Feature(Feature::Forest), far, other)
            .unwrap();
        draft
            .apply(
                ScenarioBrush::Resource(Some(ResourceKind::Iron)),
                far,
                other,
            )
            .unwrap();
        draft
            .apply(
                ScenarioBrush::River(Some(RiverDirection::NorthWest)),
                far,
                other,
            )
            .unwrap();
        draft.apply(ScenarioBrush::City, far, other).unwrap();
        draft
            .apply(ScenarioBrush::Unit(UnitKind::Builder), TileId::new(1), seat)
            .unwrap();
        draft
            .apply(ScenarioBrush::Road, TileId::new(1), seat)
            .unwrap();
        draft
            .apply(ScenarioBrush::City, TileId::new(0), seat)
            .unwrap();
        let loaded = ScenarioDraft::open(draft.bytes()).unwrap();
        assert_eq!(loaded.board(), draft.board());
        assert!(loaded.board().visible_tiles()[1].road);
        let target = &loaded.board().visible_tiles()[31];
        assert_eq!(
            (
                target.terrain,
                target.feature,
                target.resource,
                target.river
            ),
            (
                Terrain::Grassland,
                Feature::Forest,
                Some(ResourceKind::Iron),
                Some(RiverDirection::NorthWest)
            )
        );
        let mut playable = ClientSession::from_save_bytes(draft.bytes()).unwrap();
        assert!(playable.embedding(far).is_none());
        assert_eq!(playable.observation().cities().len(), 1);
        playable.end_turn().unwrap();
        assert_eq!(playable.observation().turn(), 1);
        assert_eq!(draft.board().turn(), 0);
    }
}
