use core::{time, time::Duration};
use std::io;
use std::io::Stdout;

use libafl::bolts::current_time;
use libafl::stats::{ClientStats, Stats};
use termion::event::Key;
use termion::raw::{IntoRawMode, RawTerminal};
use tui::backend::TermionBackend;
use tui::layout::Alignment;
use tui::style::{Color, Style};
use tui::widgets::{Block, Borders, Paragraph, Wrap};
use tui::Terminal;

use crate::fuzzer::terminal_stats::util::{Event, Events};
use nix::sys::signal::Signal;

pub struct TerminalStats {
    terminal: Terminal<TermionBackend<RawTerminal<Stdout>>>,
    start_time: Duration,
    corpus_size: usize,
    client_stats: Vec<ClientStats>,
    events: Events,
}

impl Clone for TerminalStats {
    fn clone(&self) -> Self {
        let stdout = io::stdout().into_raw_mode().unwrap();
        let backend = TermionBackend::new(stdout);
        Self {
            terminal: Terminal::new(backend).unwrap(),
            start_time: self.start_time,
            corpus_size: self.corpus_size,
            client_stats: self.client_stats.clone(),
            events: Events::new(),
        }
    }
}

impl Stats for TerminalStats {
    fn client_stats_mut(&mut self) -> &mut Vec<ClientStats> {
        &mut self.client_stats
    }
    fn client_stats(&self) -> &[ClientStats] {
        &self.client_stats
    }
    fn start_time(&mut self) -> time::Duration {
        self.start_time
    }

    fn display(&mut self, event_msg: String, _sender_id: u32) {
        let global_fmt = format!(
            "[{}] (GLOBAL) clients: {}, corpus: {}, objectives: {}, executions: {}, exec/sec: {}",
            event_msg,
            self.client_stats().len(),
            self.corpus_size(),
            self.objective_size(),
            self.total_execs(),
            self.execs_per_sec()
        );

        self.terminal
            .draw(|f| {
                let size = f.size();

                let p = Paragraph::new(global_fmt)
                    .block(Block::default().title("Stats").borders(Borders::ALL))
                    .style(Style::default().fg(Color::White).bg(Color::Black))
                    .alignment(Alignment::Center)
                    .wrap(Wrap { trim: true });

                f.render_widget(p, size);
            })
            .unwrap();

        // Handle input
        if let Ok(event) = self.events.next() {
            if let Event::Input(input) = event {
                match input {
                    Key::Char('q') => {
                        println!("Stopping");
                        nix::sys::signal::raise(Signal::SIGINT).unwrap();
                    }
                    _ => {}
                }
            }
        }
    }
}

impl TerminalStats {
    pub fn new() -> Self {
        let stdout = io::stdout().into_raw_mode().unwrap();
        let backend = TermionBackend::new(stdout);

        Self {
            terminal: Terminal::new(backend).unwrap(),
            start_time: current_time(),
            corpus_size: 0,
            client_stats: vec![],
            events: Events::new(),
        }
    }
}

mod util {
    use std::io;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use std::sync::mpsc::TryRecvError;
    use termion::event::Key;
    use termion::input::TermRead;

    pub enum Event<I> {
        Input(I),
        Tick,
    }

    /// A small event handler that wrap termion input and tick events. Each event
    /// type is handled in its own thread and returned to a common `Receiver`
    pub struct Events {
        rx: mpsc::Receiver<Event<Key>>,
        input_handle: thread::JoinHandle<()>,
        tick_handle: thread::JoinHandle<()>,
    }

    #[derive(Debug, Clone, Copy)]
    pub struct Config {
        pub tick_rate: Duration,
    }

    impl Default for Config {
        fn default() -> Config {
            Config {
                tick_rate: Duration::from_millis(250),
            }
        }
    }

    impl Events {
        pub fn new() -> Events {
            Events::with_config(Config::default())
        }

        pub fn with_config(config: Config) -> Events {
            let (tx, rx) = mpsc::channel();
            let input_handle = {
                let tx = tx.clone();
                thread::spawn(move || {
                    let stdin = io::stdin();
                    for evt in stdin.keys() {
                        if let Ok(key) = evt {
                            if let Err(err) = tx.send(Event::Input(key)) {
                                eprintln!("{}", err);
                                return;
                            }
                        }
                    }
                })
            };
            let tick_handle = {
                thread::spawn(move || loop {
                    if let Err(err) = tx.send(Event::Tick) {
                        eprintln!("{}", err);
                        break;
                    }
                    thread::sleep(config.tick_rate);
                })
            };
            Events {
                rx,
                input_handle,
                tick_handle,
            }
        }

        pub fn next(&self) -> Result<Event<Key>, TryRecvError> {
            self.rx.try_recv()
        }
    }
}
