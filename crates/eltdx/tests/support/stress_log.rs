#![allow(dead_code)]

//! Append-only raw evidence, used only by explicitly ignored stress targets.
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone)]
pub struct Log {
    file: Arc<Mutex<File>>,
    case: &'static str,
    start: Instant,
}
impl Log {
    pub fn start(case: &'static str, expected: usize, concurrency: usize) -> io::Result<Self> {
        if std::env::var("ELTDX_IMPLEMENTATION_COMPLETE").as_deref() != Ok("yes") {
            return Err(io::Error::other(
                "complete all implementation sources before stress execution",
            ));
        }
        let directory = std::env::var_os("ELTDX_RUST_STRESS_EVENTS").ok_or_else(|| {
            io::Error::other("set ELTDX_RUST_STRESS_EVENTS to a new evidence directory")
        })?;
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory)?;
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(directory.join(format!("{case}.ndjson")))?;
        let log = Self {
            file: Arc::new(Mutex::new(file)),
            case,
            start: Instant::now(),
        };
        log.write(format!(
            "\"kind\":\"start\",\"expected\":{expected},\"concurrency\":{concurrency},\"seed\":0"
        ))?;
        log.phase("baseline")?;
        std::thread::park_timeout(Duration::from_secs(1));
        log.phase("measurement")?;
        Ok(log)
    }
    pub fn write(&self, fields: String) -> io::Result<()> {
        let mut file = self
            .file
            .lock()
            .map_err(|_| io::Error::other("stress evidence mutex poisoned"))?;
        let unix_ns = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        writeln!(
            file,
            "{{\"case\":\"{}\",\"elapsed_ns\":{},\"unix_ns\":{unix_ns},{} }}",
            self.case,
            self.start.elapsed().as_nanos(),
            fields
        )
    }
    pub fn phase(&self, phase: &'static str) -> io::Result<()> {
        self.write(format!("\"kind\":\"phase\",\"phase\":\"{phase}\""))
    }
    pub fn warmed(&self) -> io::Result<()> {
        self.phase("baseline")?;
        std::thread::park_timeout(Duration::from_secs(1));
        self.phase("measurement")
    }
    pub fn latency(&self, id: usize, stage: &'static str, elapsed: Duration) -> io::Result<()> {
        self.write(format!(
            "\"kind\":\"latency\",\"id\":{id},\"stage\":\"{stage}\",\"latency_ns\":{}",
            elapsed.as_nanos()
        ))
    }
    pub fn terminal(&self, id: usize, outcome: &'static str, elapsed: Duration) -> io::Result<()> {
        self.write(format!(
            "\"kind\":\"terminal\",\"id\":{id},\"outcome\":\"{outcome}\",\"latency_ns\":{}",
            elapsed.as_nanos()
        ))
    }
    pub fn wire(&self, connection: usize, id: u32, command: u16) -> io::Result<()> {
        self.write(format!(
            "\"kind\":\"wire\",\"connection\":{connection},\"id\":{id},\"command\":{command}"
        ))
    }
    pub fn wire_for(
        &self,
        operation: usize,
        connection: usize,
        id: u32,
        command: u16,
    ) -> io::Result<()> {
        self.write(format!("\"kind\":\"wire\",\"operation\":{operation},\"connection\":{connection},\"id\":{id},\"command\":{command}"))
    }
    pub fn finish(&self, completed: usize) -> io::Result<()> {
        self.phase("end")?;
        self.phase("cooldown")?;
        std::thread::park_timeout(Duration::from_secs(2));
        self.write(format!("\"kind\":\"finish\",\"completed\":{completed}"))?;
        self.file
            .lock()
            .map_err(|_| io::Error::other("stress evidence mutex poisoned"))?
            .sync_all()
    }
}

pub fn count(name: &str, default: usize, minimum: usize, maximum: usize) -> io::Result<usize> {
    let value = match std::env::var(name) {
        Ok(value) => value.parse::<usize>().map_err(io::Error::other)?,
        Err(std::env::VarError::NotPresent) => default,
        Err(error) => return Err(io::Error::other(error)),
    };
    if !(minimum..=maximum).contains(&value) {
        return Err(io::Error::other(format!(
            "{name} outside {minimum}..={maximum}"
        )));
    }
    Ok(value)
}
