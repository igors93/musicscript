use super::*;

#[test]
fn only_web_links_are_opened() {
    assert!(is_web_url("https://cadquery.readthedocs.io/en/latest/"));
    assert!(is_web_url("HTTP://example.com/model?id=1"));
    // Links from an upload service's reply must not start local programs.
    assert!(!is_web_url(r"C:\Windows\System32\calc.exe"));
    assert!(!is_web_url("file:///etc/passwd"));
    assert!(!is_web_url("javascript:alert(1)"));
    assert!(!is_web_url("https://"));
    assert!(!is_web_url("https://example.com/\nsecond line"));
}

#[test]
fn the_documentation_link_points_to_these_docs() {
    assert!(DOCUMENTATION_URL.ends_with("/docs/README.md"));
    assert!(!DOCUMENTATION_URL.contains("/wiki"));
    assert!(is_web_url(DOCUMENTATION_URL));
}

#[test]
fn links_clicked_in_widgets_are_taken_from_the_platform_output() {
    let ctx = egui::Context::default();
    let mut urls = Vec::new();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        ui.ctx()
            .open_url(egui::OpenUrl::new_tab("https://example.com/a"));
        ui.ctx().copy_text("kept".to_owned());
        urls = take_requested_urls(ui.ctx());
    });
    output.textures_delta.clear();

    assert_eq!(urls, ["https://example.com/a"]);
    // Other commands, such as copying, still reach the platform.
    assert_eq!(
        output.platform_output.commands,
        [egui::OutputCommand::CopyText("kept".to_owned())]
    );
}

// -- application state without a window ----------------------------------------

/// An `AppState` with defaults and no Python, for exercising actions and
/// worker replies directly.
pub(super) fn test_state(text: &str) -> AppState {
    let ctx = egui::Context::default();
    let cache_dir = std::env::temp_dir().join("cq-editor-app-tests");
    AppState {
        worker: Worker::new(ctx.clone()),
        ctx,
        settings: Settings::default(),
        document: Document::untitled(text),
        editor: CodeEditor::default(),
        scene: Scene::default(),
        viewer: Viewer::default(),
        variables: Vec::new(),
        traceback: None,
        inspector: InspectorState::default(),
        measurements: MeasurementState::default(),
        selector: SelectorState::default(),
        parameters: ParametersState::default(),
        parameter_render_at: None,
        document_generation: 0,
        run_script: None,
        console: ConsoleState::default(),
        log_view: LogView::default(),
        status: None,
        dialogs: Dialogs::default(),
        execution: Execution::default(),
        actions: Vec::new(),
        watcher: FileWatcher::default(),
        watch_key: None,
        examples: Vec::new(),
        palette: Palette::DARK,
        recovery: RecoveryStore::new(cache_dir.join("recovery")),
        cache_dir,
        next_recovery: Instant::now() + CHECKPOINT_INTERVAL,
        recovery_error_reported: false,
        python_override: None,
        worker_was_ready: false,
        allow_close: false,
        title: String::new(),
        applied_theme: None,
        applied_scale: 1.0,
        logo: None,
    }
}

fn selector_reply(id: crate::worker::protocol::RequestId) -> crate::worker::protocol::Message {
    use crate::worker::protocol::{Message, SelectorCandidate, SelectorReply};
    let candidate = |expression: &str, index_based| SelectorCandidate {
        expression: expression.to_owned(),
        description: String::new(),
        index_based,
    };
    Message::Selector(SelectorReply {
        id,
        method: "faces".into(),
        candidates: vec![
            candidate(r#".faces(">Z")"#, false),
            candidate(".faces()[5]", true),
        ],
    })
}

/// A face picked in the view, as `select` records it.
fn pick_face(state: &mut AppState) {
    use crate::scene::{TopologyEntity, TopologySelection};
    let mut mesh = crate::scene::Mesh::new(
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        vec![[0.0, 0.0, 1.0]; 3],
        vec![0, 1, 2],
        Vec::new(),
        Vec::new(),
    );
    mesh.topology.triangle_faces = vec![6];
    let node = crate::scene::SceneNode {
        key: "n1".into(),
        name: "box".into(),
        kind: "Workplane".into(),
        inspectable: false,
        parts: vec![crate::scene::Part {
            mesh: std::sync::Arc::new(mesh),
            transform: glam::Mat4::IDENTITY,
            color: None,
            alpha: None,
        }],
        children: Vec::new(),
        visible: true,
        color: None,
        alpha: None,
        edge_color: None,
        overrides: Default::default(),
        line: None,
        script: None,
        ..Default::default()
    };
    state.scene.append(vec![node], None);
    state.scene.select_only("n1");
    assert!(state.scene.select_topology(
        TopologySelection {
            node: "n1".into(),
            part: 0,
            entity: TopologyEntity::Face(6),
            position: Default::default(),
        },
        false,
    ));
}

/// Pretends a selector request is out, as `request_selector` sends it.
fn awaiting_selector(state: &mut AppState, id: crate::worker::protocol::RequestId) {
    state.execution.track(id, Pending::Selector);
    state.selector.loading = true;
    state.selector.request_id = Some(id);
}

#[test]
fn a_selector_asked_for_while_loading_is_inserted_when_it_arrives() {
    let mut state = test_state("result = cq.Workplane().box(1, 1, 1)\n");
    pick_face(&mut state);
    awaiting_selector(&mut state, 7);
    state.use_selector(SelectorUse::Insert);
    assert_eq!(state.selector.then, Some(SelectorUse::Insert));
    assert!(!state.document.text.contains("faces"));

    // A late reply to an earlier selection changes nothing.
    state.handle_message(selector_reply(3));
    assert!(state.selector.reply.is_none());

    state.handle_message(selector_reply(7));
    assert_eq!(state.selector.then, None);
    assert!(!state.selector.loading);
    assert_eq!(
        state.document.text, ".faces(\">Z\")result = cq.Workplane().box(1, 1, 1)\n",
        "inserted at the caret, at the start of a fresh document"
    );
    assert_eq!(state.status.as_ref().unwrap().kind, StatusKind::Success);

    // One undo step takes it out again.
    state.editor.undo(&mut state.document);
    assert_eq!(
        state.document.text,
        "result = cq.Workplane().box(1, 1, 1)\n"
    );
}

#[test]
fn copying_a_selector_puts_it_on_the_clipboard_and_flags_index_fallbacks() {
    let mut state = test_state("");
    pick_face(&mut state);
    awaiting_selector(&mut state, 4);
    state.handle_message(selector_reply(4));

    let ctx = state.ctx.clone();
    let mut output = ctx.run_ui(egui::RawInput::default(), |_| {
        state.use_selector(SelectorUse::Copy);
    });
    output.textures_delta.clear();
    assert_eq!(
        output.platform_output.commands,
        [egui::OutputCommand::CopyText(r#".faces(">Z")"#.to_owned())]
    );
    assert!(!state.status.as_ref().unwrap().text.contains("index"));

    // The alternative chosen in the panel is used, with a caution.
    state.selector.chosen = 1;
    let mut output = ctx.run_ui(egui::RawInput::default(), |_| {
        state.use_selector(SelectorUse::Copy);
    });
    output.textures_delta.clear();
    assert_eq!(
        output.platform_output.commands,
        [egui::OutputCommand::CopyText(".faces()[5]".to_owned())]
    );
    assert!(state.status.as_ref().unwrap().text.contains("by index"));
}

#[test]
fn a_selector_error_is_kept_and_reported_to_a_waiting_copy() {
    let mut state = test_state("");
    pick_face(&mut state);
    awaiting_selector(&mut state, 9);
    state.use_selector(SelectorUse::Copy);
    state.request_failed(Some(9), "Selectors are generated for workplanes".into());

    assert_eq!(
        state.selector.error.as_deref(),
        Some("Selectors are generated for workplanes")
    );
    assert!(!state.selector.loading);
    let status = state.status.as_ref().unwrap();
    assert_eq!(status.kind, StatusKind::Warning);
    assert!(status.text.contains("workplanes"));
}

#[test]
fn selector_actions_need_picked_entities() {
    let mut state = test_state("");
    state.use_selector(SelectorUse::Insert);
    assert_eq!(state.status.as_ref().unwrap().kind, StatusKind::Warning);
    assert!(state.document.text.is_empty());
}

#[test]
fn the_console_gets_the_picked_entities_and_the_selected_objects() {
    use crate::worker::protocol::TopologyKind;
    let mut state = test_state("");
    assert_eq!(state.console_selection(), Default::default());

    pick_face(&mut state);
    let selection = state.console_selection();
    assert_eq!(selection.objects, ["n1"]);
    let [entity] = selection.entities.as_slice() else {
        panic!("one picked face");
    };
    assert_eq!(
        (entity.key.as_str(), entity.kind, entity.identifier),
        ("n1", TopologyKind::Face, 6)
    );
}

#[test]
fn a_parameter_change_rewrites_its_literal_and_renders_once_it_settles() {
    let script = "width = 40  # [10, 100]\nheight = 12.5\nresult = box(width, height)\n";
    let mut state = test_state(script);

    state.set_parameter("height", 2, 20.25);
    assert_eq!(
        state.document.text,
        "width = 40  # [10, 100]\nheight = 20.2\nresult = box(width, height)\n"
    );
    assert!(state.document.is_modified());
    let due = state.parameter_render_at.expect("a render is scheduled");

    // Not yet: the drag may go on.
    state.parameter_render_at = Some(f64::MAX);
    let ctx = state.ctx.clone();
    state.poll_parameter_render(&ctx);
    assert!(state.parameter_render_at.is_some());
    assert!(state.status.is_none());

    // Once due, it renders (here: reports that Python is not running).
    state.parameter_render_at = Some(due - 10.0);
    state.poll_parameter_render(&ctx);
    assert!(state.parameter_render_at.is_none());
    assert!(state.status.as_ref().unwrap().text.contains("Python"));
}

#[test]
fn a_parameter_is_found_again_by_line_and_name() {
    let mut state = test_state("width = 40\n");
    state.set_parameter("width", 1, 55.0);
    assert_eq!(state.document.text, "width = 55\n");
    // Edited meanwhile into something that is no longer a parameter.
    state.document.text = "width = w0\n".to_owned();
    state.set_parameter("width", 1, 60.0);
    assert_eq!(state.document.text, "width = w0\n");
    // Another name on that line is another parameter.
    state.document.text = "depth = 3\n".to_owned();
    state.set_parameter("width", 1, 60.0);
    assert_eq!(state.document.text, "depth = 3\n");
}

#[test]
fn a_commented_range_bounds_the_written_value() {
    let mut state = test_state("width = 40  # [10, 100]\n");
    state.set_parameter("width", 1, 400.0);
    assert_eq!(state.document.text, "width = 100  # [10, 100]\n");
}

/// A scene with one node made on `line` by a run of `ran`.
fn shown_from_run(state: &mut AppState, ran: &str, line: usize) {
    pick_face(state);
    let script = std::sync::Arc::new(crate::scene::RunScript {
        document: state.document_generation,
        text: ran.to_owned(),
    });
    let node = state.scene.find_mut("n1").unwrap();
    node.line = Some(line);
    node.attach_script(&script);
}

fn status_text(state: &AppState) -> &str {
    &state.status.as_ref().unwrap().text
}

#[test]
fn going_to_the_source_follows_the_edits_made_since_the_run() {
    let ran = "import cadquery as cq\nbox = cq.Workplane().box(1, 2, 3)\n";
    let mut state = test_state(ran);
    shown_from_run(&mut state, ran, 2);

    state.go_to_source("n1", None);
    assert_eq!(status_text(&state), "Line 2");
    assert_eq!(state.status.as_ref().unwrap().kind, StatusKind::Info);

    state.document.text = format!("# a box\n{ran}");
    state.go_to_source("n1", None);
    assert_eq!(status_text(&state), "Line 3 (line 2 when the script ran)");

    state.document.text = state.document.text.replace("box(1, 2, 3)", "box(1, 2, 4)");
    state.go_to_source("n1", None);
    assert_eq!(state.status.as_ref().unwrap().kind, StatusKind::Warning);
    assert!(status_text(&state).contains("edited since the last run"));

    state.document.text = "import cadquery as cq\n".to_owned();
    state.go_to_source("n1", None);
    assert!(status_text(&state).contains("deleted"));
}

#[test]
fn objects_of_another_document_or_the_console_have_no_source_here() {
    let ran = "box = cq.Workplane().box(1, 1, 1)\n";
    let mut state = test_state(ran);
    shown_from_run(&mut state, ran, 1);
    state.document_generation += 1;
    state.go_to_source("n1", None);
    assert!(status_text(&state).contains("another script"));

    let node = state.scene.find_mut("n1").unwrap();
    node.script = None;
    state.go_to_source("n1", None);
    assert!(status_text(&state).contains("console"));
}

#[test]
fn inspector_steps_go_to_the_line_that_made_them() {
    use crate::worker::protocol::{StackFrame, StackItem};
    let ran = "plate = (\n    cq.Workplane()\n    .box(4, 4, 1)\n    .faces(\">Z\")\n)\n";
    let mut state = test_state(ran);
    shown_from_run(&mut state, ran, 4);
    state.inspector.key = Some("n1".into());
    state.inspector.frames = vec![StackFrame {
        type_name: "Workplane".into(),
        value: String::new(),
        line: Some(4),
        items: vec![StackItem {
            type_name: "Solid".into(),
            value: String::new(),
            displayable: true,
            line: Some(3),
        }],
    }];

    state.go_to_source("n1", Some((0, Some(0))));
    assert_eq!(status_text(&state), "Line 3");
    state.go_to_source("n1", Some((0, None)));
    assert_eq!(status_text(&state), "Line 4");
    // A step of another object's stack is not this node's.
    state.inspector.key = Some("other".into());
    state.go_to_source("n1", Some((0, None)));
    assert!(status_text(&state).contains("No script line"));
}

#[test]
fn only_the_run_that_made_the_nodes_gives_them_its_script() {
    let mut state = test_state("x = 1\n");
    let script = std::sync::Arc::new(crate::scene::RunScript {
        document: 0,
        text: "x = 1\n".into(),
    });
    state.run_script = Some((7, script));
    pick_face(&mut state);
    let mut nodes = std::mem::take(&mut state.scene.nodes);
    state.attach_run_script(6, &mut nodes);
    assert!(nodes[0].script.is_none());
    state.attach_run_script(7, &mut nodes);
    assert!(nodes[0].script.is_some());
}

#[test]
fn objects_are_measured_when_no_entity_is_picked() {
    let mut state = test_state("");
    pick_face(&mut state);
    let (selections, objects) = state.measured();
    assert_eq!(selections.len(), 1);
    assert!(objects.is_empty(), "picked entities come first");

    state.scene.clear_topology_selection();
    let (selections, objects) = state.measured();
    assert!(selections.is_empty());
    assert_eq!(objects, ["n1"]);
}

#[test]
fn measurement_code_goes_to_the_clipboard_with_its_check() {
    use crate::worker::protocol::Message;
    let mut state = test_state("");
    state.request_measurement_code();
    assert!(status_text(&state).contains("Select objects"));

    let ctx = state.ctx.clone();
    state.execution.track(11, Pending::MeasurementCode);
    let mut output = ctx.run_ui(egui::RawInput::default(), |_| {
        state.handle_message(Message::MeasurementCode {
            id: 11,
            code: "area_1 = face_1.Area()  # 40\n".into(),
            verified: true,
            problem: None,
        });
    });
    output.textures_delta.clear();
    assert_eq!(
        output.platform_output.commands,
        [egui::OutputCommand::CopyText(
            "area_1 = face_1.Area()  # 40\n".into()
        )]
    );
    assert_eq!(state.status.as_ref().unwrap().kind, StatusKind::Success);

    state.execution.track(12, Pending::MeasurementCode);
    let mut output = ctx.run_ui(egui::RawInput::default(), |_| {
        state.handle_message(Message::MeasurementCode {
            id: 12,
            code: "shape_1 = obj.val()\n".into(),
            verified: false,
            problem: Some("the Workplane is not a variable of the script".into()),
        });
    });
    output.textures_delta.clear();
    assert_eq!(state.status.as_ref().unwrap().kind, StatusKind::Warning);
    assert!(status_text(&state).contains("not a variable"));

    // A reply nobody asked for is not copied.
    let mut output = ctx.run_ui(egui::RawInput::default(), |_| {
        state.handle_message(Message::MeasurementCode {
            id: 99,
            code: "stale\n".into(),
            verified: true,
            problem: None,
        });
    });
    output.textures_delta.clear();
    assert!(output.platform_output.commands.is_empty());

    state.execution.track(13, Pending::MeasurementCode);
    state.request_failed(Some(13), "Not available while debugging".into());
    assert!(status_text(&state).contains("Cannot write the measurement as code"));
}

#[test]
fn explode_and_isolate_commands_change_what_the_view_shows() {
    let mut state = test_state("");

    // Isolating needs a selection.
    state.execute(Action::ToggleIsolate);
    assert!(!state.scene.is_isolating());
    assert_eq!(state.status.as_ref().unwrap().kind, StatusKind::Info);

    pick_face(&mut state);
    state.execute(Action::ToggleIsolate);
    assert!(state.scene.is_isolating());
    state.execute(Action::ToggleIsolate);
    assert!(!state.scene.is_isolating(), "the same command ends it");

    state.execute(Action::ToggleIsolate);
    state.scene.set_visible("n1", false);
    state.execute(Action::ShowAll);
    assert!(!state.scene.is_isolating());
    assert!(state.scene.find("n1").unwrap().visible);

    state.execute(Action::ToggleExplode);
    assert!(state.viewer.explode_controls);
    assert!(state.scene.explode() > 0.0, "opening spreads the parts");
    state.execute(Action::SetExplode(1.5));
    assert_eq!(state.scene.explode(), 1.5);
    state.execute(Action::ToggleExplode);
    assert!(!state.viewer.explode_controls);
    assert_eq!(state.scene.explode(), 0.0, "closing assembles them");
}

#[test]
fn named_views_follow_their_file() {
    use crate::document::Encoding;
    let dir = std::env::temp_dir().join(format!("cq-editor-views-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let (a, b) = (dir.join("a.py"), dir.join("b.py"));
    std::fs::write(&a, "a = 1\n").unwrap();
    std::fs::write(&b, "b = 2\n").unwrap();

    let mut state = test_state("");
    state.open_path_with(&a, Encoding::Utf8);
    state.execute(Action::SaveView(Some("front".into())));
    assert_eq!(state.viewer.named_views.len(), 1);
    assert_eq!(state.status.as_ref().unwrap().kind, StatusKind::Success);

    state.open_path_with(&b, Encoding::Utf8);
    assert!(state.viewer.named_views.is_empty(), "b has no views");
    state.open_path_with(&a, Encoding::Utf8);
    assert_eq!(state.viewer.named_views[0].name, "front");

    // Going to a view brings its projection back.
    state.settings.viewer.projection = crate::settings::Projection::Perspective;
    state.execute(Action::GoToView(0));
    assert_eq!(
        state.settings.viewer.projection,
        crate::settings::Projection::Orthographic
    );

    // A new document starts without views.
    state.execute(Action::DeleteView(0));
    state.execute(Action::SaveView(None));
    state.open_path_with(&b, Encoding::Utf8);
    state.open_path_with(&a, Encoding::Utf8);
    assert_eq!(state.viewer.named_views[0].name, "View 1");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_ground_grid_and_scale_bar_are_toggled_from_the_view_menu() {
    let mut state = test_state("");
    assert!(state.settings.viewer.ground_grid && state.settings.viewer.scale_bar);
    state.execute(Action::ToggleGroundGrid);
    state.execute(Action::ToggleScaleBar);
    assert!(!state.settings.viewer.ground_grid && !state.settings.viewer.scale_bar);
}

#[test]
fn an_interrupted_run_is_reported_as_stopped_not_as_an_error() {
    let mut state = test_state("while True:\n    pass\n");
    state.execution.started(4, false);
    let message: crate::worker::protocol::Message = serde_json::from_str(
        r#"{"type":"run_finished","id":4,"ok":false,"debug":false,"stopped":false,
            "cancelled":false,"interrupted":true,"objects":null,"variables":null,
            "error":{"type_name":"KeyboardInterrupt","message":"","frames":[],"line":2,
                "text":"Traceback (most recent call last):\nKeyboardInterrupt\n"},
            "duration":1.0}"#,
    )
    .unwrap();
    state.handle_message(message);

    let status = state.status.as_ref().unwrap();
    assert_eq!(status.kind, StatusKind::Info);
    assert_eq!(status.text, "Stopped at line 2");
    assert_eq!(state.editor.error_line, None, "no error marker");
    assert!(
        state.traceback.is_none(),
        "the view does not offer it as an error"
    );
    assert!(!state.is_busy());
}

#[test]
fn a_profiled_run_puts_its_times_in_the_gutter_of_the_edited_script() {
    let run_text = "import time\ntime.sleep(1)\nx = 1\n";
    let mut state = test_state(run_text);
    state.execution.started(4, false);
    state.run_script = Some((
        4,
        std::sync::Arc::new(crate::scene::RunScript {
            document: 0,
            text: run_text.to_owned(),
        }),
    ));
    state.editor.sync_markers(run_text);
    // A line was added while it ran.
    state.document.text = format!("# timing\n{run_text}");
    let message: crate::worker::protocol::Message = serde_json::from_str(
        r#"{"type":"run_finished","id":4,"ok":true,"debug":false,"stopped":false,
            "objects":[],"variables":[],"error":null,"duration":1.0,
            "profile":{"lines":[{"line":1,"seconds":0.002,"hits":1},
                {"line":2,"seconds":1.001,"hits":1},{"line":3,"seconds":0.0001,"hits":1}],
                "operations":[]}}"#,
    )
    .unwrap();
    state.handle_message(message);

    let lines: Vec<_> = state.editor.line_profile.keys().copied().collect();
    assert_eq!(lines, [2, 3, 4]);
    assert_eq!(state.editor.line_profile[&3].seconds, 1.001);
    let status = state.status.as_ref().unwrap();
    assert!(
        status.text.contains("slowest line is 3 (1.0 s)"),
        "{}",
        status.text
    );

    state.execute(Action::ClearProfile);
    assert!(state.editor.line_profile.is_empty());
}

#[test]
fn new_environments_go_to_a_free_folder() {
    let root = std::env::temp_dir().join(format!("cq-editor-free-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let preferred = root.join("cadquery");
    assert_eq!(unused_folder(&preferred), preferred, "missing: free");
    std::fs::create_dir_all(&preferred).unwrap();
    assert_eq!(unused_folder(&preferred), preferred, "empty: free");
    std::fs::write(preferred.join("pyvenv.cfg"), "").unwrap();
    assert_eq!(unused_folder(&preferred), root.join("cadquery-2"));
    std::fs::create_dir_all(root.join("cadquery-2/bin")).unwrap();
    assert_eq!(unused_folder(&preferred), root.join("cadquery-3"));
    let _ = std::fs::remove_dir_all(&root);

    let mut state = test_state("");
    state.execute(Action::InstallPackages);
    assert!(state.dialogs.environment.is_some());
}

fn updates_reply(id: u64, editor: &str) -> crate::worker::protocol::Message {
    serde_json::from_str(&format!(
        r#"{{"type":"updates","id":{id},"current":"2.8.0","newer":[],"editor":{editor}}}"#
    ))
    .unwrap()
}

#[test]
fn the_daily_check_only_speaks_when_a_newer_cq_editor_is_out() {
    let mut state = test_state("");
    let newer = r#"{"current":"0.8.0","newer":[{"tag":"v99.0.0","name":"99",
        "url":"https://github.com/igors93/CQ-editor/releases/tag/v99.0.0"}]}"#;

    state
        .execution
        .track(1, Pending::Updates { automatic: true });
    state.handle_message(updates_reply(1, r#"{"current":"0.8.0","newer":[]}"#));
    assert!(
        state.status.is_none() && state.dialogs.message.is_none(),
        "silent"
    );

    state
        .execution
        .track(2, Pending::Updates { automatic: true });
    state.handle_message(updates_reply(2, newer));
    assert!(state.dialogs.message.is_none());
    assert!(state.status.as_ref().unwrap().text.contains("v99.0.0"));
    assert_eq!(
        state.settings.general.newer_release.as_ref().unwrap().tag,
        "v99.0.0"
    );

    // A failed daily check is only logged.
    state
        .execution
        .track(3, Pending::Updates { automatic: true });
    state.handle_message(crate::worker::protocol::Message::Error {
        id: Some(3),
        message: "offline".into(),
    });
    assert!(state.dialogs.message.is_none());
}

#[test]
fn checking_by_hand_reports_cq_editor_and_cadquery() {
    let mut state = test_state("");
    state
        .execution
        .track(4, Pending::Updates { automatic: false });
    state.handle_message(updates_reply(4, r#"{"current":"0.8.0","newer":[]}"#));
    let message = state.dialogs.message.as_ref().unwrap();
    assert_eq!(message.title, "No updates available");
    assert!(message.text.contains("is the latest version"));
    assert!(message.text.contains("CadQuery 2.8.0"));
}

#[test]
fn console_results_from_the_worker_reach_the_console() {
    use crate::app::execution::Pending;
    use crate::worker::protocol::{DisplayImage, DisplayOutput, Message};

    let mut state = test_state("");
    state.execution.track(3, Pending::Console);
    state.handle_message(Message::Display(DisplayOutput {
        id: 3,
        text: "<Figure size 2x1>".into(),
        image: Some(DisplayImage {
            size: [2, 1],
            rgba: vec![255; 8],
        }),
    }));
    let entry = state.console.entries.last().unwrap();
    assert!(entry.image.is_some());
    assert_eq!(entry.text, "<Figure size 2x1>\n");

    state.handle_message(Message::Display(DisplayOutput {
        id: 4,
        text: "stale".into(),
        image: None,
    }));
    assert_eq!(state.console.entries.len(), 1, "stale output is ignored");
}
