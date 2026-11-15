use std::path::{Path, PathBuf};

use ratatui::{
    crossterm::event::{
        KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    },
    layout::Rect,
};
use unicode_width::UnicodeWidthChar;

use crate::download::{AudioFormat, Job, Msg, Worker};

pub const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Video,
    Audio,
}

impl Mode {
    pub const ALL: [Mode; 2] = [Mode::Video, Mode::Audio];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Video => "视频",
            Mode::Audio => "音频",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Mode::Video => "\u{f03d}",
            Mode::Audio => "\u{f001}",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Input,
    List,
}

impl Focus {
    fn other(self) -> Self {
        match self {
            Focus::Input => Focus::List,
            Focus::List => Focus::Input,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Status {
    Queued,
    Running {
        pct: f64,
        speed: String,
        eta: String,
    },
    Done {
        path: String,
    },
    Failed {
        msg: String,
    },
    Cancelled,
}

impl Status {
    pub fn active(&self) -> bool {
        matches!(self, Status::Queued | Status::Running { .. })
    }
}

pub struct Item {
    pub id: u64,
    pub url: String,
    pub title: Option<String>,
    pub label: String,
    pub status: Status,
}

#[derive(Clone)]
pub struct SettingsDraft {
    pub quality: String,
    pub audio_id: String,
    pub cookies: String,
    pub field: usize,
    pub cursor: usize,
    pub scrolls: [u16; 3],
    pub error: Option<String>,
}

impl SettingsDraft {
    pub fn value(&self, field: usize) -> &str {
        match field {
            0 => &self.quality,
            1 => &self.audio_id,
            _ => &self.cookies,
        }
    }

    pub fn text(&self) -> &str {
        self.value(self.field)
    }

    fn edit(&mut self, f: impl FnOnce(&mut String, &mut usize)) {
        match self.field {
            0 => f(&mut self.quality, &mut self.cursor),
            1 => f(&mut self.audio_id, &mut self.cursor),
            _ => f(&mut self.cookies, &mut self.cursor),
        }
    }

    pub fn insert(&mut self, c: char) {
        self.edit(|text, cursor| insert_char(text, cursor, c));
    }

    pub fn backspace(&mut self) {
        self.edit(backspace_char);
    }

    pub fn delete(&mut self) {
        self.edit(delete_char);
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.text().chars().count());
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.text().chars().count();
    }

    pub fn switch_field(&mut self, delta: isize) {
        self.field = (self.field as isize + delta).rem_euclid(3) as usize;
        self.cursor = self.text().chars().count();
        self.error = None;
    }

    pub fn cursor_from_click(&self, field: usize, rect: Rect, x: u16) -> usize {
        let text = self.value(field);
        let col = x.saturating_sub(rect.x) as usize + self.scrolls[field] as usize;
        let mut acc = 0;
        for (idx, c) in text.chars().enumerate() {
            let width = UnicodeWidthChar::width(c).unwrap_or(0);
            if col < acc + width {
                return idx;
            }
            acc += width;
        }
        text.chars().count()
    }
}

pub struct ProbeState {
    pub seq: u64,
    pub entries: Vec<AudioFormat>,
    pub selected: usize,
    pub offset: usize,
    pub loading: bool,
    pub error: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Input,
    Mode(Mode),
    Download,
    Row(u64),
    Cancel(u64),
    OpenSettings(usize),
    SettingsField(usize),
    SettingsSave,
    SettingsCancel,
    SettingsFetch,
    ProbePick(usize),
    ProbeCancel,
}

pub struct App {
    pub input: String,
    pub cursor: usize,
    pub input_scroll: u16,
    pub focus: Focus,
    pub mode: Mode,
    pub quality: String,
    pub audio_id: String,
    pub cookies_text: String,
    pub cookies: Option<PathBuf>,
    pub settings: Option<SettingsDraft>,
    pub probe: Option<ProbeState>,
    pub items: Vec<Item>,
    pub selected: Option<u64>,
    pub offset: usize,
    pub visible_rows: usize,
    pub mouse: (u16, u16),
    pub hits: Vec<(Rect, Hit)>,
    pub list_area: Rect,
    pub quit: bool,
    pub frame: usize,
    pub outdir: PathBuf,
    worker: Worker,
    next_id: u64,
    probe_seq: u64,
}

impl App {
    pub fn new() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let outdir = std::env::var_os("BILI_TUI_OUT")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("Downloads"));
        let cookies_text =
            std::env::var("BILI_TUI_COOKIES").unwrap_or_else(|_| "~/cookies.txt".to_string());
        let cookies = resolve_cookie(&cookies_text);
        Self {
            input: String::new(),
            cursor: 0,
            input_scroll: 0,
            focus: Focus::Input,
            mode: Mode::Video,
            quality: "1080".to_string(),
            audio_id: String::new(),
            cookies_text,
            cookies,
            settings: None,
            probe: None,
            items: Vec::new(),
            selected: None,
            offset: 0,
            visible_rows: 0,
            mouse: (0, 0),
            hits: Vec::new(),
            list_area: Rect::default(),
            quit: false,
            frame: 0,
            outdir,
            worker: Worker::new(),
            next_id: 1,
            probe_seq: 0,
        }
    }

    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
        for msg in self.worker.poll() {
            self.apply(msg);
        }
    }

    pub fn submit(&mut self) {
        let url = self.input.trim().to_string();
        if url.is_empty() {
            self.focus = Focus::Input;
            return;
        }
        let _ = std::fs::create_dir_all(&self.outdir);
        let id = self.next_id;
        self.next_id += 1;
        let label = self.format_label();
        self.items.push(Item {
            id,
            url: url.clone(),
            title: None,
            label,
            status: Status::Queued,
        });
        self.worker.enqueue(Job {
            id,
            url,
            mode: self.mode,
            quality: self.quality.clone(),
            audio_id: self.audio_id.clone(),
            outdir: self.outdir.clone(),
            cookies: self.cookies.clone(),
        });
        self.input.clear();
        self.cursor = 0;
        self.input_scroll = 0;
        self.selected = Some(id);
        self.focus = Focus::List;
        self.offset = self.items.len().saturating_sub(self.visible_rows.max(1));
    }

    fn format_label(&self) -> String {
        match self.mode {
            Mode::Video => {
                let quality = if self.quality == "best" {
                    "best".to_string()
                } else {
                    format!("≤{}", self.quality)
                };
                if self.audio_id.is_empty() {
                    format!("视频 {quality} · 音频自动")
                } else {
                    format!("视频 {quality} · 音频ID {}", self.audio_id)
                }
            }
            Mode::Audio => {
                if self.audio_id.is_empty() {
                    "音频 · 自动".to_string()
                } else {
                    format!("音频ID {}", self.audio_id)
                }
            }
        }
    }

    pub fn open_settings(&mut self, field: usize) {
        let field = field.min(2);
        let mut draft = SettingsDraft {
            quality: self.quality.clone(),
            audio_id: self.audio_id.clone(),
            cookies: self.cookies_text.clone(),
            field,
            cursor: 0,
            scrolls: [0; 3],
            error: None,
        };
        draft.cursor = draft.value(field).chars().count();
        self.settings = Some(draft);
    }

    pub fn fetch_audio_ids(&mut self) {
        if self.probe.is_some() {
            return;
        }
        let Some(url) = self.current_url() else {
            if let Some(settings) = self.settings.as_mut() {
                settings.error = Some("先粘贴链接再获取音频ID".to_string());
            }
            return;
        };
        let cookies = self.probe_cookies();
        self.probe_seq = self.probe_seq.wrapping_add(1);
        let seq = self.probe_seq;
        self.probe = Some(ProbeState {
            seq,
            entries: Vec::new(),
            selected: 0,
            offset: 0,
            loading: true,
            error: None,
        });
        self.worker.probe(url, cookies, seq);
    }

    fn current_url(&self) -> Option<String> {
        let input = self.input.trim();
        if !input.is_empty() {
            return Some(input.to_string());
        }
        if let Some(id) = self.selected
            && let Some(item) = self.items.iter().find(|item| item.id == id)
        {
            return Some(item.url.clone());
        }
        self.items.last().map(|item| item.url.clone())
    }

    fn probe_cookies(&self) -> Option<PathBuf> {
        if let Some(draft) = &self.settings {
            let text = draft.cookies.trim();
            if text.is_empty() {
                return None;
            }
            let path = expand_path(text);
            if path.is_file() {
                return Some(path);
            }
        }
        self.cookies.clone()
    }

    fn probe_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => self.probe = None,
            KeyCode::Enter => self.apply_probe_selection(),
            KeyCode::Up | KeyCode::Char('k') => self.move_probe(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_probe(1),
            KeyCode::Char('g')
                if self
                    .probe
                    .as_ref()
                    .is_some_and(|probe| probe.error.is_some()) =>
            {
                self.probe = None;
                self.fetch_audio_ids();
            }
            _ => {}
        }
    }

    fn move_probe(&mut self, delta: isize) {
        let Some(probe) = self.probe.as_mut() else {
            return;
        };
        if probe.loading || probe.error.is_some() {
            return;
        }
        let total = 1 + probe.entries.len();
        probe.selected = (probe.selected as isize + delta).clamp(0, total as isize - 1) as usize;
    }

    fn apply_probe_selection(&mut self) {
        let Some(probe) = self.probe.take() else {
            return;
        };
        if probe.loading || probe.error.is_some() {
            self.probe = Some(probe);
            return;
        }
        let id = if probe.selected == 0 {
            String::new()
        } else {
            probe
                .entries
                .get(probe.selected - 1)
                .map(|format| format.id.clone())
                .unwrap_or_default()
        };
        if let Some(settings) = self.settings.as_mut() {
            settings.audio_id = id;
            settings.cursor = settings.audio_id.chars().count();
            settings.error = None;
        }
    }

    pub fn save_settings(&mut self) {
        let Some(draft) = self.settings.clone() else {
            return;
        };
        let mut error = None;
        let quality_raw = draft.quality.trim();
        let quality = if quality_raw.eq_ignore_ascii_case("best") {
            Some("best".to_string())
        } else if let Ok(height) = quality_raw.parse::<u32>() {
            if (144..=4320).contains(&height) {
                Some(height.to_string())
            } else {
                error = Some("清晰度需在 144-4320 之间".to_string());
                None
            }
        } else {
            error = Some("清晰度填 best 或 144-4320 的数字".to_string());
            None
        };

        let audio_id = draft.audio_id.trim().to_string();
        if error.is_none() && !audio_id.is_empty() && !audio_id.chars().all(|c| c.is_ascii_digit())
        {
            error = Some("音频ID 只能填数字，留空自动选择".to_string());
        }

        let cookies_text = draft.cookies.trim().to_string();
        let mut cookies = None;
        if error.is_none() && !cookies_text.is_empty() {
            let path = expand_path(&cookies_text);
            if path.is_file() {
                cookies = Some(path);
            } else {
                error = Some(format!("cookies 文件不存在: {}", path.display()));
            }
        }

        if let Some(message) = error {
            if let Some(current) = self.settings.as_mut() {
                current.error = Some(message);
            }
            return;
        }
        let Some(quality) = quality else { return };
        self.quality = quality;
        self.audio_id = audio_id;
        self.cookies_text = cookies_text;
        self.cookies = cookies;
        self.settings = None;
    }

    pub fn cancel(&mut self, id: u64) {
        let mut cancel = false;
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id)
            && item.status.active()
        {
            item.status = Status::Cancelled;
            cancel = true;
        }
        if cancel {
            self.worker.cancel(id);
        }
    }

    pub fn remove_selected(&mut self) {
        let Some(id) = self.selected else { return };
        let Some(pos) = self.items.iter().position(|item| item.id == id) else {
            return;
        };
        if self.items[pos].status.active() {
            return;
        }
        self.items.remove(pos);
        self.selected = self
            .items
            .get(pos.min(self.items.len().saturating_sub(1)))
            .map(|item| item.id);
        self.clamp_offset();
    }

    pub fn clamp_offset(&mut self) {
        let max = self.items.len().saturating_sub(self.visible_rows.max(1));
        self.offset = self.offset.min(max);
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind == KeyEventKind::Release {
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('c') | KeyCode::Char('q') => self.quit = true,
                KeyCode::Char('u') if self.focus == Focus::Input && self.settings.is_none() => {
                    self.input.clear();
                    self.cursor = 0;
                }
                KeyCode::Char('g') if self.settings.is_some() => self.fetch_audio_ids(),
                _ => {}
            }
            return;
        }
        if self.settings.is_some() {
            if self.probe.is_some() {
                self.probe_key(key.code);
            } else {
                self.settings_key(key.code);
            }
            return;
        }
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => self.focus = self.focus.other(),
            _ => match self.focus {
                Focus::Input => self.input_key(key.code),
                Focus::List => self.list_key(key.code),
            },
        }
    }

    fn settings_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Esc => self.settings = None,
            KeyCode::Enter => self.save_settings(),
            _ => {
                let Some(draft) = self.settings.as_mut() else {
                    return;
                };
                match code {
                    KeyCode::Tab => draft.switch_field(1),
                    KeyCode::BackTab | KeyCode::Up => draft.switch_field(-1),
                    KeyCode::Down => draft.switch_field(1),
                    KeyCode::Backspace => draft.backspace(),
                    KeyCode::Delete => draft.delete(),
                    KeyCode::Left => draft.left(),
                    KeyCode::Right => draft.right(),
                    KeyCode::Home => draft.home(),
                    KeyCode::End => draft.end(),
                    KeyCode::Char(c) if !c.is_control() => draft.insert(c),
                    _ => {}
                }
            }
        }
    }

    fn input_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Enter => self.submit(),
            KeyCode::Esc => self.focus = Focus::List,
            KeyCode::Backspace => self.backspace(),
            KeyCode::Delete => self.delete(),
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.input.chars().count()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.input.chars().count(),
            KeyCode::Char(c) if !c.is_control() => self.insert(c),
            _ => {}
        }
    }

    fn list_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc | KeyCode::Enter | KeyCode::Char('i') => self.focus = Focus::Input,
            KeyCode::Char('s') => self.open_settings(0),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::PageUp => self.move_selection(-(self.visible_rows.max(1) as i32)),
            KeyCode::PageDown => self.move_selection(self.visible_rows.max(1) as i32),
            KeyCode::Char('x') => {
                if let Some(id) = self.selected {
                    self.cancel(id);
                }
            }
            KeyCode::Delete => self.remove_selected(),
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: i32) {
        if self.items.is_empty() {
            return;
        }
        let current = self
            .selected
            .and_then(|id| self.items.iter().position(|item| item.id == id));
        let next = match current {
            Some(idx) => (idx as i32 + delta).clamp(0, self.items.len() as i32 - 1) as usize,
            None if delta >= 0 => 0,
            None => self.items.len() - 1,
        };
        self.selected = Some(self.items[next].id);
        if next < self.offset {
            self.offset = next;
        } else if next >= self.offset + self.visible_rows.max(1) {
            self.offset = next + 1 - self.visible_rows.max(1);
        }
        self.clamp_offset();
    }

    fn insert(&mut self, c: char) {
        insert_char(&mut self.input, &mut self.cursor, c);
    }

    fn backspace(&mut self) {
        backspace_char(&mut self.input, &mut self.cursor);
    }

    fn delete(&mut self) {
        delete_char(&mut self.input, &mut self.cursor);
    }

    pub fn on_paste(&mut self, text: &str) {
        if self.probe.is_some() {
            return;
        }
        if let Some(draft) = self.settings.as_mut() {
            for c in text.chars().filter(|c| !c.is_control()) {
                draft.insert(c);
            }
            return;
        }
        if self.focus != Focus::Input {
            return;
        }
        for c in text.chars().filter(|c| !c.is_control()) {
            self.insert(c);
        }
    }

    pub fn on_mouse(&mut self, mouse: MouseEvent) {
        self.mouse = (mouse.column, mouse.row);
        if self.probe.is_some() {
            match mouse.kind {
                MouseEventKind::Down(MouseButton::Left) => self.click(mouse.column, mouse.row),
                MouseEventKind::ScrollUp => self.move_probe(-1),
                MouseEventKind::ScrollDown => self.move_probe(1),
                _ => {}
            }
            return;
        }
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => self.click(mouse.column, mouse.row),
            MouseEventKind::ScrollUp
                if self.settings.is_none() && point_in(self.list_area, mouse.column, mouse.row) =>
            {
                self.scroll(-1)
            }
            MouseEventKind::ScrollDown
                if self.settings.is_none() && point_in(self.list_area, mouse.column, mouse.row) =>
            {
                self.scroll(1)
            }
            _ => {}
        }
    }

    fn click(&mut self, x: u16, y: u16) {
        let hit = self
            .hits
            .iter()
            .rev()
            .find(|(rect, _)| point_in(*rect, x, y))
            .copied();
        match hit {
            Some((rect, Hit::Input)) => {
                self.focus = Focus::Input;
                self.cursor = self.index_at(rect, x);
            }
            Some((_, Hit::Mode(mode))) => self.mode = mode,
            Some((_, Hit::Download)) => self.submit(),
            Some((_, Hit::Row(id))) => {
                self.selected = Some(id);
                self.focus = Focus::List;
            }
            Some((_, Hit::Cancel(id))) => self.cancel(id),
            Some((_, Hit::OpenSettings(field))) => self.open_settings(field),
            Some((rect, Hit::SettingsField(field))) => {
                if let Some(draft) = self.settings.as_mut() {
                    let field = field.min(2);
                    let cursor = draft.cursor_from_click(field, rect, x);
                    draft.field = field;
                    draft.cursor = cursor;
                    draft.error = None;
                }
            }
            Some((_, Hit::SettingsSave)) => self.save_settings(),
            Some((_, Hit::SettingsCancel)) => self.settings = None,
            Some((_, Hit::SettingsFetch)) => self.fetch_audio_ids(),
            Some((_, Hit::ProbeCancel)) => self.probe = None,
            Some((_, Hit::ProbePick(index))) => {
                let total = self
                    .probe
                    .as_ref()
                    .map(|probe| 1 + probe.entries.len())
                    .unwrap_or(1);
                if let Some(probe) = self.probe.as_mut()
                    && !probe.loading
                    && probe.error.is_none()
                {
                    probe.selected = index.min(total - 1);
                }
                self.apply_probe_selection();
            }
            None => {}
        }
    }

    fn scroll(&mut self, delta: i32) {
        let max = self.items.len().saturating_sub(self.visible_rows.max(1));
        self.offset = (self.offset as i32 + delta).clamp(0, max as i32) as usize;
    }

    fn index_at(&self, rect: Rect, x: u16) -> usize {
        let col = x.saturating_sub(rect.x) as usize + self.input_scroll as usize;
        let mut acc = 0;
        for (idx, c) in self.input.chars().enumerate() {
            let width = UnicodeWidthChar::width(c).unwrap_or(0);
            if col < acc + width {
                return idx;
            }
            acc += width;
        }
        self.input.chars().count()
    }

    fn apply(&mut self, msg: Msg) {
        match msg {
            Msg::Started { id } => self.update(id, |item| {
                if item.status.active() {
                    item.status = Status::Running {
                        pct: 0.0,
                        speed: String::new(),
                        eta: String::new(),
                    };
                }
            }),
            Msg::Title { id, title } => self.update(id, |item| {
                if item.title.as_deref() != Some(title.as_str()) {
                    item.title = Some(title);
                }
            }),
            Msg::Progress {
                id,
                pct,
                speed,
                eta,
            } => self.update(id, |item| {
                if item.status.active() {
                    item.status = Status::Running { pct, speed, eta };
                }
            }),
            Msg::Done { id, path } => self.update(id, |item| {
                if !matches!(item.status, Status::Cancelled) {
                    item.status = Status::Done { path };
                }
            }),
            Msg::Failed { id, msg } => self.update(id, |item| {
                if item.status.active() {
                    item.status = Status::Failed { msg };
                }
            }),
            Msg::Formats { seq, entries } => {
                if let Some(probe) = self.probe.as_mut()
                    && probe.seq == seq
                {
                    probe.entries = entries;
                    probe.selected = 0;
                    probe.offset = 0;
                    probe.loading = false;
                    probe.error = None;
                }
            }
            Msg::FormatsFailed { seq, msg } => {
                if let Some(probe) = self.probe.as_mut()
                    && probe.seq == seq
                {
                    probe.loading = false;
                    probe.error = Some(msg);
                }
            }
        }
    }

    fn update(&mut self, id: u64, f: impl FnOnce(&mut Item)) {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            f(item);
        }
    }

    pub fn outdir_name(&self) -> String {
        self.outdir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.outdir.display().to_string())
    }
}

pub fn pretty_path(path: &Path) -> String {
    let text = path.to_string_lossy().into_owned();
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return text;
    };
    let home = home.to_string_lossy();
    match text.strip_prefix(home.as_ref()) {
        Some("") => "~".to_string(),
        Some(rest) if rest.starts_with('/') => format!("~{rest}"),
        _ => text,
    }
}

fn resolve_cookie(text: &str) -> Option<PathBuf> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let path = expand_path(text);
    path.is_file().then_some(path)
}

fn expand_path(text: &str) -> PathBuf {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return PathBuf::from(text);
    };
    if text == "~" {
        return home;
    }
    if let Some(rest) = text.strip_prefix("~/") {
        return home.join(rest);
    }
    PathBuf::from(text)
}

fn byte_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(idx, _)| idx)
        .unwrap_or(text.len())
}

fn insert_char(text: &mut String, cursor: &mut usize, c: char) {
    let idx = byte_index(text, *cursor);
    text.insert(idx, c);
    *cursor += 1;
}

fn backspace_char(text: &mut String, cursor: &mut usize) {
    if *cursor == 0 {
        return;
    }
    let start = byte_index(text, *cursor - 1);
    let end = byte_index(text, *cursor);
    text.replace_range(start..end, "");
    *cursor -= 1;
}

fn delete_char(text: &mut String, cursor: &mut usize) {
    if *cursor >= text.chars().count() {
        return;
    }
    let start = byte_index(text, *cursor);
    let end = byte_index(text, *cursor + 1);
    text.replace_range(start..end, "");
}

fn point_in(rect: Rect, x: u16, y: u16) -> bool {
    rect.width > 0
        && rect.height > 0
        && x >= rect.x
        && x < rect.x + rect.width
        && y >= rect.y
        && y < rect.y + rect.height
}
