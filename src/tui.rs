mod catalog;
mod editor;
mod execution;

use std::{
    io::{self, IsTerminal as _},
    sync::mpsc::{Receiver, TryRecvError},
    time::Duration,
};

use catalog::{
    CATEGORY_COUNT, Category, FieldState, TOOLS, ToolDef, adjust_field, build_args,
    command_preview, find_tool, new_form, toggle_field, tool_id, tools_in, visible_fields,
};
use editor::Editor;
use execution::{Execution, clipboard_text, format_execution};
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Wrap},
};
use unicode_width::{UnicodeWidthChar as _, UnicodeWidthStr as _};
use vutils::{
    Result, VutilsError,
    config::{DEFAULT_TUI_HOME, UserConfig},
};

const MIN_WIDTH: u16 = 72;
const MIN_HEIGHT: u16 = 20;
const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(50);
const OPERATIONS_WIDTH: u16 = 27;
const FIELD_LABEL_WIDTH: usize = 17;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Operations,
    Parameters,
    Input,
    Output,
}

enum RunState {
    Idle,
    Running {
        tool_id: String,
        tool_name: &'static str,
        refresh_config: bool,
        receiver: Receiver<std::result::Result<Execution, String>>,
    },
}

#[cfg(test)]
fn default_home_ids() -> Vec<String> {
    DEFAULT_TUI_HOME
        .iter()
        .map(|value| (*value).to_owned())
        .collect()
}

fn default_home_tools() -> Vec<usize> {
    DEFAULT_TUI_HOME
        .iter()
        .filter_map(|shortcut| find_tool(shortcut))
        .collect()
}

fn replace_form_value(tool: &ToolDef, form: &mut [FieldState], key: &str, value: &str) {
    let Some(index) = tool.fields.iter().position(|field| field.key == key) else {
        return;
    };
    form[index].replace_value(&tool.fields[index], value);
}

fn configured_form(tool: &ToolDef, config: Option<&UserConfig>) -> Vec<FieldState> {
    let mut form = new_form(tool);
    if matches!(tool.base, ["enc"] | ["dec"]) {
        let (source, source_value) = match config {
            Some(config) if config.password_env().is_some() => (
                "environment",
                config.password_env().unwrap_or_default().to_owned(),
            ),
            Some(config) if config.password_file().is_some() => (
                "file",
                config
                    .password_file()
                    .map_or_else(String::new, |path| path.display().to_string()),
            ),
            _ => ("direct", String::new()),
        };
        replace_form_value(tool, &mut form, "password_source", source);
        if source == "environment" {
            replace_form_value(tool, &mut form, "password_env", &source_value);
        } else if source == "file" {
            replace_form_value(tool, &mut form, "password_file", &source_value);
        }
        if tool.base == ["enc"]
            && let Some(config) = config
        {
            replace_form_value(tool, &mut form, "algorithm", config.crypto_algorithm());
        }
        return form;
    }

    let Some(config) = config else {
        return form;
    };

    if matches!(tool.base.first(), Some(&"vruno")) {
        let values = [
            (
                "collection",
                config
                    .vruno_collection()
                    .map_or_else(String::new, |path| path.display().to_string()),
            ),
            (
                "openapi",
                config
                    .vruno_openapi()
                    .map_or_else(String::new, |path| path.display().to_string()),
            ),
        ];
        for (key, value) in values {
            if value.is_empty() {
                continue;
            }
            replace_form_value(tool, &mut form, key, &value);
        }
    } else if let ["config", "set", key] = tool.base
        && let Ok(value) = config.get(key)
        && let (Some(state), Some(definition)) = (form.first_mut(), tool.fields.first())
    {
        state.replace_value(definition, &value);
    }
    form
}

fn empty_output(tool: &ToolDef) -> String {
    format!(
        "No output for {} yet. Review its parameters and press Ctrl-R to run.",
        tool.name
    )
}

fn resolves_to_config_command(tool: &ToolDef) -> bool {
    matches!(tool.base.first(), Some(&"config")) || tool.base == ["vruno", "configure"]
}

fn resolve_configured_home(config: &UserConfig) -> (Vec<usize>, Option<String>) {
    let (mut home_tools, unknown) = resolve_home_tools(&config.tui_home());
    if home_tools.is_empty() {
        home_tools = default_home_tools();
        return (
            home_tools,
            Some("Configured Home has no known operations; using defaults".into()),
        );
    }
    if unknown.is_empty() {
        (home_tools, None)
    } else {
        (
            home_tools,
            Some(format!(
                "Ignored unknown Home shortcuts: {}",
                unknown.join(", ")
            )),
        )
    }
}

fn format_exit_status(status: Option<i32>) -> String {
    status.map_or_else(|| "signal".into(), |code| code.to_string())
}

fn background_status(tool_name: &str, success: bool, elapsed: Duration, exit: &str) -> String {
    format!(
        "{} {} in {:.2?} · exit {exit}",
        tool_name,
        if success {
            "completed in background"
        } else {
            "failed in background"
        },
        elapsed,
    )
}

fn resolve_home_tools(shortcuts: &[String]) -> (Vec<usize>, Vec<String>) {
    let mut tools = Vec::new();
    let mut unknown = Vec::new();
    for shortcut in shortcuts {
        match find_tool(shortcut) {
            Some(index) if !tools.contains(&index) => tools.push(index),
            Some(_) => {}
            None => unknown.push(shortcut.clone()),
        }
    }
    (tools, unknown)
}

struct App {
    category: usize,
    tool_selections: [usize; CATEGORY_COUNT],
    home_tools: Vec<usize>,
    config: Option<UserConfig>,
    form: Vec<FieldState>,
    field_selection: usize,
    editing_field: bool,
    focus: Focus,
    input: Editor,
    output: String,
    output_scroll: u16,
    clipboard_value: Option<String>,
    run_state: RunState,
    status: String,
    command: Option<Editor>,
    show_help: bool,
    should_quit: bool,
}

impl App {
    fn new() -> Self {
        match UserConfig::load() {
            Ok(config) => Self::from_config(Some(config), None),
            Err(error) => Self::from_config(
                None,
                Some(format!(
                    "Could not load config; using default Home: {error}"
                )),
            ),
        }
    }

    fn from_config(config: Option<UserConfig>, warning: Option<String>) -> Self {
        let (home_tools, home_warning) = config
            .as_ref()
            .map_or_else(|| (default_home_tools(), None), resolve_configured_home);
        let status = warning.or(home_warning);
        let tool = &TOOLS[home_tools[0]];
        let form = configured_form(tool, config.as_ref());
        Self {
            category: 0,
            tool_selections: [0; CATEGORY_COUNT],
            home_tools,
            config,
            form,
            field_selection: 0,
            editing_field: false,
            focus: Focus::Operations,
            input: Editor::default(),
            output: empty_output(tool),
            output_scroll: 0,
            clipboard_value: None,
            run_state: RunState::Idle,
            status: status.unwrap_or_else(|| tool.description.into()),
            command: None,
            show_help: false,
            should_quit: false,
        }
    }

    fn category(&self) -> Category {
        Category::ALL[self.category]
    }

    fn tool_indices(&self) -> Vec<usize> {
        if self.category() == Category::Home {
            self.home_tools.clone()
        } else {
            tools_in(self.category())
        }
    }

    fn tool(&self) -> &'static ToolDef {
        let indices = self.tool_indices();
        &TOOLS[indices[self.tool_selections[self.category]]]
    }

    fn visible_field_indices(&self) -> Vec<usize> {
        visible_fields(self.tool(), &self.form)
    }

    fn selected_field_index(&self) -> Option<usize> {
        self.visible_field_indices()
            .get(self.field_selection)
            .copied()
    }

    fn is_running(&self) -> bool {
        matches!(self.run_state, RunState::Running { .. })
    }

    fn is_home_tool(&self, index: usize) -> bool {
        self.home_tools.contains(&index)
    }

    fn load_selected_tool(&mut self) {
        let tool = *self.tool();
        self.form = configured_form(&tool, self.config.as_ref());
        self.field_selection = 0;
        self.editing_field = false;
        self.input.clear();
        self.output = empty_output(&tool);
        self.output_scroll = 0;
        self.clipboard_value = None;
        self.status = tool.description.into();
    }

    fn toggle_selected_home_tool(&mut self) {
        let tool_index = self.tool_indices()[self.tool_selections[self.category]];
        let mut home_tools = self.home_tools.clone();
        let message = if let Some(position) = home_tools
            .iter()
            .position(|candidate| *candidate == tool_index)
        {
            if home_tools.len() == 1 {
                self.status = "Home needs at least one shortcut".into();
                return;
            }
            home_tools.remove(position);
            format!("Removed {} from Home", TOOLS[tool_index].name)
        } else {
            home_tools.push(tool_index);
            format!("Added {} to Home", TOOLS[tool_index].name)
        };
        self.persist_home(home_tools, false, message);
    }

    fn reset_home(&mut self) {
        self.persist_home(
            default_home_tools(),
            true,
            "Home restored to default shortcuts".into(),
        );
    }

    fn persist_home(&mut self, home_tools: Vec<usize>, reset: bool, message: String) {
        let loaded = self.config.clone().map_or_else(UserConfig::load, Ok);
        let mut config = match loaded {
            Ok(config) => config,
            Err(error) => {
                self.status = format!("Cannot update Home: {error}");
                return;
            }
        };
        let update = if reset {
            config.unset("tui.home")
        } else {
            let shortcuts = home_tools
                .iter()
                .map(|index| tool_id(&TOOLS[*index]))
                .collect::<Vec<_>>()
                .join(",");
            config.set("tui.home", &shortcuts)
        };
        if let Err(error) = update.and_then(|()| config.save()) {
            self.status = format!("Cannot update Home: {error}");
            return;
        }

        self.config = Some(config);
        self.home_tools = home_tools;
        if self.category() == Category::Home {
            self.tool_selections[self.category] =
                self.tool_selections[self.category].min(self.home_tools.len().saturating_sub(1));
            self.load_selected_tool();
        }
        self.status = message;
    }

    fn switch_category(&mut self, direction: isize) {
        self.category = if direction.is_negative() {
            self.category.checked_sub(1).unwrap_or(CATEGORY_COUNT - 1)
        } else {
            (self.category + 1) % CATEGORY_COUNT
        };
        self.focus = Focus::Operations;
        self.load_selected_tool();
    }

    fn select_category(&mut self, index: usize) {
        if index < CATEGORY_COUNT && index != self.category {
            self.category = index;
            self.focus = Focus::Operations;
            self.load_selected_tool();
        }
    }

    fn move_tool(&mut self, direction: isize) {
        let count = self.tool_indices().len();
        let selection = &mut self.tool_selections[self.category];
        let previous = *selection;
        *selection = selection
            .saturating_add_signed(direction)
            .min(count.saturating_sub(1));
        if *selection != previous {
            self.load_selected_tool();
        }
    }

    fn activate_tool(&mut self) {
        let tool = *self.tool();
        if !self.visible_field_indices().is_empty() {
            self.focus = Focus::Parameters;
        } else if tool.uses_input {
            self.focus = Focus::Input;
        } else {
            self.start_execution();
        }
    }

    fn move_field(&mut self, direction: isize) {
        let count = self.visible_field_indices().len();
        self.field_selection = self
            .field_selection
            .saturating_add_signed(direction)
            .min(count.saturating_sub(1));
    }

    fn adjust_selected_field(&mut self, direction: i8, large_step: bool) {
        let Some(index) = self.selected_field_index() else {
            return;
        };
        let definition = self.tool().fields[index];
        adjust_field(&definition, &mut self.form[index], direction, large_step);
        self.clamp_field_selection();
    }

    fn toggle_selected_field(&mut self) {
        let Some(index) = self.selected_field_index() else {
            return;
        };
        let definition = self.tool().fields[index];
        toggle_field(&definition, &mut self.form[index]);
        self.clamp_field_selection();
    }

    fn edit_selected_field(&mut self) {
        let Some(index) = self.selected_field_index() else {
            return;
        };
        if self.form[index].is_editable() {
            self.editing_field = true;
            self.status = format!(
                "Editing {} · Enter accepts · Esc returns",
                self.tool().fields[index].label
            );
        } else {
            self.toggle_selected_field();
        }
    }

    fn handle_field_editor_key(&mut self, key: KeyEvent) {
        let Some(index) = self.selected_field_index() else {
            self.editing_field = false;
            return;
        };
        let numeric = self.form[index].is_numeric();
        if let Some(editor) = self.form[index].editor_mut() {
            editor.handle_key(key, false, numeric);
        }
    }

    fn paste_into_field(&mut self, value: &str) {
        let Some(index) = self.selected_field_index() else {
            return;
        };
        let numeric = self.form[index].is_numeric();
        let filtered;
        let value = if numeric {
            filtered = value
                .chars()
                .filter(|character| character.is_ascii_digit())
                .collect::<String>();
            &filtered
        } else {
            value
        };
        if let Some(editor) = self.form[index].editor_mut() {
            editor.insert_str(value, false);
        }
    }

    fn clamp_field_selection(&mut self) {
        self.field_selection = self
            .field_selection
            .min(self.visible_field_indices().len().saturating_sub(1));
    }

    fn cycle_focus(&mut self, backwards: bool) {
        let fields = !self.visible_field_indices().is_empty();
        let input = self.tool().uses_input;
        self.focus = match (self.focus, backwards) {
            (Focus::Operations, false) if fields => Focus::Parameters,
            (Focus::Operations, false) if input => Focus::Input,
            (Focus::Operations, false) => Focus::Output,
            (Focus::Parameters, false) if input => Focus::Input,
            (Focus::Parameters, false) => Focus::Output,
            (Focus::Input, false) => Focus::Output,
            (Focus::Output, false) => Focus::Operations,
            (Focus::Operations, true) => Focus::Output,
            (Focus::Parameters, true) => Focus::Operations,
            (Focus::Input, true) if fields => Focus::Parameters,
            (Focus::Input, true) => Focus::Operations,
            (Focus::Output, true) if input => Focus::Input,
            (Focus::Output, true) if fields => Focus::Parameters,
            (Focus::Output, true) => Focus::Operations,
        };
    }

    fn refresh_config(&mut self) -> std::result::Result<Option<String>, String> {
        let config = UserConfig::load().map_err(|error| error.to_string())?;
        let (home_tools, warning) = resolve_configured_home(&config);
        self.config = Some(config);
        self.home_tools = home_tools;
        self.tool_selections[Category::Home as usize] = self.tool_selections
            [Category::Home as usize]
            .min(self.home_tools.len().saturating_sub(1));
        if self.category() == Category::Home {
            self.load_selected_tool();
        }
        Ok(warning)
    }

    fn start_execution(&mut self) {
        if self.is_running() {
            self.status = "A command is already running".into();
            return;
        }
        let tool = *self.tool();
        let args = match build_args(&tool, &self.form) {
            Ok(args) => args,
            Err(error) => {
                let visible = self.visible_field_indices();
                self.field_selection = visible
                    .iter()
                    .position(|index| *index == error.field_index)
                    .unwrap_or(0);
                self.focus = Focus::Parameters;
                self.status = error.message;
                return;
            }
        };
        let input = if tool.uses_input {
            self.input.value().into_bytes()
        } else {
            Vec::new()
        };
        match execution::spawn(args, input) {
            Ok(receiver) => {
                self.output = "Running…".into();
                self.output_scroll = 0;
                self.clipboard_value = None;
                self.run_state = RunState::Running {
                    tool_id: tool_id(&tool),
                    tool_name: tool.name,
                    refresh_config: resolves_to_config_command(&tool),
                    receiver,
                };
                self.status = format!("Running {}", tool.name);
                self.focus = Focus::Output;
            }
            Err(error) => self.status = error,
        }
    }

    fn poll_execution(&mut self) -> bool {
        let (run_tool_id, run_tool_name, refresh_config, received) = match &self.run_state {
            RunState::Idle => return false,
            RunState::Running {
                tool_id,
                tool_name,
                refresh_config,
                receiver,
            } => (
                tool_id.clone(),
                *tool_name,
                *refresh_config,
                receiver.try_recv(),
            ),
        };
        match received {
            Ok(Ok(execution)) => {
                let success = execution.status == Some(0);
                let exit = format_exit_status(execution.status);
                let config_message = if success && refresh_config {
                    match self.refresh_config() {
                        Ok(warning) => warning,
                        Err(error) => Some(format!("could not reload configuration: {error}")),
                    }
                } else {
                    None
                };
                let same_tool = tool_id(self.tool()) == run_tool_id;
                if same_tool {
                    self.clipboard_value = clipboard_text(&execution);
                    self.output = format_execution(&execution);
                    self.output_scroll = 0;
                    self.status = format!(
                        "{} in {:.2?} · exit {exit}",
                        if success { "Completed" } else { "Failed" },
                        execution.elapsed,
                    );
                } else {
                    self.status =
                        background_status(run_tool_name, success, execution.elapsed, &exit);
                }
                if let Some(message) = config_message {
                    self.status.push_str(" · ");
                    self.status.push_str(&message);
                }
                self.run_state = RunState::Idle;
                true
            }
            Ok(Err(error)) => {
                if tool_id(self.tool()) == run_tool_id {
                    self.output = error.clone();
                    self.output_scroll = 0;
                    self.clipboard_value = None;
                    self.status = error;
                } else {
                    self.status = format!("{run_tool_name} failed in background: {error}");
                }
                self.run_state = RunState::Idle;
                true
            }
            Err(TryRecvError::Disconnected) => {
                if tool_id(self.tool()) == run_tool_id {
                    self.output = "The command worker stopped without a result".into();
                    self.output_scroll = 0;
                    self.clipboard_value = None;
                    self.status = "Command worker disconnected".into();
                } else {
                    self.status = format!("{run_tool_name} worker disconnected in background");
                }
                self.run_state = RunState::Idle;
                true
            }
            Err(TryRecvError::Empty) => false,
        }
    }

    fn copy_output(&mut self) {
        let Some(value) = self.clipboard_value.as_ref() else {
            self.status = "No UTF-8 command output is available to copy".into();
            return;
        };
        match arboard::Clipboard::new().and_then(|mut clipboard| clipboard.set_text(value)) {
            Ok(()) => self.status = "Output copied to the clipboard".into(),
            Err(error) => self.status = format!("Clipboard unavailable: {error}"),
        }
    }

    fn start_command(&mut self) {
        self.command = Some(Editor::default());
        self.status = "Vim command · Enter executes · Esc cancels".into();
    }

    fn handle_command_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.command = None;
                self.status = self.tool().description.into();
            }
            KeyCode::Enter => {
                let command = self
                    .command
                    .take()
                    .map_or_else(String::new, |editor| editor.value());
                self.execute_command(&command);
            }
            KeyCode::Backspace if self.command.as_ref().is_some_and(Editor::is_empty) => {
                self.command = None;
                self.status = self.tool().description.into();
            }
            _ => {
                if let Some(editor) = &mut self.command {
                    editor.handle_key(key, false, false);
                }
            }
        }
    }

    fn execute_command(&mut self, command: &str) {
        let command = command.trim().to_ascii_lowercase();
        match command.as_str() {
            "q" | "q!" | "qa" | "qa!" | "qall" | "qall!" => self.request_quit(),
            "" => self.status = self.tool().description.into(),
            _ => self.status = format!("Unknown command: :{command}"),
        }
    }

    fn request_quit(&mut self) {
        if self.is_running() {
            self.status = "Wait for the running command before quitting".into();
        } else {
            self.should_quit = true;
        }
    }
}

pub fn run() -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(VutilsError::InvalidInput(
            "the TUI requires an interactive terminal".into(),
        ));
    }
    ratatui::run(run_app).map_err(VutilsError::from)
}

fn run_app(terminal: &mut DefaultTerminal) -> io::Result<()> {
    let mut app = App::new();
    terminal.draw(|frame| render(frame, &app))?;
    while !app.should_quit {
        let mut redraw = app.poll_execution();
        if event::poll(EVENT_POLL_INTERVAL)? {
            match event::read()? {
                Event::Key(key)
                    if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) =>
                {
                    handle_key(&mut app, key);
                }
                Event::Paste(value) => {
                    if app.editing_field {
                        app.paste_into_field(&value);
                    } else if app.focus == Focus::Input {
                        app.input.insert_str(&value, true);
                    }
                }
                _ => {}
            }
            redraw = true;
        }
        if redraw && !app.should_quit {
            terminal.draw(|frame| render(frame, &app))?;
        }
    }
    Ok(())
}

fn handle_key(app: &mut App, key: KeyEvent) {
    if app.show_help {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('?')) {
            app.show_help = false;
        }
        return;
    }

    if app.command.is_some() {
        app.handle_command_key(key);
        return;
    }

    if app.editing_field {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                app.editing_field = false;
                app.status = app
                    .selected_field_index()
                    .map_or(app.tool().description, |index| {
                        app.tool().fields[index].help
                    })
                    .into();
            }
            KeyCode::Tab | KeyCode::BackTab => {
                app.editing_field = false;
                app.cycle_focus(key.code == KeyCode::BackTab);
            }
            _ => app.handle_field_editor_key(key),
        }
        return;
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('c') => app.request_quit(),
            KeyCode::Char('r') => app.start_execution(),
            _ if app.focus == Focus::Input => {
                app.input.handle_key(key, true, false);
            }
            _ => {}
        }
        return;
    }

    match key.code {
        KeyCode::F(5) => {
            app.start_execution();
            return;
        }
        KeyCode::Tab => {
            app.cycle_focus(false);
            return;
        }
        KeyCode::BackTab => {
            app.cycle_focus(true);
            return;
        }
        KeyCode::Esc => {
            app.focus = Focus::Operations;
            return;
        }
        KeyCode::Char('q') if app.focus != Focus::Input => {
            app.request_quit();
            return;
        }
        KeyCode::Char('?') if app.focus != Focus::Input => {
            app.show_help = true;
            return;
        }
        KeyCode::Char(':') if app.focus != Focus::Input => {
            app.start_command();
            return;
        }
        KeyCode::Char('[') if app.focus != Focus::Input => {
            app.switch_category(-1);
            return;
        }
        KeyCode::Char(']') if app.focus != Focus::Input => {
            app.switch_category(1);
            return;
        }
        KeyCode::Char(value @ '0'..='9') if app.focus != Focus::Input => {
            app.select_category(usize::from(value as u8 - b'0'));
            return;
        }
        _ => {}
    }

    match app.focus {
        Focus::Operations => handle_operation_key(app, key),
        Focus::Parameters => handle_parameter_key(app, key),
        Focus::Input => {
            app.input.handle_key(key, true, false);
        }
        Focus::Output => handle_output_key(app, key),
    }
}

fn handle_operation_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.move_tool(1),
        KeyCode::Char('k') | KeyCode::Up => app.move_tool(-1),
        KeyCode::Char('g') | KeyCode::Home => {
            app.tool_selections[app.category] = 0;
            app.load_selected_tool();
        }
        KeyCode::Char('G') | KeyCode::End => {
            app.tool_selections[app.category] = app.tool_indices().len().saturating_sub(1);
            app.load_selected_tool();
        }
        KeyCode::Char('h') | KeyCode::Left => app.switch_category(-1),
        KeyCode::Char('l') | KeyCode::Right => app.switch_category(1),
        KeyCode::Char('f') => app.toggle_selected_home_tool(),
        KeyCode::Delete if app.category() == Category::Home => app.toggle_selected_home_tool(),
        KeyCode::Char('R') if app.category() == Category::Home => app.reset_home(),
        KeyCode::Enter => app.activate_tool(),
        _ => {}
    }
}

fn handle_parameter_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => app.move_field(1),
        KeyCode::Char('k') | KeyCode::Up => app.move_field(-1),
        KeyCode::Char('h') | KeyCode::Left => app.adjust_selected_field(-1, false),
        KeyCode::Char('l') | KeyCode::Right => app.adjust_selected_field(1, false),
        KeyCode::PageDown => app.adjust_selected_field(1, true),
        KeyCode::PageUp => app.adjust_selected_field(-1, true),
        KeyCode::Char(' ') => app.toggle_selected_field(),
        KeyCode::Enter | KeyCode::Char('e') => app.edit_selected_field(),
        _ => {}
    }
}

fn handle_output_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('h') | KeyCode::Left => app.cycle_focus(true),
        KeyCode::Char('l') | KeyCode::Right => app.cycle_focus(false),
        KeyCode::Char('y') => app.copy_output(),
        KeyCode::Char('j') | KeyCode::Down => {
            app.output_scroll = app.output_scroll.saturating_add(1);
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.output_scroll = app.output_scroll.saturating_sub(1);
        }
        KeyCode::PageDown => app.output_scroll = app.output_scroll.saturating_add(10),
        KeyCode::PageUp => app.output_scroll = app.output_scroll.saturating_sub(10),
        KeyCode::Home | KeyCode::Char('g') => app.output_scroll = 0,
        KeyCode::End | KeyCode::Char('G') => {
            app.output_scroll = usize_to_u16(app.output.lines().count().saturating_sub(1));
        }
        _ => {}
    }
}

fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        frame.render_widget(
            Paragraph::new(format!(
                "vutils TUI needs at least {MIN_WIDTH}×{MIN_HEIGHT}\ncurrent terminal: {}×{}\n\nPress Ctrl-C to quit.",
                area.width, area.height
            ))
            .alignment(Alignment::Center)
            .block(Block::bordered().title(" Terminal too small ")),
            area,
        );
        return;
    }

    let [header, tabs, body, command, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(2),
        Constraint::Min(12),
        Constraint::Length(2),
        Constraint::Length(2),
    ])
    .areas(area);
    render_header(frame, app, header);
    render_tabs(frame, app, tabs);
    let [operations, workspace] =
        Layout::horizontal([Constraint::Length(OPERATIONS_WIDTH), Constraint::Min(40)]).areas(body);
    render_operations(frame, app, operations);
    render_workspace(frame, app, workspace);
    render_command(frame, app, command);
    render_footer(frame, app, footer);

    if app.show_help {
        render_help(frame, area);
    }
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let activity = if app.is_running() { "RUNNING" } else { "READY" };
    let activity_style = if app.is_running() {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " vutils ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  backend workbench  "),
            Span::styled(activity, activity_style),
        ]))
        .block(
            Block::new()
                .borders(Borders::BOTTOM)
                .border_style(Color::DarkGray),
        ),
        area,
    );
}

fn render_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let full_labels_width = 1 + Category::ALL
        .iter()
        .enumerate()
        .map(|(index, category)| format!("{} {} ", index, category.label()).chars().count())
        .sum::<usize>();
    let compact = usize::from(area.width) < full_labels_width;
    let compact_labels_width = 1 + Category::ALL
        .iter()
        .enumerate()
        .map(|(index, category)| {
            format!("{} {} ", index, category.tab_label(true, false))
                .chars()
                .count()
        })
        .sum::<usize>();
    let narrow = usize::from(area.width) < compact_labels_width;
    let mut spans = vec![Span::raw(" ")];
    for (index, category) in Category::ALL.iter().enumerate() {
        let label = format!("{} {}", index, category.tab_label(compact, narrow));
        if index == app.category {
            spans.push(Span::styled(
                label,
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(label, Style::default().fg(Color::Gray)));
        }
        spans.push(Span::raw(" "));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).block(
            Block::new()
                .borders(Borders::BOTTOM)
                .border_style(Color::DarkGray),
        ),
        area,
    );
}

fn render_operations(frame: &mut Frame, app: &App, area: Rect) {
    let title = if app.category() == Category::Home {
        " Home shortcuts ".into()
    } else {
        format!(" {} operations ", app.category().label())
    };
    let block = panel(title, app.focus == Focus::Operations);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [list_area, detail_area] =
        Layout::vertical([Constraint::Min(4), Constraint::Length(4)]).areas(inner);
    let indices = app.tool_indices();
    let items = indices.iter().map(|index| {
        ListItem::new(format!(
            "{} {}",
            if app.is_home_tool(*index) { "★" } else { " " },
            TOOLS[*index].name
        ))
    });
    let mut state = ListState::default().with_selected(Some(app.tool_selections[app.category]));
    frame.render_stateful_widget(
        List::new(items).highlight_symbol("▸ ").highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        list_area,
        &mut state,
    );
    frame.render_widget(
        Paragraph::new(format!(
            "{}\n\n{}",
            app.tool().description,
            if app.category() == Category::Home {
                "Enter open · f remove"
            } else {
                "Enter open · f Home"
            }
        ))
        .style(Style::default().fg(Color::Gray))
        .wrap(Wrap { trim: true })
        .block(
            Block::new()
                .borders(Borders::TOP)
                .border_style(Color::DarkGray),
        ),
        detail_area,
    );
}

fn render_workspace(frame: &mut Frame, app: &App, area: Rect) {
    let visible_count = app.visible_field_indices().len();
    let desired_parameters = if visible_count == 0 {
        3
    } else {
        usize_to_u16(visible_count).saturating_add(4)
    };
    let reserved = if app.tool().uses_input { 7 } else { 4 };
    let parameter_height = desired_parameters.min(area.height.saturating_sub(reserved).max(3));
    let [parameters, remaining] =
        Layout::vertical([Constraint::Length(parameter_height), Constraint::Min(3)]).areas(area);
    render_parameters(frame, app, parameters);
    if app.tool().uses_input {
        let [input, output] =
            Layout::vertical([Constraint::Percentage(47), Constraint::Percentage(53)])
                .areas(remaining);
        render_editor(
            frame,
            &app.input,
            input,
            " Input ",
            app.focus == Focus::Input,
            app.tool().sample,
        );
        render_output(frame, app, output);
    } else {
        render_output(frame, app, remaining);
    }
}

fn render_parameters(frame: &mut Frame, app: &App, area: Rect) {
    let block = panel(" Parameters ".into(), app.focus == Focus::Parameters);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let visible = app.visible_field_indices();
    if visible.is_empty() {
        frame.render_widget(
            Paragraph::new("No parameters · Ctrl-R runs immediately")
                .style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    }

    let [list_area, help_area] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(inner);
    let capacity = usize::from(list_area.height.max(1));
    let start = app
        .field_selection
        .saturating_sub(capacity.saturating_sub(1));
    let end = (start + capacity).min(visible.len());
    let selected_row = app.field_selection.saturating_sub(start);
    let value_width = usize::from(list_area.width).saturating_sub(FIELD_LABEL_WIDTH + 4);
    let items = visible[start..end].iter().map(|index| {
        let definition = &app.tool().fields[*index];
        let state = &app.form[*index];
        let selected = Some(*index) == app.selected_field_index();
        let editing = selected && app.editing_field;
        let value = if editing {
            state.editor().map_or_else(
                || state.display(definition),
                |editor| {
                    let (_, column) = editor.cursor_line_column();
                    let value = if definition.is_secret() {
                        "•".repeat(editor.value().chars().count())
                    } else {
                        editor.value()
                    };
                    visible_suffix(&value, column.saturating_sub(value_width.saturating_sub(1)))
                },
            )
        } else {
            state.display(definition)
        };
        ListItem::new(Line::from(vec![
            Span::styled(
                format!("{:<width$}", definition.label, width = FIELD_LABEL_WIDTH),
                Style::default().fg(Color::Gray),
            ),
            Span::raw(" "),
            Span::styled(
                value,
                if selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                },
            ),
        ]))
    });
    let mut state = ListState::default().with_selected(Some(selected_row));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_symbol("▸ ")
            .highlight_style(Style::default().add_modifier(Modifier::BOLD)),
        list_area,
        &mut state,
    );

    let selected_index = visible[app.field_selection];
    frame.render_widget(
        Paragraph::new(app.tool().fields[selected_index].help)
            .style(Style::default().fg(Color::DarkGray))
            .wrap(Wrap { trim: true }),
        help_area,
    );

    if let (true, Some(editor)) = (app.editing_field, app.form[selected_index].editor()) {
        let (_, column) = editor.cursor_line_column();
        let scroll = column.saturating_sub(value_width.saturating_sub(1));
        let visible_value = visible_suffix(&editor.value(), scroll);
        let cursor_column = visible_value.width();
        frame.set_cursor_position((
            list_area
                .x
                .saturating_add(2 + usize_to_u16(FIELD_LABEL_WIDTH + 1 + cursor_column))
                .min(list_area.right().saturating_sub(1)),
            list_area.y.saturating_add(usize_to_u16(selected_row)),
        ));
    }
}

fn render_editor(
    frame: &mut Frame,
    editor: &Editor,
    area: Rect,
    title: &str,
    focused: bool,
    placeholder: Option<&str>,
) {
    let block = panel(title.into(), focused);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    let (line, column) = editor.cursor_line_column();
    let vertical_scroll = line.saturating_sub(usize::from(inner.height.saturating_sub(1)));
    let horizontal_scroll = column.saturating_sub(usize::from(inner.width.saturating_sub(1)));
    if editor.is_empty() {
        frame.render_widget(
            Paragraph::new(
                placeholder
                    .filter(|value| !value.is_empty())
                    .unwrap_or("Type or paste input here"),
            )
            .style(Style::default().fg(Color::DarkGray))
            .wrap(Wrap { trim: false }),
            inner,
        );
    } else {
        frame.render_widget(
            Paragraph::new(editor.value()).scroll((
                usize_to_u16(vertical_scroll),
                usize_to_u16(horizontal_scroll),
            )),
            inner,
        );
    }
    if focused {
        frame.set_cursor_position((
            inner
                .x
                .saturating_add(usize_to_u16(column.saturating_sub(horizontal_scroll))),
            inner
                .y
                .saturating_add(usize_to_u16(line.saturating_sub(vertical_scroll))),
        ));
    }
}

fn render_output(frame: &mut Frame, app: &App, area: Rect) {
    let block = panel(" Output ".into(), app.focus == Focus::Output);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(app.output.as_str())
            .scroll((app.output_scroll, 0))
            .wrap(Wrap { trim: false }),
        inner,
    );
}

fn render_command(frame: &mut Frame, app: &App, area: Rect) {
    frame.render_widget(
        Paragraph::new(command_preview(app.tool(), &app.form))
            .style(Style::default().fg(Color::DarkGray))
            .block(
                Block::new()
                    .borders(Borders::TOP)
                    .border_style(Color::DarkGray),
            ),
        area,
    );
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    if let Some(editor) = &app.command {
        let (_, column) = editor.cursor_line_column();
        let capacity = usize::from(area.width.saturating_sub(2));
        let scroll = column.saturating_sub(capacity);
        let visible = visible_suffix(&editor.value(), scroll);
        let cursor = visible.width();
        frame.render_widget(
            Paragraph::new(vec![
                Line::styled(app.status.as_str(), Style::default().fg(Color::Gray)),
                Line::from(vec![Span::styled(":", key_style()), Span::raw(visible)]),
            ]),
            area,
        );
        frame.set_cursor_position((
            area.x
                .saturating_add(1 + usize_to_u16(cursor))
                .min(area.right().saturating_sub(1)),
            area.y.saturating_add(1),
        ));
        return;
    }

    let focus_hint = match app.focus {
        Focus::Operations if app.category() == Category::Home => {
            "↑↓ shortcut · ←→ tabs · f/Del remove · R reset"
        }
        Focus::Operations => "↑↓ operation · ←→ tabs · f Home · Enter configure",
        Focus::Parameters if app.editing_field => "type value · Enter accept · Esc return",
        Focus::Parameters => "↑↓ field · ←→ value · Space toggle · Enter edit",
        Focus::Input => "edit input · Tab output · Ctrl-R run",
        Focus::Output => "↑↓ scroll · y copy · Tab operations",
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(app.status.as_str(), Style::default().fg(Color::Gray)),
            Line::from(vec![
                Span::styled(" Ctrl-R ", key_style()),
                Span::raw("run  "),
                Span::styled(" [ ] / 0-7 ", key_style()),
                Span::raw("tabs  "),
                Span::styled(" ? ", key_style()),
                Span::raw("help  "),
                Span::styled(focus_hint, Style::default().fg(Color::DarkGray)),
            ]),
        ]),
        area,
    );
}

fn render_help(frame: &mut Frame, area: Rect) {
    let width = area.width.saturating_sub(6).min(82);
    let height = area.height.saturating_sub(2).min(20);
    let popup = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, popup);
    let help = [
        "Navigation",
        "  0 Home · 1-7 categories · [ ] previous/next tab",
        "  ↑↓/jk operation or field · ←→/hl tab or value",
        "  Enter open/edit · Space toggle Yes/No",
        "  Tab / Shift-Tab moves focus between panels",
        "  f add/remove Home · Delete remove · R reset",
        "  :q / :qa / :qall  close the TUI",
        "",
        "Execution",
        "  Ctrl-R / F5 run · y copy UTF-8 output",
        "  q / Ctrl-C quits when no command is running",
        "",
        "Preview shows exact CLI arguments · ? / Esc closes help.",
    ]
    .join("\n");
    frame.render_widget(
        Paragraph::new(help)
            .block(panel(" Help ".into(), true).padding(Padding::uniform(1)))
            .wrap(Wrap { trim: false }),
        popup,
    );
}

fn panel(title: String, focused: bool) -> Block<'static> {
    Block::new()
        .borders(Borders::ALL)
        .title(title)
        .border_style(if focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        })
}

fn key_style() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn visible_suffix(value: &str, columns_to_skip: usize) -> String {
    let mut skipped = 0;
    value
        .chars()
        .skip_while(|character| {
            if skipped >= columns_to_skip {
                return false;
            }
            skipped += character.width().unwrap_or(0);
            true
        })
        .collect()
}

fn usize_to_u16(value: usize) -> u16 {
    u16::try_from(value).unwrap_or(u16::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn app() -> App {
        App::from_config(None, None)
    }

    fn press(app: &mut App, code: KeyCode) {
        handle_key(app, KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn run_ex(command: &str) -> App {
        let mut app = app();
        press(&mut app, KeyCode::Char(':'));
        for character in command.chars() {
            press(&mut app, KeyCode::Char(character));
        }
        press(&mut app, KeyCode::Enter);
        app
    }

    #[test]
    fn switching_tabs_loads_the_first_contextual_form() {
        let mut app = app();
        app.select_category(1);
        assert_eq!(app.category(), Category::Random);
        assert_eq!(app.tool().name, "UUID");
        assert_eq!(app.visible_field_indices().len(), 3);
    }

    #[test]
    fn random_tab_shows_the_uuid_determinism_easter_egg() {
        let backend = TestBackend::new(100, MIN_HEIGHT);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = app();
        app.select_category(1);
        app.activate_tool();

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("1 Random"));
        assert!(rendered.contains("Plot twist: v3/v5 are deterministic"));
    }

    #[test]
    fn focus_cycle_skips_input_for_generators() {
        let mut app = app();
        app.select_category(1);
        app.activate_tool();
        assert_eq!(app.focus, Focus::Parameters);
        app.cycle_focus(false);
        assert_eq!(app.focus, Focus::Output);
    }

    #[test]
    fn category_navigation_wraps_in_both_directions() {
        let mut app = app();
        app.switch_category(-1);
        assert_eq!(app.category(), Category::Configuration);
        app.switch_category(1);
        assert_eq!(app.category(), Category::Home);
    }

    #[test]
    fn numeric_shortcuts_open_vruno_and_configuration() {
        let mut app = app();

        handle_key(&mut app, KeyEvent::from(KeyCode::Char('6')));

        assert_eq!(app.category(), Category::Vruno);
        assert_eq!(app.tool().name, "Configure");

        handle_key(&mut app, KeyEvent::from(KeyCode::Char('7')));

        assert_eq!(app.category(), Category::Configuration);
        assert_eq!(app.tool().name, "Configuration");
    }

    #[test]
    fn configuration_forms_use_effective_values() {
        let directory = tempfile::tempdir().unwrap();
        let mut config = UserConfig::load_from(directory.path().join("config.toml")).unwrap();
        config.set("sql.dialect", "mysql").unwrap();
        config.set("uuid.version", "v4").unwrap();
        config.set("uuid.format", "simple").unwrap();
        config.set("crypto.algorithm", "aes-256-gcm").unwrap();
        config
            .set("crypto.password-env", "BACKEND_PASSWORD")
            .unwrap();
        config.set("tui.home", "uuid,json.pretty").unwrap();
        config.set("vruno.collection", "collections/api").unwrap();
        config.set("vruno.openapi", "specs/openapi.yaml").unwrap();
        let mut app = App::from_config(Some(config), None);
        app.select_category(7);
        app.move_tool(1);

        for (name, expected) in [
            ("SQL dialect", "mysql"),
            ("UUID version", "v4"),
            ("UUID format", "simple"),
            ("Encryption algorithm", "aes-256-gcm"),
            ("Password environment", "BACKEND_PASSWORD"),
            ("Password file", "password.txt"),
            ("Home shortcuts", "uuid,json.pretty"),
            ("Vruno collection", "collections/api"),
            ("Vruno OpenAPI", "specs/openapi.yaml"),
        ] {
            app.move_tool(1);
            assert_eq!(app.tool().name, name);
            assert_eq!(app.form[0].value(&app.tool().fields[0]), expected);
        }
    }

    #[test]
    fn vruno_configuration_form_uses_persisted_paths() {
        let directory = tempfile::tempdir().unwrap();
        let mut config = UserConfig::load_from(directory.path().join("config.toml")).unwrap();
        config.set("vruno.collection", "collections/api").unwrap();
        config.set("vruno.openapi", "specs/openapi.yaml").unwrap();
        let mut app = App::from_config(Some(config), None);

        app.select_category(6);

        let values = app
            .form
            .iter()
            .zip(app.tool().fields)
            .map(|(state, field)| state.value(field))
            .collect::<Vec<_>>();
        assert_eq!(
            values,
            [
                directory
                    .path()
                    .join("collections/api")
                    .display()
                    .to_string(),
                directory
                    .path()
                    .join("specs/openapi.yaml")
                    .display()
                    .to_string(),
            ]
        );
    }

    #[test]
    fn encryption_form_exposes_the_configured_password_source() {
        let directory = tempfile::tempdir().unwrap();
        let mut config = UserConfig::load_from(directory.path().join("config.toml")).unwrap();
        config
            .set("crypto.password-file", "secrets/password.txt")
            .unwrap();
        config.set("crypto.algorithm", "aes-256-gcm").unwrap();
        let mut app = App::from_config(Some(config), None);
        app.select_category(5);

        let values = app
            .form
            .iter()
            .zip(app.tool().fields)
            .map(|(state, field)| (field.key, state.value(field)))
            .collect::<std::collections::HashMap<_, _>>();

        assert_eq!(app.tool().name, "Encrypt");
        assert_eq!(values["algorithm"], "aes-256-gcm");
        assert_eq!(values["password_source"], "file");
        assert_eq!(
            values["password_file"],
            directory
                .path()
                .join("secrets/password.txt")
                .display()
                .to_string()
        );
    }

    #[test]
    fn encryption_without_a_configured_source_requests_a_masked_password() {
        let directory = tempfile::tempdir().unwrap();
        let config = UserConfig::load_from(directory.path().join("config.toml")).unwrap();
        let mut app = App::from_config(Some(config), None);
        app.select_category(5);

        assert_eq!(app.form[1].value(&app.tool().fields[1]), "direct");
        assert_eq!(app.form[2].display(&app.tool().fields[2]), "(not set)");
        app.start_execution();
        assert_eq!(app.focus, Focus::Parameters);
        assert_eq!(app.status, "Password is required");
    }

    #[test]
    fn invalid_numeric_field_stays_in_context_instead_of_running() {
        let mut app = app();
        app.select_category(1);
        app.move_tool(1);
        app.activate_tool();
        app.form[0].editor_mut().unwrap().clear();

        app.start_execution();

        assert_eq!(app.focus, Focus::Parameters);
        assert_eq!(app.field_selection, 0);
        assert!(app.status.contains("between 4 and 4096"));
        assert!(!app.is_running());
    }

    #[test]
    fn lazyvim_minimum_float_size_keeps_all_tabs_and_workspace_visible() {
        let backend = TestBackend::new(MIN_WIDTH, MIN_HEIGHT);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = app();
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();

        assert!(rendered.contains("2 Format"));
        assert!(rendered.contains("3 Parse"));
        assert!(rendered.contains("6 Vruno"));
        assert!(rendered.contains("7 Config"));
        assert!(rendered.contains("Home shortcuts"));
        assert!(rendered.contains("Format JSON"));
        assert!(rendered.contains("Input"));
        assert!(rendered.contains("Output"));
        assert!(rendered.contains("←→ tabs"));
        assert!(rendered.contains(r#"{"name":"Volnei"#));
        assert!(app.input.is_empty());
        assert!(!rendered.contains("Terminal too small"));

        app.show_help = true;
        terminal.draw(|frame| render(frame, &app)).unwrap();
        let help = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(help.contains("? / Esc closes help."));
    }

    #[test]
    fn input_samples_are_placeholders_instead_of_submitted_content() {
        let mut app = app();
        assert!(app.input.is_empty());

        app.focus = Focus::Input;
        press(&mut app, KeyCode::Char('{'));

        assert_eq!(app.input.value(), "{");
    }

    #[test]
    fn changing_operation_or_tab_clears_unrelated_output() {
        let mut app = app();
        app.output = "result from Format JSON".into();
        app.clipboard_value = Some("result from Format JSON".into());
        app.output_scroll = 4;

        app.move_tool(1);

        assert_eq!(app.tool().name, "UUID");
        assert_eq!(app.output, empty_output(app.tool()));
        assert_eq!(app.output_scroll, 0);
        assert!(app.clipboard_value.is_none());

        app.output = "result from UUID".into();
        app.switch_category(1);

        assert_eq!(app.category(), Category::Random);
        assert_eq!(app.output, empty_output(app.tool()));
        assert!(!app.output.contains("result from UUID"));
    }

    #[test]
    fn background_completion_does_not_replace_the_selected_tools_output() {
        let mut app = app();
        let source = *app.tool();
        let (sender, receiver) = std::sync::mpsc::channel();
        app.run_state = RunState::Running {
            tool_id: tool_id(&source),
            tool_name: source.name,
            refresh_config: false,
            receiver,
        };
        app.select_category(1);
        let selected_output = app.output.clone();
        sender
            .send(Ok(Execution {
                status: Some(0),
                stdout: b"unrelated result".to_vec(),
                stderr: Vec::new(),
                elapsed: Duration::from_millis(5),
            }))
            .unwrap();

        assert!(app.poll_execution());
        assert_eq!(app.output, selected_output);
        assert!(!app.output.contains("unrelated result"));
        assert!(app.status.contains("completed in background"));
    }

    #[test]
    fn wide_terminal_uses_the_full_configuration_tab_label() {
        let backend = TestBackend::new(100, MIN_HEIGHT);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = app();

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("6 Vruno"));
        assert!(rendered.contains("7 Configuration"));
    }

    #[test]
    fn long_operation_lists_keep_the_selected_item_visible() {
        let backend = TestBackend::new(MIN_WIDTH, MIN_HEIGHT);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = app();
        app.select_category(7);
        press(&mut app, KeyCode::End);

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert_eq!(app.tool().name, "Manual page");
        assert!(rendered.contains("Manual page"));
    }

    #[test]
    fn password_field_stays_masked_while_editing() {
        let backend = TestBackend::new(100, MIN_HEIGHT);
        let mut terminal = Terminal::new(backend).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let config = UserConfig::load_from(directory.path().join("config.toml")).unwrap();
        let mut app = App::from_config(Some(config), None);
        app.select_category(5);
        app.focus = Focus::Parameters;
        app.field_selection = 2;
        app.editing_field = true;
        app.form[2]
            .editor_mut()
            .unwrap()
            .replace("never-render-this");

        terminal.draw(|frame| render(frame, &app)).unwrap();

        let rendered = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(!rendered.contains("never-render-this"));
        assert!(rendered.contains("••••"));
    }

    #[test]
    fn quit_shortcut_is_global_except_while_editing_input() {
        let mut app = app();
        app.focus = Focus::Input;
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
        );
        assert!(!app.should_quit);
        assert!(app.input.value().ends_with('q'));

        app.focus = Focus::Parameters;
        handle_key(
            &mut app,
            KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
        );
        assert!(app.should_quit);
    }

    #[test]
    fn vim_navigation_keys_work_across_navigable_panels() {
        let mut app = app();
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.tool().name, "UUID");
        press(&mut app, KeyCode::Char('k'));
        assert_eq!(app.tool().name, "Format JSON");
        press(&mut app, KeyCode::Char('l'));
        assert_eq!(app.category(), Category::Random);
        press(&mut app, KeyCode::Char('h'));
        assert_eq!(app.category(), Category::Home);

        app.focus = Focus::Output;
        press(&mut app, KeyCode::Char('h'));
        assert_eq!(app.focus, Focus::Input);
        app.focus = Focus::Output;
        press(&mut app, KeyCode::Char('l'));
        assert_eq!(app.focus, Focus::Operations);
    }

    #[test]
    fn vim_quit_commands_close_the_tui() {
        for command in ["q", "q!", "qa", "qa!", "qall", "qall!"] {
            assert!(run_ex(command).should_quit, ":{command} did not quit");
        }
    }

    #[test]
    fn unknown_vim_command_reports_an_error_without_quitting() {
        let app = run_ex("unknown");
        assert!(!app.should_quit);
        assert_eq!(app.status, "Unknown command: :unknown");
        assert!(app.command.is_none());
    }

    #[test]
    fn favorite_shortcut_is_persisted_and_added_to_home() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let config = UserConfig::load_from(path.clone()).unwrap();
        let mut app = App::from_config(Some(config), None);
        app.select_category(2);
        app.move_tool(3);
        assert_eq!(app.tool().name, "Minify");

        app.toggle_selected_home_tool();

        assert!(
            app.home_tools
                .iter()
                .any(|index| tool_id(&TOOLS[*index]) == "json.minify")
        );
        assert!(app.status.contains("Added Minify"));
        assert!(
            UserConfig::load_from(path)
                .unwrap()
                .tui_home()
                .contains(&"json.minify".to_owned())
        );
    }

    #[test]
    fn reset_home_restores_built_in_shortcuts() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.toml");
        let mut config = UserConfig::load_from(path.clone()).unwrap();
        config.set("tui.home", "uuid,sql.format").unwrap();
        config.save().unwrap();
        let mut app = App::from_config(Some(config), None);

        app.reset_home();

        assert_eq!(
            app.home_tools
                .iter()
                .map(|index| tool_id(&TOOLS[*index]))
                .collect::<Vec<_>>(),
            default_home_ids()
        );
        assert_eq!(
            UserConfig::load_from(path).unwrap().tui_home(),
            default_home_ids()
        );
        assert!(app.status.contains("restored"));
    }

    #[test]
    fn unknown_configured_shortcuts_do_not_break_home() {
        let directory = tempfile::tempdir().unwrap();
        let mut config = UserConfig::load_from(directory.path().join("config.toml")).unwrap();
        config.set("tui.home", "unknown.command,uuid").unwrap();

        let app = App::from_config(Some(config), None);

        assert_eq!(app.home_tools.len(), 1);
        assert_eq!(tool_id(app.tool()), "uuid");
        assert!(app.status.contains("unknown.command"));
    }
}
