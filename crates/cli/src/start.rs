pub(super) use histeries_sim::frontier;

#[cfg(test)]
mod tests {
    use super::*;
    use histeries_hex::FlatHex;
    use histeries_rules_core::PlayerId;
    use histeries_sim::MapState;

    #[test]
    fn frontier_and_json_scenario_batch_use_normal_dispatch_and_preserve_input_on_failure() {
        let map = MapState::blank(FlatHex::new(7, 3, false, false).unwrap());
        let game = frontier(map, 2).unwrap();
        for seat in [PlayerId::new(1), PlayerId::new(2)] {
            assert_eq!(game.observe(seat).unwrap().cities().len(), 1);
            assert_eq!(game.observe(seat).unwrap().units().len(), 1);
        }
        let dir =
            std::env::temp_dir().join(format!("histeries-scenario-batch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let state = dir.join("game.hst");
        let source = dir.join("orders.json");
        std::fs::write(&state, game.state_snapshot().encode()).unwrap();
        let run = || {
            crate::run(vec![
                "scenario-editor".into(),
                state.display().to_string(),
                "apply".into(),
                "--commands".into(),
                source.display().to_string(),
            ])
        };
        let marker = serde_json::json!({"command":"editor.set_marker","player":1,"value":23});
        let original = std::fs::read(&state).unwrap();
        std::fs::write(
            &source,
            serde_json::to_vec(&serde_json::json!([marker,{"command":"turn.end","player":42}]))
                .unwrap(),
        )
        .unwrap();
        assert!(run().is_err());
        assert_eq!(std::fs::read(&state).unwrap(), original);
        std::fs::write(
            &source,
            serde_json::to_vec(&serde_json::json!([marker])).unwrap(),
        )
        .unwrap();
        run().unwrap();
        assert_eq!(
            crate::read_state(state.to_str().unwrap())
                .unwrap()
                .marker(PlayerId::new(1)),
            Some(23)
        );
        for file in [state, source] {
            std::fs::remove_file(file).unwrap();
        }
        std::fs::remove_dir(dir).unwrap();
    }
}
