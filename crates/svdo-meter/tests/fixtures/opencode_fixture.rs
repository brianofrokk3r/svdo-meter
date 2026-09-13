use std::env;
use std::fs::OpenOptions;
use std::io::{self, Write as _};
use std::path::Path;

fn main() -> io::Result<()> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    capture_args(&args)?;

    if args.first().map(String::as_str) != Some("run")
        || args.get(1).map(String::as_str) != Some("--format")
        || args.get(2).map(String::as_str) != Some("json")
    {
        std::process::exit(9);
    }

    if has_session(&args, "ses_stale") {
        eprintln!("Error: Session not found");
        std::process::exit(1);
    }

    if args
        .iter()
        .any(|arg| arg.contains("Return only one JSON object"))
    {
        println!(
            "{{\"type\":\"message\",\"part\":{{\"type\":\"text\",\"text\":\"{{\\\"score\\\":1.0,\\\"passed\\\":true,\\\"violations\\\":[],\\\"model\\\":\\\"github-copilot/gpt-5\\\",\\\"harness\\\":\\\"opencode\\\"}}\"}}}}"
        );
        return Ok(());
    }

    let session = if has_session(&args, "ses_fixture") {
        "ses_fixture"
    } else if args.iter().any(|arg| arg == "Continue ENG-OPENCODE-STALE") {
        "ses_fresh"
    } else {
        "ses_opencode_discovered"
    };
    println!(
        "{{\"type\":\"step_start\",\"sessionID\":\"{session}\",\"model\":\"github-copilot/gpt-5\"}}"
    );
    Ok(())
}

fn capture_args(args: &[String]) -> io::Result<()> {
    let Some(path) = env::var_os("OPENCODE_CAPTURE") else {
        return Ok(());
    };
    let append_attempts =
        env::var_os("OPENCODE_CAPTURE_MODE").as_deref() == Some("attempts".as_ref());
    let mut options = OpenOptions::new();
    options.create(true).write(true);
    if append_attempts {
        options.append(true);
    } else {
        options.truncate(true);
    }
    let mut file = options.open(Path::new(&path))?;
    if append_attempts {
        writeln!(file, "---")?;
    }
    for arg in args {
        writeln!(file, "{arg}")?;
    }
    Ok(())
}

fn has_session(args: &[String], session: &str) -> bool {
    args.windows(2)
        .any(|window| window[0] == "--session" && window[1] == session)
}
