use std::path::Path;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    symbols::line,
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, LineGauge, Paragraph},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{App, Focus, Hit, Mode, SPINNER, Status, pretty_path};

// Catppuccin Mocha
const CRUST: Color = Color::Rgb(0x11, 0x11, 0x1b);
const SURFACE0: Color = Color::Rgb(0x31, 0x32, 0x44);
const SURFACE1: Color = Color::Rgb(0x45, 0x47, 0x5a);
const SURFACE2: Color = Color::Rgb(0x58, 0x5b, 0x70);
const OVERLAY0: Color = Color::Rgb(0x6c, 0x70, 0x86);
const SUBTEXT0: Color = Color::Rgb(0xa6, 0xad, 0xc8);
const TEXT: Color = Color::Rgb(0xcd, 0xd6, 0xf4);
const LAVENDER: Color = Color::Rgb(0xb4, 0xbe, 0xfe);
const GREEN: Color = Color::Rgb(0xa6, 0xe3, 0xa1);
const YELLOW: Color = Color::Rgb(0xf9, 0xe2, 0xaf);
const RED: Color = Color::Rgb(0xf3, 0x8b, 0xa8);
const MAUVE: Color = Color::Rgb(0xcb, 0xa6, 0xf7);
const PINK: Color = Color::Rgb(0xf5, 0xc2, 0xe7);
const PINK_HI: Color = Color::Rgb(0xf2, 0xcd, 0xcd);

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    app.hits.clear();

    if area.width < 60 || area.height < 16 {
        let hint = Paragraph::new(vec![
            Line::from(Span::styled(
                "\u{f05e} 窗口太小",
                Style::new().fg(PINK).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled("至少需要 60 × 16", Style::new().fg(OVERLAY0))),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(hint, area);
        return;
    }

    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(1),
        Constraint::Min(4),
        Constraint::Length(1),
    ])
    .split(area);

    draw_header(frame, app, chunks[0]);
    draw_input(frame, app, chunks[1]);
    draw_toolbar(frame, app, chunks[2]);
    draw_queue(frame, app, chunks[3]);
    draw_footer(frame, app, chunks[4]);

    if app.settings.is_some() && app.probe.is_none() {
        app.hits.clear();
        draw_settings(frame, app, area);
    }
    if app.probe.is_some() {
        app.hits.clear();
        draw_probe(frame, app, area);
    }
}

fn hover(mouse: (u16, u16), rect: Rect) -> bool {
    let (x, y) = mouse;
    rect.width > 0
        && rect.height > 0
        && x >= rect.x
        && x < rect.x + rect.width
        && y >= rect.y
        && y < rect.y + rect.height
}

fn draw_header(frame: &mut Frame, app: &mut App, area: Rect) {
    let left = Line::from(vec![
        Span::styled("\u{f008} ", Style::new().fg(PINK)),
        Span::styled(
            "BILI-TUI",
            Style::new().fg(PINK).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ·  B 站下载器", Style::new().fg(OVERLAY0)),
    ]);
    frame.render_widget(Paragraph::new(left), area);

    let outdir = shorten(&app.outdir_name(), 10);
    let cookies = if app.cookies_text.trim().is_empty() {
        "cookies 关".to_string()
    } else {
        cookie_label(&app.cookies_text, 14)
    };
    let cookie_color = if app.cookies_text.trim().is_empty() {
        OVERLAY0
    } else if app.cookies.is_some() {
        GREEN
    } else {
        YELLOW
    };
    let make = |active: bool| {
        let accent = if active { PINK } else { MAUVE };
        let text = if active { PINK } else { SUBTEXT0 };
        vec![
            Span::styled("\u{f07b} ".to_string(), Style::new().fg(accent)),
            Span::styled(outdir.clone(), Style::new().fg(text)),
            Span::raw("   "),
            Span::styled("\u{f013} ".to_string(), Style::new().fg(accent)),
            Span::styled(cookies.clone(), Style::new().fg(text)),
            Span::raw(" "),
            Span::styled("\u{25cf}".to_string(), Style::new().fg(cookie_color)),
            Span::raw(" "),
        ]
    };
    let probe = make(false);
    let width = probe.iter().map(|span| span.width()).sum::<usize>() as u16;
    let right_area = Rect::new(
        area.right().saturating_sub(width).max(area.x),
        area.y,
        width.min(area.width),
        1,
    );
    let active = hover(app.mouse, right_area);
    app.hits.push((right_area, Hit::OpenSettings(2)));
    frame.render_widget(
        Paragraph::new(Line::from(make(active)).right_aligned()),
        area,
    );
}

fn draw_input(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Input;
    let border = if focused {
        PINK
    } else if hover(app.mouse, area) {
        LAVENDER
    } else {
        SURFACE1
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(border))
        .title(Line::from(vec![
            Span::styled(
                " \u{f0c1} 链接 ",
                Style::new()
                    .fg(if focused { PINK } else { OVERLAY0 })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Enter 下载 · 支持粘贴 ", Style::new().fg(OVERLAY0)),
        ]));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.hits.push((inner, Hit::Input));

    if app.input.is_empty() {
        app.input_scroll = 0;
        frame.render_widget(
            Paragraph::new(Span::styled(
                "粘贴 B 站链接，例如 https://www.bilibili.com/video/BV...",
                Style::new().fg(OVERLAY0).add_modifier(Modifier::ITALIC),
            )),
            inner,
        );
        if focused && app.settings.is_none() {
            frame.set_cursor_position(Position::new(inner.x, inner.y));
        }
        return;
    }

    let (visible, start_width) = visible_text(&app.input, app.cursor, inner.width);
    app.input_scroll = start_width;
    frame.render_widget(
        Paragraph::new(Span::styled(visible, Style::new().fg(TEXT))),
        inner,
    );
    if focused && app.settings.is_none() {
        let cursor_width: usize = app
            .input
            .chars()
            .take(app.cursor)
            .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
            .sum();
        let x = (inner.x as usize + cursor_width - start_width as usize)
            .min(inner.right().saturating_sub(1) as usize) as u16;
        frame.set_cursor_position(Position::new(x, inner.y));
    }
}

fn draw_toolbar(frame: &mut Frame, app: &mut App, area: Rect) {
    let button_label = " \u{f019} 下载 ";
    let button_width = UnicodeWidthStr::width(button_label) as u16;
    let button_area = Rect::new(
        area.right().saturating_sub(button_width),
        area.y,
        button_width.min(area.width),
        1,
    );

    let mut spans = Vec::new();
    let mut x = area.x;
    let limit = button_area.x.saturating_sub(1);

    for mode in Mode::ALL {
        let label = format!(" {} {} ", mode.icon(), mode.label());
        let width = UnicodeWidthStr::width(label.as_str()) as u16;
        if x + width > limit {
            break;
        }
        let rect = Rect::new(x, area.y, width, 1);
        let selected = app.mode == mode;
        let style = if selected {
            Style::new().bg(PINK).fg(CRUST).add_modifier(Modifier::BOLD)
        } else if hover(app.mouse, rect) {
            Style::new()
                .bg(SURFACE1)
                .fg(PINK)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().bg(SURFACE0).fg(SUBTEXT0)
        };
        app.hits.push((rect, Hit::Mode(mode)));
        spans.push(Span::styled(label, style));
        spans.push(Span::raw(" "));
        x += width + 1;
    }

    let quality = if app.quality == "best" {
        "清晰度 best".to_string()
    } else {
        format!("清晰度 \u{2264}{}", app.quality)
    };
    push_tool_button(
        app,
        area,
        &mut x,
        limit,
        format!(" {quality} "),
        Hit::OpenSettings(0),
        &mut spans,
    );

    let audio = if app.audio_id.is_empty() {
        "自动".to_string()
    } else {
        app.audio_id.clone()
    };
    push_tool_button(
        app,
        area,
        &mut x,
        limit,
        format!(" 音频ID {audio} "),
        Hit::OpenSettings(1),
        &mut spans,
    );

    frame.render_widget(Paragraph::new(Line::from(spans)), area);

    let enabled = !app.input.trim().is_empty();
    let style = if !enabled {
        Style::new().bg(SURFACE0).fg(OVERLAY0)
    } else if hover(app.mouse, button_area) {
        Style::new()
            .bg(PINK_HI)
            .fg(CRUST)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new().bg(PINK).fg(CRUST).add_modifier(Modifier::BOLD)
    };
    app.hits.push((button_area, Hit::Download));
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(button_label, style)).right_aligned()),
        area,
    );
}

fn push_tool_button(
    app: &mut App,
    area: Rect,
    x: &mut u16,
    limit: u16,
    label: String,
    hit: Hit,
    spans: &mut Vec<Span<'static>>,
) {
    let width = UnicodeWidthStr::width(label.as_str()) as u16;
    if *x + width > limit {
        return;
    }
    let rect = Rect::new(*x, area.y, width, 1);
    let style = if hover(app.mouse, rect) {
        Style::new()
            .bg(SURFACE1)
            .fg(PINK)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::new().bg(SURFACE0).fg(SUBTEXT0)
    };
    app.hits.push((rect, hit));
    spans.push(Span::styled(label, style));
    spans.push(Span::raw(" "));
    *x += width + 1;
}

fn draw_queue(frame: &mut Frame, app: &mut App, area: Rect) {
    let selected_index = app
        .selected
        .and_then(|id| app.items.iter().position(|item| item.id == id))
        .map(|idx| idx + 1)
        .unwrap_or(0);
    let count = format!(" {}/{} ", selected_index, app.items.len());
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(SURFACE1))
        .title(Line::from(vec![
            Span::styled(
                " \u{f03a} 下载队列 ",
                Style::new().fg(PINK).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{} 项 ", app.items.len()),
                Style::new().fg(OVERLAY0),
            ),
        ]))
        .title_top(Line::from(Span::styled(count, Style::new().fg(OVERLAY0))).right_aligned());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.list_area = area;

    let rows = (inner.height / 2) as usize;
    app.visible_rows = rows;
    app.clamp_offset();

    if app.items.is_empty() {
        let empty = Line::from(vec![
            Span::styled("\u{f0c1} ", Style::new().fg(PINK)),
            Span::styled("还没有任务，粘贴链接后按 Enter", Style::new().fg(OVERLAY0)),
        ]);
        frame.render_widget(Paragraph::new(empty).alignment(Alignment::Center), inner);
        return;
    }

    for (idx, item) in app.items.iter().enumerate().skip(app.offset).take(rows) {
        let y = inner.y + ((idx - app.offset) * 2) as u16;
        if y + 1 >= inner.bottom() {
            break;
        }
        let row = Rect::new(inner.x, y, inner.width, 2);
        let is_selected = app.selected == Some(item.id);
        let is_hovered = hover(app.mouse, row);
        app.hits.push((row, Hit::Row(item.id)));

        if is_selected {
            frame.render_widget(
                Paragraph::new(Span::styled("\u{258c}", Style::new().fg(PINK))),
                Rect::new(inner.x, y, 1, 1),
            );
        }

        let (icon, icon_color) = status_visual(&item.status, app.frame);
        let title = item
            .title
            .as_deref()
            .filter(|title| !title.trim().is_empty())
            .unwrap_or(&item.url);
        let title_style = if is_selected {
            Style::new().fg(PINK).add_modifier(Modifier::BOLD)
        } else if is_hovered {
            Style::new().fg(PINK)
        } else if item.title.is_some() {
            Style::new().fg(TEXT)
        } else {
            Style::new().fg(SUBTEXT0)
        };
        let title_line = Line::from(vec![
            Span::styled(icon, Style::new().fg(icon_color)),
            Span::raw(" "),
            Span::styled(title.to_string(), title_style),
        ]);
        let title_area = Rect::new(inner.x + 1, y, inner.width.saturating_sub(4), 1);
        frame.render_widget(Paragraph::new(title_line), title_area);

        if item.status.active() {
            let cancel_area = Rect::new(inner.right().saturating_sub(3), y, 3, 1);
            let cancel_style = if hover(app.mouse, cancel_area) {
                Style::new().fg(RED).add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(OVERLAY0)
            };
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(" \u{f00d} ", cancel_style))),
                cancel_area,
            );
            app.hits.push((cancel_area, Hit::Cancel(item.id)));
        }

        let detail_area = Rect::new(inner.x + 1, y + 1, inner.width.saturating_sub(2), 1);
        match &item.status {
            Status::Queued => frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("\u{f017} 排队中  ·  ", Style::new().fg(YELLOW)),
                    Span::styled(item.label.clone(), Style::new().fg(OVERLAY0)),
                ])),
                detail_area,
            ),
            Status::Running { pct, speed, eta } => {
                let mut parts = vec![format!("{:.1}%", pct.clamp(0.0, 1.0) * 100.0)];
                if !speed.is_empty() {
                    parts.push(speed.clone());
                }
                if !eta.is_empty() {
                    parts.push(format!("ETA {eta}"));
                }
                let ratio = if pct.is_finite() {
                    pct.clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let gauge = LineGauge::default()
                    .ratio(ratio)
                    .label(Line::from(Span::styled(
                        parts.join("  ·  "),
                        Style::new().fg(SUBTEXT0),
                    )))
                    .filled_style(Style::new().fg(PINK))
                    .unfilled_style(Style::new().fg(SURFACE1))
                    .line_set(line::THICK);
                frame.render_widget(gauge, detail_area);
            }
            Status::Done { path } => {
                let text = if path.is_empty() {
                    "已完成".to_string()
                } else {
                    format!("\u{f00c} {}", pretty_path(Path::new(path)))
                };
                frame.render_widget(
                    Paragraph::new(Span::styled(text, Style::new().fg(GREEN))),
                    detail_area,
                );
            }
            Status::Failed { msg } => {
                let message = msg.replace('\n', " ").chars().take(180).collect::<String>();
                frame.render_widget(
                    Paragraph::new(Line::from(vec![
                        Span::styled("\u{f00d} ", Style::new().fg(RED)),
                        Span::styled(message, Style::new().fg(RED)),
                    ])),
                    detail_area,
                );
            }
            Status::Cancelled => frame.render_widget(
                Paragraph::new(Span::styled("\u{f05e} 已取消", Style::new().fg(OVERLAY0))),
                detail_area,
            ),
        }
    }
}

fn status_visual(status: &Status, frame: usize) -> (String, Color) {
    match status {
        Status::Queued => ("\u{f017}".to_string(), YELLOW),
        Status::Running { .. } => (SPINNER[frame % SPINNER.len()].to_string(), PINK),
        Status::Done { .. } => ("\u{f00c}".to_string(), GREEN),
        Status::Failed { .. } => ("\u{f00d}".to_string(), RED),
        Status::Cancelled => ("\u{f05e}".to_string(), OVERLAY0),
    }
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let hints: &[(&str, &str)] = if let Some(probe) = &app.probe {
        if probe.error.is_some() {
            &[("g", "重试"), ("Esc", "返回")]
        } else {
            &[
                ("\u{2191}\u{2193}", "选择"),
                ("Enter", "确定"),
                ("Esc", "返回"),
            ]
        }
    } else if app.settings.is_some() {
        &[
            ("Tab", "切换"),
            ("Ctrl+G", "获取ID"),
            ("Enter", "保存"),
            ("Esc", "取消"),
        ]
    } else {
        match app.focus {
            Focus::Input => &[
                ("Tab", "焦点"),
                ("Enter", "下载"),
                ("Ctrl+U", "清空"),
                ("Esc", "列表"),
            ],
            Focus::List => &[
                ("\u{2191}\u{2193}", "选择"),
                ("x", "取消"),
                ("s", "设置"),
                ("Del", "清理"),
                ("滚轮", "滚动"),
                ("q", "退出"),
            ],
        }
    };
    let mut spans = Vec::new();
    for (index, (key, description)) in hints.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(" \u{00b7} ", Style::new().fg(OVERLAY0)));
        }
        spans.push(Span::styled(
            (*key).to_string(),
            Style::new().fg(PINK).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            description.to_string(),
            Style::new().fg(SUBTEXT0),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_settings(frame: &mut Frame, app: &mut App, area: Rect) {
    let width = 72.min(area.width.saturating_sub(4)).max(40);
    let height = 11.min(area.height.saturating_sub(2));
    let modal = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, modal);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(PINK))
        .style(Style::new().bg(SURFACE0))
        .title(Line::from(Span::styled(
            " 设置 ",
            Style::new().fg(PINK).add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(modal);
    frame.render_widget(block, modal);

    let mouse = app.mouse;
    let probe_open = app.probe.is_some();
    let hits = &mut app.hits;
    let Some(draft) = app.settings.as_mut() else {
        return;
    };

    let labels = ["清晰度上限", "音频ID", "Cookies 路径"];
    let placeholders = ["best 或 144-4320", "留空自动", "留空关闭"];
    for index in 0..3 {
        let y = inner.y + (index as u16) * 2;
        let focused = draft.field == index;
        frame.render_widget(
            Paragraph::new(Span::styled(
                labels[index],
                Style::new().fg(if focused { PINK } else { SUBTEXT0 }),
            )),
            Rect::new(inner.x + 1, y, 12, 1),
        );

        let box_area = Rect::new(inner.x + 13, y, inner.width.saturating_sub(14), 1);
        let text_area = Rect::new(box_area.x + 1, y, box_area.width.saturating_sub(2), 1);
        let is_hovered = hover(mouse, box_area);
        let background = if focused || is_hovered {
            SURFACE2
        } else {
            SURFACE1
        };
        frame
            .buffer_mut()
            .set_style(box_area, Style::new().bg(background));

        let value = draft.value(index).to_string();
        let (visible, start_width) = if value.is_empty() {
            (String::new(), 0)
        } else if focused {
            visible_text(&value, draft.cursor, text_area.width)
        } else {
            visible_text(&value, 0, text_area.width)
        };
        draft.scrolls[index] = start_width;

        if value.is_empty() {
            frame.render_widget(
                Paragraph::new(Span::styled(
                    placeholders[index],
                    Style::new().fg(OVERLAY0).add_modifier(Modifier::ITALIC),
                )),
                text_area,
            );
        } else {
            let style = if focused {
                Style::new().fg(PINK).add_modifier(Modifier::BOLD)
            } else if is_hovered {
                Style::new().fg(PINK)
            } else {
                Style::new().fg(TEXT)
            };
            frame.render_widget(Paragraph::new(Span::styled(visible, style)), text_area);
        }
        hits.push((text_area, Hit::SettingsField(index)));

        if focused && !probe_open {
            let cursor_width: usize = value
                .chars()
                .take(draft.cursor)
                .map(|c| UnicodeWidthChar::width(c).unwrap_or(0))
                .sum();
            let x = (text_area.x as usize + cursor_width - start_width as usize)
                .min(text_area.right().saturating_sub(1) as usize) as u16;
            frame.set_cursor_position(Position::new(x, text_area.y));
        }
    }

    let hint_y = inner.bottom().saturating_sub(2);
    let (hint, color) = if let Some(error) = &draft.error {
        (error.clone(), RED)
    } else {
        (
            "Ctrl+G 获取音频ID · 144-4320 · cookies留空关闭".to_string(),
            OVERLAY0,
        )
    };
    frame.render_widget(
        Paragraph::new(Span::styled(hint, Style::new().fg(color))),
        Rect::new(inner.x + 1, hint_y, inner.width.saturating_sub(2), 1),
    );

    let save_label = " 保存 ";
    let cancel_label = " 取消 ";
    let cancel_area = Rect::new(inner.right().saturating_sub(7), inner.bottom() - 1, 6, 1);
    let save_area = Rect::new(cancel_area.x.saturating_sub(7), inner.bottom() - 1, 6, 1);
    let save_style = if hover(mouse, save_area) {
        Style::new().bg(PINK).fg(CRUST).add_modifier(Modifier::BOLD)
    } else {
        Style::new()
            .bg(GREEN)
            .fg(CRUST)
            .add_modifier(Modifier::BOLD)
    };
    let cancel_style = if hover(mouse, cancel_area) {
        Style::new().bg(RED).fg(CRUST).add_modifier(Modifier::BOLD)
    } else {
        Style::new().bg(SURFACE1).fg(SUBTEXT0)
    };
    frame.render_widget(
        Paragraph::new(Span::styled(save_label, save_style)),
        save_area,
    );
    frame.render_widget(
        Paragraph::new(Span::styled(cancel_label, cancel_style)),
        cancel_area,
    );
    hits.push((save_area, Hit::SettingsSave));
    hits.push((cancel_area, Hit::SettingsCancel));

    let fetch_label = " 获取ID ";
    let fetch_area = Rect::new(inner.x + 1, inner.bottom() - 1, 8, 1);
    let fetch_style = if hover(mouse, fetch_area) {
        Style::new().bg(PINK).fg(CRUST).add_modifier(Modifier::BOLD)
    } else {
        Style::new()
            .bg(MAUVE)
            .fg(CRUST)
            .add_modifier(Modifier::BOLD)
    };
    frame.render_widget(
        Paragraph::new(Span::styled(fetch_label, fetch_style)),
        fetch_area,
    );
    hits.push((fetch_area, Hit::SettingsFetch));
}

fn draw_probe(frame: &mut Frame, app: &mut App, area: Rect) {
    let mouse = app.mouse;
    let hits = &mut app.hits;
    let Some(probe) = app.probe.as_mut() else {
        return;
    };
    let loaded = !probe.loading && probe.error.is_none();
    let total = 1 + probe.entries.len();
    let visible = if loaded { total.min(8) } else { 1 };
    let height = (visible + 3) as u16;
    let width = 72.min(area.width.saturating_sub(4)).max(40);
    let modal = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, modal);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(PINK))
        .style(Style::new().bg(SURFACE0))
        .title(Line::from(Span::styled(
            " 选择音频ID ",
            Style::new().fg(PINK).add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(modal);
    frame.render_widget(block, modal);

    if probe.loading {
        let spinner = SPINNER[app.frame % SPINNER.len()];
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(spinner, Style::new().fg(PINK)),
                Span::raw(" 正在获取音频格式…"),
            ])),
            Rect::new(inner.x + 1, inner.y, inner.width.saturating_sub(2), 1),
        );
    } else if let Some(error) = &probe.error {
        frame.render_widget(
            Paragraph::new(Span::styled(error.clone(), Style::new().fg(RED))),
            Rect::new(inner.x + 1, inner.y, inner.width.saturating_sub(2), 1),
        );
    } else {
        let visible = inner.height.saturating_sub(1) as usize;
        if probe.selected < probe.offset {
            probe.offset = probe.selected;
        } else if probe.selected >= probe.offset + visible {
            probe.offset = probe.selected + 1 - visible;
        }
        let total = 1 + probe.entries.len();
        probe.offset = probe.offset.min(total.saturating_sub(visible));

        for index in probe.offset..(probe.offset + visible).min(total) {
            let y = inner.y + (index - probe.offset) as u16;
            let row = Rect::new(inner.x, y, inner.width, 1);
            let selected = probe.selected == index;
            let is_hovered = hover(mouse, row);
            hits.push((row, Hit::ProbePick(index)));
            if selected {
                frame.render_widget(
                    Paragraph::new(Span::styled("\u{258c}", Style::new().fg(PINK))),
                    Rect::new(inner.x, y, 1, 1),
                );
            }
            let label = if index == 0 {
                "自动选择（不指定音频ID）".to_string()
            } else {
                let format = &probe.entries[index - 1];
                if format.abr > 0.0 {
                    format!(
                        "{} · {:.0} kbps · {} · {}",
                        format.id, format.abr, format.codec, format.ext
                    )
                } else {
                    format!("{} · {} · {}", format.id, format.codec, format.ext)
                }
            };
            let style = if selected {
                Style::new().fg(PINK).add_modifier(Modifier::BOLD)
            } else if is_hovered {
                Style::new().fg(PINK)
            } else {
                Style::new().fg(TEXT)
            };
            frame.render_widget(
                Paragraph::new(Span::styled(label, style)),
                Rect::new(inner.x + 1, y, inner.width.saturating_sub(2), 1),
            );
        }
    }

    let hint_y = inner.bottom().saturating_sub(1);
    let hint = if probe.loading {
        "获取中…"
    } else if probe.error.is_some() {
        "g 重试 · Esc 返回"
    } else {
        "\u{2191}\u{2193} 选择 · Enter 确定"
    };
    frame.render_widget(
        Paragraph::new(Span::styled(hint, Style::new().fg(OVERLAY0))),
        Rect::new(inner.x + 1, hint_y, inner.width.saturating_sub(10), 1),
    );

    let cancel_area = Rect::new(inner.right().saturating_sub(7), hint_y, 6, 1);
    let cancel_style = if hover(mouse, cancel_area) {
        Style::new().bg(RED).fg(CRUST).add_modifier(Modifier::BOLD)
    } else {
        Style::new().bg(SURFACE1).fg(SUBTEXT0)
    };
    frame.render_widget(
        Paragraph::new(Span::styled(" 返回 ", cancel_style)),
        cancel_area,
    );
    hits.push((cancel_area, Hit::ProbeCancel));
}

fn visible_text(text: &str, cursor: usize, width: u16) -> (String, u16) {
    if width == 0 {
        return (String::new(), 0);
    }
    let chars: Vec<char> = text.chars().collect();
    let cursor = cursor.min(chars.len());
    let cursor_width: usize = chars[..cursor]
        .iter()
        .map(|c| UnicodeWidthChar::width(*c).unwrap_or(0))
        .sum();
    let scroll = cursor_width.saturating_sub(width as usize - 1);
    let mut start = chars.len();
    let mut start_width = 0;
    let mut current = 0;
    for (idx, c) in chars.iter().enumerate() {
        let char_width = UnicodeWidthChar::width(*c).unwrap_or(0);
        if current + char_width > scroll {
            start = idx;
            start_width = current;
            break;
        }
        current += char_width;
    }
    let visible: String = chars[start..].iter().collect();
    (visible, start_width as u16)
}

fn cookie_label(text: &str, max: usize) -> String {
    let name = Path::new(text)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| text.to_string());
    shorten(&name, max)
}

fn shorten(text: &str, max: usize) -> String {
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    let tail: String = text.chars().skip(count - max + 1).collect();
    format!("\u{2026}{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, Focus, Hit, Item, Mode, ProbeState, Status};
    use crate::download::AudioFormat;
    use ratatui::{
        Terminal,
        backend::TestBackend,
        crossterm::event::{
            KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
        },
    };

    fn render(app: &mut App) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();
        terminal
    }

    fn find_hit(app: &App, hit: Hit) -> Rect {
        app.hits
            .iter()
            .find_map(|(rect, candidate)| (*candidate == hit).then_some(*rect))
            .expect("hit rect")
    }

    #[test]
    fn hover_highlights_mode_chip_and_click_selects() {
        let mut app = App::new();
        let mut terminal = render(&mut app);
        let rect = find_hit(&app, Hit::Mode(Mode::Audio));
        let before = terminal
            .backend()
            .buffer()
            .cell((rect.x + 1, rect.y))
            .unwrap()
            .bg;
        assert_eq!(before, SURFACE0);

        app.on_mouse(MouseEvent {
            kind: MouseEventKind::Moved,
            column: rect.x + 1,
            row: rect.y,
            modifiers: KeyModifiers::NONE,
        });
        terminal = render(&mut app);
        let after = terminal
            .backend()
            .buffer()
            .cell((rect.x + 1, rect.y))
            .unwrap()
            .bg;
        assert_eq!(after, SURFACE1);

        app.on_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x + 1,
            row: rect.y,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.mode, Mode::Audio);
    }

    #[test]
    fn click_input_moves_cursor() {
        let mut app = App::new();
        app.input = "BV1GJ411x7h7".to_string();
        app.cursor = 12;
        app.focus = Focus::List;
        render(&mut app);
        let rect = find_hit(&app, Hit::Input);

        app.on_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x + 3,
            row: rect.y,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.focus, Focus::Input);
        assert_eq!(app.cursor, 3);
    }

    #[test]
    fn wheel_scrolls_the_queue() {
        let mut app = App::new();
        app.items = (1..=40)
            .map(|id| Item {
                id,
                url: format!("https://example.com/{id}"),
                title: None,
                label: "视频".to_string(),
                status: Status::Queued,
            })
            .collect();
        app.selected = Some(1);
        render(&mut app);
        assert!(app.visible_rows > 0);

        let area = app.list_area;
        app.on_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: area.x + area.width / 2,
            row: area.y + area.height / 2,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.offset, 1);
    }

    #[test]
    fn empty_ui_keeps_terminal_background_transparent() {
        let mut app = App::new();
        let terminal = render(&mut app);
        let cell = terminal.backend().buffer().cell((50, 20)).unwrap();
        assert_eq!(cell.bg, Color::Reset);
    }

    #[test]
    fn settings_save_applies_quality_and_audio_id() {
        let mut app = App::new();
        app.open_settings(0);
        if let Some(draft) = app.settings.as_mut() {
            draft.quality.clear();
            draft.cursor = 0;
            draft.cookies.clear();
        }
        for c in "720".chars() {
            app.on_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE));
        }
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        app.settings.as_mut().unwrap().audio_id = "30280".to_string();
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert_eq!(app.quality, "720");
        assert_eq!(app.audio_id, "30280");
        assert!(app.cookies.is_none());
        assert!(app.settings.is_none());
    }

    #[test]
    fn settings_reject_missing_cookie_file() {
        let mut app = App::new();
        app.open_settings(2);
        app.settings.as_mut().unwrap().cookies = "/definitely/missing.txt".to_string();
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.settings.is_some());
        assert!(app.settings.as_ref().unwrap().error.is_some());
    }

    #[test]
    fn probe_picker_up_down_enter_selects_audio_id() {
        let mut app = App::new();
        app.open_settings(1);
        app.probe = Some(ProbeState {
            seq: 1,
            entries: vec![
                AudioFormat {
                    id: "30280".to_string(),
                    codec: "mp4a.40.2".to_string(),
                    abr: 203.8,
                    ext: "m4a".to_string(),
                },
                AudioFormat {
                    id: "30216".to_string(),
                    codec: "mp4a.40.5".to_string(),
                    abr: 43.9,
                    ext: "m4a".to_string(),
                },
            ],
            selected: 0,
            offset: 0,
            loading: false,
            error: None,
        });
        render(&mut app);
        find_hit(&app, Hit::ProbePick(1));

        app.on_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        app.on_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        assert!(app.probe.is_none());
        assert_eq!(app.settings.as_ref().unwrap().audio_id, "30280");
    }

    #[test]
    fn settings_modal_exposes_mouse_hits() {
        let mut app = App::new();
        app.open_settings(0);
        render(&mut app);
        find_hit(&app, Hit::SettingsSave);
        find_hit(&app, Hit::SettingsCancel);
        find_hit(&app, Hit::SettingsField(1));
    }
}
