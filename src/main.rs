use clap::Parser;
use rand::Rng;
use serial2::SerialPort;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io::BufRead, path::PathBuf};
use termion::input::TermRead;

const WARMUP_LINE_COUNT: usize = 500; // how many serial lines to discard as warm-up
const FLUSH_EVERY: usize = 5_000; // flush readings every N lines
const DEFAULT_DEVICE_NAME: &str = "/dev/serial/by-id/usb-1a86_USB_Serial-if00-port0";
const DEFAULT_DIR: &str = ".";
const BAUD: u32 = 115200;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the recordings directory
    #[arg(long)]
    dir: Option<PathBuf>,

    /// Path to the devive
    #[arg(long)]
    dev: Option<PathBuf>,
}

fn main() -> std::io::Result<()> {
    let args = Args::parse();

    let dev = args
        .dev
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_DEVICE_NAME));

    let port = SerialPort::open(&dev, BAUD).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("Device not found: {}", DEFAULT_DEVICE_NAME),
            )
        } else {
            e
        }
    })?;

    let base_dir = args
        .dir
        .clone()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_DIR));
    validate_dir(&base_dir)?;

    let sex = prompt_choice("sex (f/m): ", &["f", "m"], None)?;
    let hand = prompt_choice("hand (l/R): ", &["l", "r"], Some("r"))?;
    let height = prompt_height("height (in cm): ")?;

    let recording_dir = next_numeric_subdir(&base_dir)?;
    fs::create_dir(&recording_dir)?;
    println!(
        "New record: {}",
        recording_dir.to_str().unwrap_or("Failed to convert")
    );

    let char_file = recording_dir.join("chars.txt");
    let mut char_file = File::create(char_file)?;
    let _ = writeln!(
        char_file,
        "sex={}\nhand={}\nheight={}",
        sex,
        hand,
        height.unwrap_or("none".to_string())
    );

    let label_file_path = recording_dir.join("labels.csv");
    let mut label_file = File::create(label_file_path)?;

    thread::spawn(move || {
        let _ = writeln!(label_file, "{};n", now_ms());

        let stdin = io::stdin();
        for key in stdin.keys() {
            if key.is_err() {
                eprintln!("Failed to capture the pressing of a key!");
                continue;
            }
            let key = key.unwrap();

            let now = now_ms();
            match key {
                termion::event::Key::Char('t') => {
                    let _ = writeln!(label_file, "{};t", now);
                    println!("Typing")
                }
                termion::event::Key::Char('s') => {
                    let _ = writeln!(label_file, "{};s", now);
                    println!("Scrolling")
                }
                termion::event::Key::Char('f') => {
                    let _ = writeln!(label_file, "{};f", now);
                    println!("Fidgeting")
                }
                termion::event::Key::Char('n') => {
                    let _ = writeln!(label_file, "{};n", now);
                    println!("Nothing")
                }
                termion::event::Key::Char('q') => {
                    println!("Exiting...");
                    break;
                }
                _ => {}
            }
        }
    });

    let readings_file_path = recording_dir.join("readings.csv");
    let readings_file = File::create(readings_file_path)?;

    let mut buffered_writer = BufWriter::new(readings_file);

    let mut skip_first_three = 0;
    let mut counter = 0;

    let mut reader = io::BufReader::new(port);

    let mut line = String::new();

    loop {
        line.clear();
        match BufRead::read_line(&mut reader, &mut line) {
            Ok(0) => {
                break;
            }
            Ok(_) => {
                if skip_first_three < WARMUP_LINE_COUNT {
                    skip_first_three += 1;
                    continue;
                }
                let start = SystemTime::now();
                let timestamp;

                match start.duration_since(UNIX_EPOCH) {
                    Ok(duration) => {
                        // Get the seconds part of the duration
                        timestamp = duration.as_millis();
                    }
                    Err(e) => panic!("{:?}", e),
                }
                if !line.trim().is_empty() {
                    write!(buffered_writer, "{};{}", timestamp, line)?;
                    if counter > FLUSH_EVERY {
                        buffered_writer.flush()?;
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading line: {}", e);
            }
        }
        counter += 1;
    }

    buffered_writer.flush()?;

    Ok(())
}

fn validate_dir(dir: &Path) -> io::Result<()> {
    if !dir.exists() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "'{}' does not exist",
                dir.to_str().unwrap_or("Failed to convert to string")
            ),
        ));
    }
    if !dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "'{}' is not a directory",
                dir.to_str().unwrap_or("Failed to convert to string")
            ),
        ));
    }
    Ok(())
}

fn next_numeric_subdir(base_dir: &Path) -> io::Result<PathBuf> {
    let mut current_index = 1;

    let entries = fs::read_dir(&base_dir)?;
    for entry in entries {
        match entry {
            Ok(_) => current_index += 1,
            Err(e) => eprintln!("Error reading entry: {}", e),
        }
    }

    Ok(base_dir.join(current_index.to_string()).to_path_buf())
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis()
}

fn prompt_choice(prompt: &str, allowed: &[&str], default_opt: Option<&str>) -> io::Result<String> {
    let mut input = String::new();
    loop {
        print!("{}", prompt);
        io::stdout().flush()?;
        input.clear();
        io::stdin().read_line(&mut input)?;
        let mut s = input.trim().to_lowercase();
        if s.is_empty() {
            if let Some(d) = default_opt {
                s = d.to_string();
            }
        }
        if allowed.contains(&s.as_str()) {
            return Ok(s);
        } else {
            eprint!("Invalid input. Expected one of {:?}. Try again.\n", allowed);
            io::stderr().flush()?;
        }
    }
}

fn prompt_height(prompt: &str) -> io::Result<Option<String>> {
    let mut input = String::new();
    loop {
        print!("{}", prompt);
        io::stdout().flush()?;
        input.clear();
        io::stdin().read_line(&mut input)?;
        let s = input.trim();
        if s.is_empty() {
            return Ok(None);
        }
        match s.parse::<i32>() {
            Ok(h) if (50..=300).contains(&h) => return Ok(Some(h.to_string())),
            Ok(_) => {
                eprint!("You sure? Height must be between 50 and 300cm. Try again: \n");
                io::stderr().flush()?;
            }
            Err(_) => {
                eprint!("Height must be a valid integer. Try again: \n");
                io::stderr().flush()?;
            }
        }
    }
}
