use clap::Parser;
use serial2::SerialPort;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{fs, io::BufRead, path::PathBuf};
use termion::input::TermRead;

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

const DEFAULT_DEVICE_NAME: &str = "/dev/serial/by-id/usb-1a86_USB_Serial-if00-port0";
const DEFAULT_DIR: &str = ".";
const BAUD: u32 = 115200;

fn main() -> std::io::Result<()> {
    let args = Args::parse();

    let dev = args
        .dev
        .clone()
        .unwrap_or(PathBuf::from(DEFAULT_DEVICE_NAME));

    let port = SerialPort::open(dev, BAUD).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("Device not found: {}", DEFAULT_DEVICE_NAME),
            )
        } else {
            e
        }
    })?;

    let mut dir = args.dir.clone().unwrap_or(PathBuf::from(DEFAULT_DIR));

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

    let mut sex = String::new();
    print!("sex (f/m): ");
    io::stdout().flush()?;
    loop {
        io::stdin().read_line(&mut sex)?;
        sex = sex.trim().to_lowercase();
        if sex == "f" || sex == "m" {
            break;
        } else {
            eprint!("Sex must be either 'f' or 'm'. Try again: ");
            io::stderr().flush()?;
        }
        sex.clear();
    }

    let mut hand = String::new();
    print!("hand (l/R): ");
    io::stdout().flush()?;
    loop {
        io::stdin().read_line(&mut hand)?;

        if hand.trim().is_empty() {
            hand = "r".to_string();
        }

        hand = hand.trim().to_lowercase();
        if hand == "l" || hand == "r" {
            break;
        } else {
            eprint!("hand must be either 'l' or 'r'. Try again: ");
            io::stderr().flush()?;
        }
        hand.clear();
    }

    let mut height = String::new();
    print!("height (in cm): ");
    io::stdout().flush()?;
    loop {
        io::stdin().read_line(&mut height)?;

        if height.trim().is_empty() {
            break;
        }

        match height.trim().parse::<i32>() {
            Ok(parsed_height) => {
                if parsed_height > 300 || parsed_height < 50 {
                    eprint!("You sure? Height must be between 50 and 300cm. Try again: ");
                    io::stderr().flush()?;
                    continue;
                }
                break;
            }
            Err(_) => {
                eprint!("Height must be a valid integer. Try again: ");
                io::stderr().flush()?;
            }
        }

        height.clear();
    }

    let mut current_index = 1;

    let entries = fs::read_dir(&dir)?;
    for entry in entries {
        match entry {
            Ok(_) => current_index += 1,
            Err(e) => eprintln!("Error reading entry: {}", e),
        }
    }

    dir.push(format!("{}", current_index));

    match fs::create_dir(&dir) {
        Ok(_) => println!(
            "New record: {}",
            dir.to_str().unwrap_or("Failed to convert")
        ),
        Err(e) => panic!("{:?}", e),
    }

    let char_file = dir.join("chars.txt");
    let mut char_file = File::create(char_file)?;
    let _ = writeln!(char_file, "sex={}\nhand={}\nheight={}", sex, hand, height);

    println!(
        "\nPress 't' for 'typing', 's' for 'scrolling', 'f' for 'fidgeting' and 'n' for 'nothing' (default)"
    );

    let label_file_path = dir.join("labels.csv");
    let mut label_file = File::create(label_file_path)?;

    thread::spawn(move || {
        let start = SystemTime::now();
        let timestamp;

        match start.duration_since(UNIX_EPOCH) {
            Ok(duration) => {
                // Get the seconds part of the duration
                timestamp = duration.as_millis();
            }
            Err(e) => panic!("{:?}", e),
        }
        let _ = writeln!(label_file, "{};n", timestamp);

        let stdin = io::stdin();
        for key in stdin.keys() {
            if key.is_err() {
                eprintln!("Failed to capture the pressing of a key!");
                continue;
            }
            let key = key.unwrap();

            let start = SystemTime::now();
            let timestamp;

            match start.duration_since(UNIX_EPOCH) {
                Ok(duration) => {
                    // Get the seconds part of the duration
                    timestamp = duration.as_millis();
                }
                Err(e) => panic!("{:?}", e),
            }
            match key {
                termion::event::Key::Char('t') => {
                    let _ = writeln!(label_file, "{};t", timestamp);
                    println!("Typing")
                }
                termion::event::Key::Char('s') => {
                    let _ = writeln!(label_file, "{};s", timestamp);
                    println!("Scrolling")
                }
                termion::event::Key::Char('f') => {
                    let _ = writeln!(label_file, "{};f", timestamp);
                    println!("Fidgeting")
                }
                termion::event::Key::Char('n') => {
                    let _ = writeln!(label_file, "{};n", timestamp);
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

    let readings_file_path = dir.join("readings.csv");
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
                if skip_first_three < 500 {
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
                    if counter >= 5000 {
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
