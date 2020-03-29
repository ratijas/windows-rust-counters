use std::borrow::Cow;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io;

use tui::{
    backend::Backend,
    Frame,
    widgets::{
        Block, Borders, Chart, Paragraph, Tabs, Text, Widget,
    },
};
use tui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use tui::style::{Color, Modifier, Style};
use tui::widgets::{Axis, Dataset, Marker};

use crate::{App, CounterStats};

// colors
const COLOR_PRIMARY: Color = Color::Cyan;
const COLOR_SECONDARY: Color = Color::Magenta;
const COLOR_SECONDARY_VARIANT: Color = Color::LightMagenta;
#[allow(unused)]
const COLOR_ON_PRIMARY: Color = Color::White;
const COLOR_ON_SECONDARY: Color = Color::White;
const COLOR_ON_BACKGROUND: Color = Color::Reset;

pub fn draw<B>(f: &mut Frame<B>, app: &mut App) -> io::Result<()>
    where
        B: Backend,
{
    let area = f.size();
    // let _lock = app.stats_read();
    print!("");

    Block::default()
        .style(Style::default())
        .render(f, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(0)
        ].as_ref())
        .split(area);

    draw_header(f, app, chunks[0])?;
    let chunk = draw_tabs(f, app, chunks[1])?;
    match active_stat(app) {
        Some(stat) => draw_stat(f, app, stat, chunk)?,
        None => draw_placeholder(f, chunk)?,
    }
    Ok(())
}

fn draw_header<B>(f: &mut Frame<B>, app: &App, area: Rect) -> io::Result<()>
    where B: Backend
{
    let object = app.stats_read().meta.clone();
    let text = [
        Text::raw(&object.help_value),
    ];
    Paragraph::new(text.iter())
        .alignment(Alignment::Center)
        .block(Block::default()
            .borders(Borders::ALL)
            .title(&object.name_value)
            .title_style(Style::default().fg(COLOR_PRIMARY))
            .style(Style::default().fg(COLOR_ON_BACKGROUND))
        )
        .wrap(true)
        .render(f, area);
    Ok(())
}

/// Returns sub-chunk of the rest area.
fn draw_tabs<B>(f: &mut Frame<B>, app: &App, area: Rect) -> io::Result<Rect>
    where B: Backend
{
    let tabs = app.stats_read().counters.iter().map(|s| s.meta.name_value.clone()).collect::<Vec<_>>();
    let selected_index = app.active_counter_index().unwrap_or(usize::MAX);

    Tabs::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Counters")
                .title_style(Style::default().fg(COLOR_PRIMARY))
        )
        .titles(&tabs)
        .select(selected_index)
        .style(Style::default().fg(COLOR_ON_BACKGROUND))
        .highlight_style(Style::default().fg(COLOR_ON_SECONDARY).bg(COLOR_SECONDARY).modifier(Modifier::BOLD))
        .render(f, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(0)
        ].as_ref())
        .split(area);

    Ok(chunks[1])
}

fn draw_stat<B>(f: &mut Frame<B>, app: &App, stat: CounterStats, area: Rect) -> io::Result<()>
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

    Paragraph::new([
        Text::raw(&stat.meta.help_value),
        Text::raw("\n\n"),
    ].iter())
        .alignment(Alignment::Center)
        .wrap(true)
        .style(Style::default().fg(COLOR_ON_BACKGROUND))
        .render(f, chunks[0]);

    let text = pretty_signal(&*stat.signal.iter().cloned().collect::<Vec<_>>());
    let text = trim_lines_to_width(&text, Alignment::Right, chunks[1]);
    Paragraph::new([
        Text::raw(Cow::from(text)),
    ].iter())
        .wrap(false)
        .alignment(Alignment::Right)
        .block(Block::default()
            .title("Decoded signal")
            .title_style(Style::default().fg(COLOR_PRIMARY))
            .borders(Borders::TOP))
        .style(Style::default().fg(COLOR_SECONDARY_VARIANT))
        .render(f, chunks[1]);

    let message = &stat.decoded;
    let message = message.replace(" ", "   "); // make spaces noticeable with this font
    if let Some(figure) = app.view.font.convert(&message) {
        let string = figure.to_string();
        let string = trim_lines_to_width(&*string, Alignment::Right, chunks[2]);
        let text = [
            Text::raw(Cow::from(string))
        ];
        Paragraph::new(text.iter())
            .wrap(false)
            .alignment(Alignment::Right)
            .style(Style::default().fg(COLOR_ON_BACKGROUND).modifier(Modifier::BOLD))
            .render(f, chunks[2]);
    }

    let dataset_owned: Vec<_> = stat.instances.iter().map(|instance| {
        let data = instance.signal
            .iter()
            .rev()
            .cloned().zip((0..100).rev())
            .map(|(y, x)| (x as f64, y as f64))
            .collect::<Vec<_>>();
        let name = format!("{}: {}",
                           instance.instance_id,
                           instance.signal.back().unwrap_or(&0));
        let color = color_for(&instance.instance_id);
        (name, data, color)
    }).collect();
    let dataset_ref: Vec<_> = dataset_owned.iter().map(|(name, data, color)| {
        Dataset::default()
            .name(name)
            .marker(if stat.instances.len() <=4 { Marker::Dot } else { Marker::Braille })
            .style(Style::default().fg(*color))
            .data(&data)
    }).collect();
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
                .style(Style::default().fg(COLOR_ON_BACKGROUND))
                .labels_style(Style::default().modifier(Modifier::ITALIC))
                .bounds([0f64, 100f64]) // last 100 values
                .labels(&["Last"]),
        )
        .y_axis(
            Axis::default()
                .title("Counter value")
                .style(Style::default().fg(COLOR_ON_BACKGROUND))
                .labels_style(Style::default().modifier(Modifier::ITALIC))
                .bounds([0.0, 100.0])
                .labels(&["0", "20", "40", "60", "80", "100"]),
        )
        .datasets(&dataset_ref)
        .render(f, chunks[3]);
    Ok(())
}

fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

fn color_for<T: Hash>(t: &T) -> Color {
    let colors = [Color::Green, Color::Blue, Color::Red, Color::Magenta];
    colors[(1 + calculate_hash(t)) as usize % colors.len()]
}

fn draw_placeholder<B>(f: &mut Frame<B>, area: Rect) -> io::Result<()>
    where B: Backend
{
    Paragraph::new([].iter())
        .block(Block::default()
            .borders(Borders::ALL)
            .title("No counter selected"))
        .render(f, area);
    Ok(())
}

fn active_stat(app: &App) -> Option<CounterStats> {
    let selected_index = app.active_counter_index()?;
    app.stats_read().counters.get(selected_index).cloned()
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