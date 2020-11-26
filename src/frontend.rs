use std::io;
use termion::raw::IntoRawMode as _;
use termion::screen::AlternateScreen;
use termion::input::MouseTerminal;
use tui::backend::TermionBackend;
use tui::layout::{Layout, Direction, Constraint};
use tui::widgets::{Block, Borders};
use tui::Terminal;

pub async fn frontend() {
    //let stdout = io::stdout().into_raw_mode().expect("into raw mode");
    //let stdout = MouseTerminal::from(stdout);
    //let stdout = AlternateScreen::from(stdout);
    //let backend = TermionBackend::new(AlternateScreen::from(io::stdout()));
    let backend = TermionBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend).expect("terminal");


    let mut n = 0;

    loop {
        terminal.clear();
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(&[
                    Constraint::Percentage(50),
                    Constraint::Percentage(50),
                ][..])
                .split(f.size());

            let block = Block::default()
                .title(concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION")))
                .borders(Borders::ALL);

            f.render_widget(block, chunks[0]);

            f.render_widget(tui::widgets::Paragraph::new(n.to_string()), chunks[1]);

            n += 1;
        }).expect("draw");

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}
