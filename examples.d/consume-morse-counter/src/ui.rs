use std::io;

use tui::{
    backend::Backend,
    Frame,
    widgets::{
        Block, Chart, Paragraph, Text, Widget, Borders, Tabs,
    },
};
use tui::style::{Color, Modifier, Style};
use tui::layout::{Constraint, Direction, Layout, Rect, Alignment};

use crate::{App, Stats};
use std::borrow::Cow;
use tui::widgets::{Axis, Dataset, Marker};

pub fn draw<B>(f: &mut Frame<B>, app: &mut App) -> io::Result<()>
    where
        B: Backend,
{
    let area = f.size();
    fix_active_counter(app);

    Block::default()
        .style(Style::default())
        .render(f, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(0)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Min(0)
        ].as_ref())
        .split(area);

    draw_header(f, app, chunks[0])?;
    draw_tabs(f, app, chunks[1])?;
    match active_stat(app) {
        Some(stat) => draw_stat(f, app, stat, chunks[2])?,
        None => draw_placeholder(f, app, chunks[2])?,
    }
    Ok(())
}

fn draw_header<B>(f: &mut Frame<B>, app: &mut App, area: Rect) -> io::Result<()>
    where B: Backend
{
    let object = app.object();
    let text = [
        Text::raw(&object.help_value),
    ];
    Paragraph::new(text.iter())
        .alignment(Alignment::Center)
        .block(Block::default()
            .title(&object.name_value)
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Magenta))
            .title_style(Style::default().fg(Color::LightMagenta).modifier(Modifier::BOLD))
        )
        .wrap(true)
        .render(f, area);
    Ok(())
}

fn draw_tabs<B>(f: &mut Frame<B>, app: &mut App, area: Rect) -> io::Result<()>
    where B: Backend
{
    let tabs = app.stats_read().iter().map(|s| s.counter.name_value.clone()).collect::<Vec<_>>();
    let selected_index = active_counter_index(app).unwrap_or(usize::MAX);

    Tabs::default()
        .block(Block::default().borders(Borders::ALL).title("Counters"))
        .titles(&tabs)
        .select(selected_index)
        .style(Style::default().fg(Color::Cyan))
        .highlight_style(Style::default().fg(Color::White).bg(Color::Magenta).modifier(Modifier::BOLD))
        .render(f, area);
    Ok(())
}

fn draw_stat<B>(f: &mut Frame<B>, app: &mut App, stat: Stats, area: Rect) -> io::Result<()>
    where B: Backend
{
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .vertical_margin(1)
        .horizontal_margin(2)
        .constraints([
            Constraint::Length(2), // help text
            Constraint::Length(2), // decoded bit stream
            Constraint::Length(6), // decoded text
            Constraint::Ratio(1, 3), // raw counter value
        ].as_ref())
        .split(area);

    Block::default()
        .borders(Borders::ALL)
        .title(&*stat.counter.name_value)
        .title_style(Style::default().fg(Color::Magenta).modifier(Modifier::BOLD))
        .render(f, area);

    Paragraph::new([
        Text::raw(&stat.counter.help_value),
        Text::raw("\n\n"),
    ].iter())
        .alignment(Alignment::Center)
        .wrap(true)
        .render(f, chunks[0]);

    if let Some(figure) = app.view.font.convert(&stat.decoded) {
        let string = figure.to_string();
        let string = trim_lines_to_width(&*string, Alignment::Right, chunks[2]);
        let text = [
            Text::raw(Cow::from(string))
        ];
        Paragraph::new(text.iter())
            .wrap(false)
            .alignment(Alignment::Right)
            .style(Style::default().fg(Color::Red).modifier(Modifier::BOLD))
            .render(f, chunks[2]);
    }

    let text = pretty_signal(&stat.signal_bool);
    let text = trim_lines_to_width(&text, Alignment::Right, chunks[1]);
    Paragraph::new([
        Text::raw(Cow::from(text)),
    ].iter())
        .wrap(false)
        .alignment(Alignment::Right)
        .block(Block::default()
            .title("Decoded signal")
            .title_style(Style::default().fg(Color::Cyan))
            .borders(Borders::TOP))
        .style(Style::default().fg(Color::Cyan))
        .render(f, chunks[1]);

    let data = stat.signal_raw
        .iter()
        .rev()
        .cloned().zip((0..100).rev())
        .map(|(y, x)| (x as f64, y as f64))
        .collect::<Vec<_>>();
    Chart::default()
        .block(
            Block::default()
                .title("Raw counter value (as can be observed via perfmon.exe)")
                .title_style(Style::default().fg(Color::Cyan))
                .borders(Borders::TOP)
        )
        .x_axis(
            Axis::default()
                .title("Time")
                .style(Style::default().fg(Color::DarkGray))
                .labels_style(Style::default().modifier(Modifier::ITALIC))
                .bounds([0f64, 100f64]) // last 100 values
                .labels(&["Last"]),
        )
        .y_axis(
            Axis::default()
                .title("Counter value")
                .style(Style::default().fg(Color::Gray))
                .labels_style(Style::default().modifier(Modifier::ITALIC))
                .bounds([0.0, 100.0])
                .labels(&["0", "20", "40", "60", "80", "100"]),
        )
        .datasets(&[
            Dataset::default()
                .name("PERF_NO_INSTANCES")
                .marker(Marker::Dot)
                .style(Style::default().fg(Color::Cyan))
                .data(&data),
        ])
        .render(f, chunks[3]);
    Ok(())
}

fn draw_placeholder<B>(f: &mut Frame<B>, _app: &mut App, area: Rect) -> io::Result<()>
    where B: Backend
{
    Paragraph::new([].iter())
        .block(Block::default()
            .borders(Borders::ALL)
            .title("No counter selected"))
        .render(f, area);
    Ok(())
}

fn fix_active_counter(app: &mut App) {
    let found = active_counter_index(app).is_some();
    let empty = app.stats_read().first().is_none();
    if !found && !empty {
        let lock = app.stats_read();
        let it = lock.first().unwrap().counter.name_index;
        drop(lock);
        app.view.active_counter = it;
    }
}

fn active_counter_index(app: &mut App) -> Option<usize> {
    app.stats_read().iter().position(|s| s.counter.name_index == app.view.active_counter)
}

fn active_stat(app: &mut App) -> Option<Stats> {
    let selected_index = active_counter_index(app)?;
    app.stats_read().get(selected_index).cloned()
}

fn trim_lines_to_width(input: &str, alignment: Alignment, area: Rect) -> String {
    fn trim(line: &str, alignment: Alignment, area: Rect) -> String {
        let w = area.width as usize;
        let line_w = line.chars().count();

        match alignment {
            Alignment::Left => line.chars().take(w).collect(),
            Alignment::Right => line.chars().skip(line_w.saturating_sub(w)).collect(),
            Alignment::Center => todo!(),
        }
    }
    let mut output = input.lines().map(|line| trim(line, alignment, area)).collect::<Vec<_>>().join("\n");
    if input.ends_with("\n") {
        output.push('\n');
    }
    output
}

/// convert raw signal to a string like ". --- ---", i.e. replace consecutive ON values with dashes.
fn pretty_signal(signal: &[bool]) -> String {
    let mut s = String::new();
    let mut it = signal.iter();
    while let Some(&value) = it.next() {
        if !value {
            s.push(' ');
        } else {
            let mut run = 1;
            let mut add_space = false;
            while let Some(&value) = it.next() {
                if value {
                    run += 1;
                } else {
                    add_space = true;
                    break;
                }
            }
            if run == 1 {
                s.push('•')
            } else {
                for _ in 0..run {
                    s.push('─');
                }
            }
            if add_space {
                s.push(' ');
            }
        }
    }
    s
}