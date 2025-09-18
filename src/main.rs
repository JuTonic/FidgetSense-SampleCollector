use clap::Parser;
use crossterm::style::Print;
use crossterm::terminal::ClearType;
use crossterm::{cursor, execute, terminal};
use figlet_rs::FIGfont;
use rand::prelude::*;
use serial2::SerialPort;
use std::fs::File;
use std::io::{self, BufWriter, Stdout, Write};
use std::path::Path;
use std::sync::LazyLock;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::{fs, io::BufRead, path::PathBuf};

const WARMUP_LINE_COUNT: usize = 500; // how many serial lines to discard as warm-up
const FLUSH_EVERY: usize = 5_000; // flush readings every N lines
const DEFAULT_DEVICE_NAME: &str = "/dev/serial/by-id/usb-1a86_USB_Serial-if00-port0";
const DEFAULT_DIR: &str = ".";
const BAUD: u32 = 115200;

static FIGFONT: LazyLock<FIGfont> =
    LazyLock::new(|| FIGfont::standard().expect("Failed to load FIGfont"));
const COUNTDOWN_DURATION_SEC: Duration = Duration::from_secs(1);
const COUNTDOWN_FROM: u32 = 5;
const ACTIVITY_DURATION_SEC: Duration = Duration::from_secs(15);

const TEXTS: [&str; 5] = [
    "The tortoise and the hare are often seen as representing two different approaches to life. The hare is fast and confident, often rushing ahead, while the tortoise is slow and steady, never losing focus. In the end, the tortoise won the race because it was consistent and patient.",
    "Humans have always been fascinated by the stars. We’ve sent spacecraft to distant planets, launched satellites to explore our solar system, and studied the cosmos through telescopes. One day, we may even establish colonies on Mars, but for now, we can only imagine the future of space exploration",
    "The butterfly effect is a concept in chaos theory that suggests that small causes can have large effects. It’s based on the idea that the flap of a butterfly’s wings in one part of the world could set off a chain of events leading to significant changes in another part of the world. It highlights the interconnectedness of all things.",
    "Cooking is both a science and an art. From the precise measurements of ingredients to the creativity of combining flavors, there’s something deeply satisfying about preparing a meal. Whether you're baking a cake or grilling a steak, cooking allows for endless experimentation, and every dish is a reflection of the cook’s personality.",
    "Music has the power to transport us to another time and place. It can evoke memories, stir emotions, and bring people together. From classical compositions to modern pop songs, music is a universal language that transcends borders and connects us to something greater than ourselves.",
];

#[derive(Clone, Debug)]
enum Activity {
    NOTHING,
    TYPING,
    SCROLLING,
    FIDGETING,
    OTHER,
}

const ACTIVITIES_ARR: [Activity; 8] = [
    Activity::NOTHING,
    Activity::NOTHING,
    Activity::TYPING,
    Activity::TYPING,
    Activity::SCROLLING,
    Activity::SCROLLING,
    Activity::FIDGETING,
    Activity::FIDGETING,
];

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
        let mut out = io::stdout();
        let mut rng = rand::rng();

        let mut activities: [Activity; 8] = ACTIVITIES_ARR.clone();
        activities.shuffle(&mut rng);

        for activity in activities {
            write_label_to_file(&Activity::OTHER, &mut label_file);
            start_countdown(&activity, &mut out);
            show_after_countdown_msg(&activity, &mut out);
            write_label_to_file(&activity, &mut label_file);
            thread::sleep(ACTIVITY_DURATION_SEC);
        }

        write_label_to_file(&Activity::OTHER, &mut label_file);
        print_msg("Done!\nYou are amazing!".to_string(), &mut out);
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
                if !line.trim().is_empty() {
                    write!(buffered_writer, "{};{}", now_ms(), line)?;
                    if counter > FLUSH_EVERY {
                        buffered_writer.flush()?;
                    }
                }
            }
            Err(_) => {
                // eprintln!("Error reading line: {}", e);
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

fn start_countdown(activity: &Activity, out: &mut Stdout) -> io::Result<()> {
    execute!(out, cursor::Hide)?;

    let activity_msg = get_before_activity_msg(activity);

    for n in (1..=COUNTDOWN_FROM).rev() {
        let number_str = n.to_string();
        print_msg(activity_msg.to_string() + " " + &number_str, out)?;
        thread::sleep(COUNTDOWN_DURATION_SEC);
    }

    Ok(())
}

fn get_before_activity_msg(activity: &Activity) -> String {
    match activity {
        Activity::TYPING => "Prepare to type!",
        Activity::NOTHING => "Prepare to nothing!",
        Activity::SCROLLING => "Prepare to scroll!",
        Activity::FIDGETING => "Prepare to fidget!",
        Activity::OTHER => unreachable!(),
    }
    .to_string()
}

fn show_after_countdown_msg(activity: &Activity, out: &mut Stdout) -> io::Result<()> {
    match activity {
        Activity::TYPING => {
            let mut rng = rand::rng();

            let text = TEXTS.choose(&mut rng).unwrap();

            execute!(
                out,
                terminal::Clear(ClearType::All),
                cursor::MoveTo(0, 0),
                Print("Retype this:\n\n"),
                Print(text),
                cursor::MoveToNextLine(2),
                cursor::Show
            )?;

            Ok(())
        }
        Activity::NOTHING => print_msg("Do nothing!".to_string(), out),
        Activity::SCROLLING => print_msg("Scroll!".to_string(), out),
        Activity::FIDGETING => print_msg("Fidget!".to_string(), out),
        Activity::OTHER => unreachable!(),
    }
}

fn print_msg(msg: String, out: &mut Stdout) -> io::Result<()> {
    execute!(out, cursor::Hide)?;

    let figure = FIGFONT.convert(&msg).unwrap();

    execute!(
        out,
        terminal::Clear(ClearType::All),
        cursor::MoveTo(0, 0),
        Print(figure.to_string())
    )?;

    out.flush()?;

    Ok(())
}

fn write_label_to_file(activity: &Activity, file: &mut File) -> io::Result<()> {
    let s = match activity {
        Activity::TYPING => "t",
        Activity::SCROLLING => "s",
        Activity::FIDGETING => "f",
        Activity::NOTHING => "n",
        Activity::OTHER => "o",
    };
    writeln!(file, "{};{}", now_ms(), s)?;
    Ok(())
}
