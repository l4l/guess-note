use std::io;
use std::sync::mpsc;

use anyhow::Context;
use argh::FromArgs;
use midir::{MidiInput, MidiOutput};

const NOTE_ON: u8 = 0x90;
const NOTE_OFF: u8 = 0x80;
const VELOCITY: u8 = 0x40;

#[derive(FromArgs)]
/// Guess Note arguments
struct Args {
    #[argh(option)]
    /// MIDI input port number
    port_no: Option<usize>,
    #[argh(switch, short = 'n')]
    /// wether or not ask for any cli input
    non_interactive: bool,
    #[argh(option, default = "36")]
    /// minimal note to generate
    min_note: u8,
    #[argh(option, default = "96")]
    /// maximal note to generate
    max_note: u8,
    #[argh(option, default = "150")]
    /// how long to play guessed note
    guess_play_duration_ms: u64,
}

fn note_number_to_sign(x: u8) -> String {
    const SIGN_COUNT: usize = 12;
    const SIGNS: [&str; SIGN_COUNT] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];

    format!(
        "{:>2}{}",
        SIGNS[usize::from(x) % SIGN_COUNT],
        i16::from(x) / SIGN_COUNT as i16 - 1
    )
}

fn sleep_ms(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms))
}

fn main() -> anyhow::Result<()> {
    let stdin = io::stdin();
    let mut input = String::new();

    macro_rules! read_line {
        () => {{
            input.clear();
            stdin.read_line(&mut input)?;
            input.clone()
        }};
    }

    let args = argh::from_env::<Args>();

    if args.min_note > args.max_note {
        anyhow::bail!("Note range cannot be empty");
    }

    let midi_in = MidiInput::new("guess-note-input")?;
    let midi_out = MidiOutput::new("guess-note-output")?;

    let out_ports = midi_out.ports();
    let mut in_ports = midi_in.ports();

    let port_no = if let Some(port_no) = args.port_no {
        port_no
    } else {
        if out_ports.is_empty() {
            anyhow::bail!("No available MIDI ports found");
        }

        println!("Select input port:");
        for (i, p) in out_ports.iter().enumerate() {
            println!("{}: {}", i, midi_out.port_name(p).unwrap());
        }

        read_line!()
            .trim()
            .parse()
            .context("invalid input, must be a number")?
    };
    let out_port = &out_ports[port_no];
    let in_port = &mut in_ports[port_no];

    let (tx, rx) = mpsc::channel();

    let _conn_in = midi_in.connect(
        in_port,
        "guess-note-input",
        move |_, message, _| {
            match message {
                &[x, _, z] if (x == NOTE_ON || x == NOTE_OFF) && z != 0 => {}
                _ => return,
            }
            let _ = tx.send(message[1]);
        },
        (),
    );
    let mut conn_out = midi_out.connect(out_port, "midir-test").unwrap();

    loop {
        println!("\n ~~ Guess the note! ~~");

        let guess_note =
            rand::random::<f32>() * (args.max_note - args.min_note) as f32 + args.min_note as f32;
        let guess_note = guess_note as u8;

        macro_rules! capture_next_note {
            () => {
                if let Some(x) = rx.try_iter().last() {
                    x
                } else {
                    rx.recv()?
                }
            };
        }

        macro_rules! play_guess_note {
            () => {
                play_guess_note!(NOTE_ON, guess_note);
                sleep_ms(args.guess_play_duration_ms);
                play_guess_note!(NOTE_OFF, guess_note);
            };
            ($kind:expr, $note: expr) => {
                conn_out
                    .send(&[$kind, $note, VELOCITY])
                    .map_err(|_| anyhow::anyhow!("cannot play note"))?;
            };
        }

        play_guess_note!();

        let mut note = capture_next_note!();
        if !args.non_interactive {
            loop {
                println!(
                    "Last played note is {}. Confirm your guess? y/n",
                    note_number_to_sign(note)
                );
                if read_line!().trim().to_lowercase() == "y" {
                    break;
                }

                play_guess_note!();

                note = capture_next_note!();
            }
        }

        if note == guess_note {
            println!(
                "Correct, you played the right note ({})",
                note_number_to_sign(note)
            );
        } else {
            println!(
                "Incorrect, you played {}, but the right one is {}",
                note_number_to_sign(note),
                note_number_to_sign(guess_note)
            );
        }
    }
}
