//! Exercise the tournament's public output with a turn-limit draw in both seats.
#![cfg(not(target_family = "wasm"))]

use std::process::Command;

#[test]
fn capped_v1_match_reports_both_draws_and_one_tied_seed() {
    let output = Command::new(env!("CARGO_BIN_EXE_histeries"))
        .args([
            "tournament",
            "--opponent",
            "v1",
            "--seed",
            "17",
            "--seeds",
            "1",
            "--turns",
            "1",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = String::from_utf8(output.stdout).unwrap();
    for (seat, line) in [1, 2].into_iter().zip(report.lines().skip(1)) {
        assert!(line.starts_with(&format!(
            "seed=17 candidate_seat={seat} turns=1 rounds_planned=1 hash="
        )));
        assert!(line.ends_with("outcome=draw"));
    }
    assert!(report.contains("games=2 wins=0 losses=0 draws=2 errors=0 crashes=0"));
    assert!(report.contains("win_bp=0 planning_round_p95_upper_us="));
    assert!(report.contains(
        "player=v1 science_wins=0 science_win_bp=0 domination_wins=0 domination_win_bp=0"
    ));
    assert!(report.contains("elo_delta_smoothed=0 paired_better=0 paired_worse=0 paired_tied=1 paired_sign_p_one_sided=1.00000000e0"));
}
