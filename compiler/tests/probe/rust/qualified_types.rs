use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn read_to_string(p: &Path) -> io::Result<String> {
    std::fs::read_to_string(p)
}

fn make_path(a: &str, b: &str) -> PathBuf {
    let mut p = PathBuf::from(a);
    p.push(b);
    p
}

fn short_wait() -> Duration {
    Duration::from_millis(100)
}

fn write_hello<W: Write>(w: &mut W) -> io::Result<()> {
    w.write_all(b"hello")
}
