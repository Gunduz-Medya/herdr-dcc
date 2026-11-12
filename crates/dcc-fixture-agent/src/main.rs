//! A pretend agent for the OSC 7501 contract test. Real agents report their state with
//! `ESC ] 7501 ; state=<state>:app=<name> BEL` (the format was found by experiment, Herdr does not
//! document it). This program sends that sequence for idle, working, blocked and done with a
//! pause in between, then exits.
//!
//!   dcc-fixture-agent --mode ok       the sequence above
//!   dcc-fixture-agent --mode broken   sequences that are not valid reports (the test must fail)
//!   dcc-fixture-agent --step-ms 1200  pause between steps

use std::io::Write;
use std::time::Duration;

fn osc(payload: &str) {
    let mut out = std::io::stdout();
    let _ = write!(out, "\x1b]7501;{payload}\x07");
    let _ = out.flush();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let broken = value("--mode").as_deref() == Some("broken");
    let step = Duration::from_millis(value("--step-ms").and_then(|v| v.parse().ok()).unwrap_or(1200));
    let steps: [&str; 4] = if broken {
        ["stat=idle:app=dcc-fixture", "", "state=sleeping:app=dcc-fixture", "app=dcc-fixture"]
    } else {
        ["state=idle:app=dcc-fixture", "state=working:app=dcc-fixture", "state=blocked:app=dcc-fixture:kind=permission", "state=done:app=dcc-fixture"]
    };
    for s in steps {
        osc(s);
        std::thread::sleep(step);
    }
    std::thread::sleep(step);
}
