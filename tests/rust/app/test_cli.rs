use super::*;

fn parse(args: &[&str]) -> Result<StartupOptions, String> {
    parse_args(args.iter().map(|s| s.to_string()))
}

#[test]
fn parses_file_and_python() {
    let options = parse(&["--python", "/opt/py", "part.py"]).unwrap();
    assert_eq!(options.python.as_deref(), Some("/opt/py"));
    assert_eq!(options.file, Some(PathBuf::from("part.py")));
    assert!(parse(&["--bogus"]).is_err());
    assert!(parse(&["a.py", "b.py"]).is_err());
    assert!(parse(&["--python"]).is_err());
}
