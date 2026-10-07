//! Manual release-only measurements, never a timing-based correctness gate.
//! cargo test --release --lib gui::regex_tester_dialog::profiling::profile_interactive_workloads -- --ignored --exact --nocapture

use super::*;
use crate::regex_tester::session::EVALUATION_DEBOUNCE;
use crate::regex_tester::{
    EvaluationPolicy, RegexDraft, RegexFlags, evaluate_substitution_with_policy,
    evaluate_with_policy,
};
use std::time::Duration;

const SAMPLES: usize = 5;

fn measure(label: &str, mut sample: impl FnMut() -> Duration) {
    let _ = sample(); // explicit warm-up, excluded from measured samples
    let mut samples: Vec<_> = (0..SAMPLES).map(|_| sample()).collect();
    samples.sort();
    println!(
        "{label}: median={:.3}ms slowest={:.3}ms n={SAMPLES}",
        samples[SAMPLES / 2].as_secs_f64() * 1000.0,
        samples[SAMPLES - 1].as_secs_f64() * 1000.0
    );
}

fn sized(seed: &str, bytes: usize) -> String {
    let mut text = seed.repeat(bytes.div_ceil(seed.len()));
    let mut end = bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text
}

fn frame(
    dialog: &mut RegexTesterDialogState,
    ctx: &egui::Context,
    size: egui::Vec2,
    events: Vec<egui::Event>,
) {
    let output = ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                dialog.body(ui);
            });
        },
    );
    std::hint::black_box(output);
}

fn settled(draft: &RegexDraft) -> RegexTesterDialogState {
    let mut dialog = RegexTesterDialogState::default();
    dialog.session.draft = draft.clone();
    let now = Instant::now();
    dialog.session.mark_changed(now);
    dialog.session.tick(now + EVALUATION_DEBOUNCE);
    dialog
}

#[test]
#[ignore = "manual release profiling; no timing assertions"]
fn profile_interactive_workloads() {
    println!(
        "Profile: release={} OS={} ARCH={} CPU={} logical={} fixed samples={SAMPLES}",
        !cfg!(debug_assertions),
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::env::var("PROCESSOR_IDENTIFIER").unwrap_or_default(),
        std::env::var("NUMBER_OF_PROCESSORS").unwrap_or_default()
    );
    println!(
        "Policy: {:?}; normal pattern/replacement1KiB, input16KiB",
        EvaluationPolicy::default()
    );
    println!(
        "Evaluation includes compilation; session tick includes first accepted explanation. Egui frames use already accepted sessions. Paste excludes setup/warm font frame but includes copying/hashing/undo."
    );
    let mut workloads = Vec::new();
    for bytes in [16 * 1024, 64 * 1024] {
        workloads.push((
            format!("log-{bytes}"),
            RegexDraft {
                pattern: r"(?m)user=(?P<user>\w+) count=(?P<count>\d+)".into(),
                test_text: sized("2026-10-07 INFO user=alice count=42 status=ok\n", bytes),
                ..Default::default()
            },
        ));
        workloads.push((
            format!("unicode-line-{bytes}"),
            RegexDraft {
                pattern: r"(?P<word>é🦀漢)+".into(),
                test_text: sized("é🦀漢", bytes),
                ..Default::default()
            },
        ));
    }
    for pattern in [".", ""] {
        workloads.push((
            format!("dense-{}", if pattern.is_empty() { "empty" } else { "dot" }),
            RegexDraft {
                pattern: pattern.into(),
                test_text: "a".repeat(64 * 1024),
                ..Default::default()
            },
        ));
    }
    let alternation = format!(
        "(?:{})",
        (0..580)
            .map(|index| format!("p{index:05}"))
            .collect::<Vec<_>>()
            .join("|")
    );
    workloads.push((
        "near-4KiB-alternation".into(),
        RegexDraft {
            pattern: alternation,
            test_text: "p00001".into(),
            ..Default::default()
        },
    ));
    workloads.push((
        "100-captures".into(),
        RegexDraft {
            pattern: "(a)".repeat(100),
            test_text: "a".repeat(100),
            ..Default::default()
        },
    ));
    workloads.push((
        "materialization-limit".into(),
        RegexDraft {
            pattern: format!("{}.*{}", "(".repeat(100), ")".repeat(100)),
            test_text: "a".repeat(64 * 1024),
            ..Default::default()
        },
    ));
    for (name, draft) in workloads {
        println!(
            "CASE {name}: pattern={}B input={}B result={:?}",
            draft.pattern.len(),
            draft.test_text.len(),
            result_summary(&evaluate_with_policy(
                &draft.pattern,
                &draft.flags,
                &draft.test_text,
                &EvaluationPolicy::default()
            ))
        );
        measure(&format!("{name}/compile-only"), || {
            let start = Instant::now();
            let _ = std::hint::black_box(crate::regex_tester::engine::compile_regex(
                &draft.pattern,
                &draft.flags,
            ));
            start.elapsed()
        });
        measure(&format!("{name}/evaluate-including-compile"), || {
            let start = Instant::now();
            std::hint::black_box(evaluate_with_policy(
                &draft.pattern,
                &draft.flags,
                &draft.test_text,
                &EvaluationPolicy::default(),
            ));
            start.elapsed()
        });
        measure(&format!("{name}/session-tick"), || {
            let mut session = RegexSession::default();
            session.draft = draft.clone();
            let now = Instant::now();
            session.mark_changed(now);
            let start = Instant::now();
            std::hint::black_box(session.tick(now + EVALUATION_DEBOUNCE));
            start.elapsed()
        });
        for size in [egui::vec2(960.0, 680.0), egui::vec2(360.0, 240.0)] {
            measure(&format!("{name}/egui-cold-{}x{}", size.x, size.y), || {
                let mut dialog = settled(&draft);
                let ctx = egui::Context::default();
                let start = Instant::now();
                frame(&mut dialog, &ctx, size, Vec::new());
                start.elapsed()
            });
            let mut dialog = settled(&draft);
            let ctx = egui::Context::default();
            frame(&mut dialog, &ctx, size, Vec::new());
            measure(&format!("{name}/egui-warm-{}x{}", size.x, size.y), || {
                let start = Instant::now();
                frame(&mut dialog, &ctx, size, Vec::new());
                start.elapsed()
            });
        }
    }
    let flags = RegexFlags::default();
    let text = "a".repeat(600);
    let replacement = "X".repeat(4000);
    let policy = EvaluationPolicy::default();
    println!(
        "CASE substitution-output-limit: {:?}",
        evaluate_substitution_with_policy("a", &flags, &text, &replacement, &policy)
    );
    measure(
        "substitution-output-limit/evaluate-including-compile",
        || {
            let start = Instant::now();
            std::hint::black_box(evaluate_substitution_with_policy(
                "a",
                &flags,
                &text,
                &replacement,
                &policy,
            ));
            start.elapsed()
        },
    );
    let directory = tempfile::tempdir().unwrap();
    let history = directory.path().join("history.json");
    let presets = directory.path().join("presets.json");
    measure("configured-stores/open", || {
        let start = Instant::now();
        std::hint::black_box(RegexTesterDialogState::with_storage_paths(
            &history, &presets,
        ));
        start.elapsed()
    });
    let mut dialog = RegexTesterDialogState::with_storage_paths(&history, &presets);
    let mut index = 0;
    measure("configured-history/atomic-record", || {
        index += 1;
        dialog.session.draft.pattern = format!("profile_{index}");
        let now = Instant::now();
        dialog.session.mark_changed(now);
        dialog.session.tick(now + EVALUATION_DEBOUNCE);
        let start = Instant::now();
        dialog.record_history();
        start.elapsed()
    });
    measure("configured-history/unchanged-pair", || {
        let start = Instant::now();
        dialog.record_history();
        start.elapsed()
    });
    for (editor, id) in [
        (0, "regex_tester_pattern"),
        (1, "regex_tester_test_text"),
        (2, "regex_substitution_replacement"),
    ] {
        for size in [egui::vec2(960.0, 680.0), egui::vec2(360.0, 240.0)] {
            let mut next_frames = Vec::new();
            measure(
                &format!("paste1MiB/editor{editor}-{}x{}", size.x, size.y),
                || {
                    let mut dialog = settled(&RegexDraft::default());
                    dialog.information_section = InformationSection::Substitution;
                    dialog
                        .session
                        .set_substitution_enabled(true, Instant::now());
                    dialog.session.tick(Instant::now() + EVALUATION_DEBOUNCE);
                    let ctx = egui::Context::default();
                    frame(&mut dialog, &ctx, size, Vec::new());
                    ctx.memory_mut(|memory| memory.request_focus(egui::Id::new(id)));
                    let events = vec![egui::Event::Paste("🦀".repeat(256 * 1024))];
                    let start = Instant::now();
                    frame(&mut dialog, &ctx, size, events);
                    let elapsed = start.elapsed();
                    let retained = match editor {
                        0 => &dialog.session.draft.pattern,
                        1 => &dialog.session.draft.test_text,
                        _ => &dialog.session.draft.replacement,
                    };
                    assert_eq!(
                        retained.len(),
                        1024 * 1024,
                        "profiling must measure an actual retained paste"
                    );
                    let next = Instant::now();
                    frame(&mut dialog, &ctx, size, Vec::new());
                    next_frames.push(next.elapsed());
                    elapsed
                },
            );
            next_frames.remove(0); // exclude warm-up
            next_frames.sort();
            println!(
                "paste1MiB/editor{editor}-{}x{}/next-preview-frame: median={:.3}ms slowest={:.3}ms",
                size.x,
                size.y,
                next_frames[SAMPLES / 2].as_secs_f64() * 1000.0,
                next_frames[SAMPLES - 1].as_secs_f64() * 1000.0
            );
        }
    }
}

fn result_summary(result: &crate::regex_tester::EvaluationResult) -> String {
    match result {
        EvaluationResult::Success {
            matches,
            completeness,
        } => format!("{} displayed, {completeness:?}", matches.len()),
        EvaluationResult::InvalidPattern(error) => format!("Invalid: {}", error.message),
        EvaluationResult::Suspended(reason) => format!("Suspended: {reason:?}"),
    }
}
