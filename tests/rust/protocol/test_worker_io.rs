use super::*;
use std::io::{BufReader, Cursor};

#[test]
fn limited_line_reader_caps_memory_and_resumes_at_the_next_line() {
    let input = Cursor::new(b"123456\nok\nlast".to_vec());
    let mut reader = BufReader::with_capacity(3, input);

    assert_eq!(
        read_limited_line(&mut reader, 4).unwrap(),
        LimitedLine::TooLong(b"1234".to_vec())
    );
    assert_eq!(
        read_limited_line(&mut reader, 4).unwrap(),
        LimitedLine::Line(b"ok".to_vec())
    );
    assert_eq!(
        read_limited_line(&mut reader, 4).unwrap(),
        LimitedLine::Line(b"last".to_vec())
    );
    assert_eq!(read_limited_line(&mut reader, 4).unwrap(), LimitedLine::Eof);
}

#[test]
fn exact_line_limit_is_accepted() {
    let mut reader = Cursor::new(b"1234\n".to_vec());
    assert_eq!(
        read_limited_line(&mut reader, 4).unwrap(),
        LimitedLine::Line(b"1234".to_vec())
    );
}

#[test]
fn image_and_mesh_messages_have_separate_larger_limits() {
    let control = br#"{"type":"output","text":"hello"}"#;
    let display = format!(
        r#"{{"type":"display","image":{{"data":"{}"}}}}"#,
        "x".repeat(MAX_CONTROL_MESSAGE_BYTES)
    );
    let run = format!(
        r#"{{"id":1,"type":"run_finished","padding":"{}"}}"#,
        "x".repeat(MAX_CONTROL_MESSAGE_BYTES)
    );
    let inspection = format!(
        r#"{{"type":"inspection_geometry","padding":"{}"}}"#,
        "x".repeat(MAX_CONTROL_MESSAGE_BYTES)
    );

    assert_eq!(worker_message_limit(control), MAX_CONTROL_MESSAGE_BYTES);
    assert_eq!(
        worker_message_limit(display.as_bytes()),
        MAX_DISPLAY_MESSAGE_BYTES
    );
    assert_eq!(worker_message_limit(run.as_bytes()), MAX_MESH_MESSAGE_BYTES);
    assert_eq!(
        worker_message_limit(inspection.as_bytes()),
        MAX_MESH_MESSAGE_BYTES
    );
}

#[test]
fn a_large_unknown_or_malformed_message_keeps_the_control_limit() {
    let unknown = format!(
        r#"{{"type":"future","data":"{}"}}"#,
        "x".repeat(2 * 1024 * 1024)
    );
    let malformed = vec![b'x'; MAX_CONTROL_MESSAGE_BYTES + 1];

    assert_eq!(
        worker_message_limit(unknown.as_bytes()),
        MAX_CONTROL_MESSAGE_BYTES
    );
    assert!(unknown.len() > worker_message_limit(unknown.as_bytes()));
    assert_eq!(worker_message_limit(&malformed), MAX_CONTROL_MESSAGE_BYTES);

    let deceptive_output = format!(
        r#"{{"type":"output","text":"\"type\":\"run_finished\"{}"}}"#,
        "x".repeat(MAX_CONTROL_MESSAGE_BYTES)
    );
    assert_eq!(
        worker_message_limit(deceptive_output.as_bytes()),
        MAX_CONTROL_MESSAGE_BYTES
    );
}

#[test]
fn a_failed_start_says_whether_python_or_cadquery_is_missing() {
    // The Microsoft Store placeholder for python.exe exits with 9009.
    let expected = if cfg!(windows) {
        Problem::NoPython
    } else {
        Problem::Exited
    };
    assert_eq!(exit_problem(Some(9009)), expected);
    assert_eq!(exit_problem(Some(1)), Problem::Exited);
    assert_eq!(exit_problem(None), Problem::Exited);

    // Only a failed worker has a problem.
    let mut worker = Worker::new(egui::Context::default());
    worker.problem = Some(Problem::NoCadQuery);
    assert_eq!(worker.problem(), None);
    worker.status = Status::Failed("Cannot load CadQuery".into());
    assert_eq!(worker.problem(), Some(Problem::NoCadQuery));
}
