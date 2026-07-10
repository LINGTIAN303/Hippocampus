//! 应用状态 + 事件处理

use crate::client::{McClient, MemoryFile, SearchHit, SummaryItem};
use crate::eval_data::{load_local_eval, EvalRow};
use crossterm::event::{KeyCode, KeyEvent};

/// Tab 页枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Memories,
    Search,
    Eval,
}

impl Tab {
    pub fn title(self) -> &'static str {
        match self {
            Tab::Overview => "概览",
            Tab::Memories => "记忆列表",
            Tab::Search => "检索演示",
            Tab::Eval => "评测对比",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Tab::Overview => Tab::Memories,
            Tab::Memories => Tab::Search,
            Tab::Search => Tab::Eval,
            Tab::Eval => Tab::Overview,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Tab::Overview => Tab::Eval,
            Tab::Memories => Tab::Overview,
            Tab::Search => Tab::Memories,
            Tab::Eval => Tab::Search,
        }
    }

    pub fn index(self) -> usize {
        match self {
            Tab::Overview => 0,
            Tab::Memories => 1,
            Tab::Search => 2,
            Tab::Eval => 3,
        }
    }
}

/// 应用状态
pub struct App {
    pub tab: Tab,
    pub should_quit: bool,

    // session_id 输入
    pub session_input: String,
    pub session_input_focused: bool,

    // 连接配置
    pub base_url: String,
    pub api_key: Option<String>,

    // 概览 Tab 数据
    pub summaries: Vec<SummaryItem>,
    pub loading: bool,
    pub error_msg: Option<String>,
    pub status_msg: Option<String>,

    // 记忆列表 Tab
    pub selected_memory: usize,
    pub memory_detail: Option<MemoryFile>,
    pub show_detail: bool,

    // 检索 Tab
    pub search_input: String,
    pub search_input_focused: bool,
    pub search_results: Vec<SearchHit>,
    pub search_mode: String,

    // 评测 Tab
    pub eval_rows: Vec<EvalRow>,
}

impl App {
    pub fn new(base_url: String, api_key: Option<String>, eval_dir: String) -> Self {
        Self {
            tab: Tab::Overview,
            should_quit: false,
            session_input: String::new(),
            session_input_focused: true,
            base_url,
            api_key,
            summaries: Vec::new(),
            loading: false,
            error_msg: None,
            status_msg: None,
            selected_memory: 0,
            memory_detail: None,
            show_detail: false,
            search_input: String::new(),
            search_input_focused: false,
            search_results: Vec::new(),
            search_mode: String::new(),
            eval_rows: load_local_eval(&eval_dir),
        }
    }

    pub fn client(&self) -> McClient {
        McClient::new(self.base_url.clone(), self.api_key.clone())
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        // 全局键
        match key.code {
            KeyCode::Char('q') if !self.is_input_focused() => {
                self.should_quit = true;
                return;
            }
            KeyCode::Tab => {
                if !self.is_input_focused() {
                    self.tab = self.tab.next();
                    return;
                }
            }
            KeyCode::BackTab => {
                if !self.is_input_focused() {
                    self.tab = self.tab.prev();
                    return;
                }
            }
            KeyCode::Char(c) if c.is_ascii_digit() && !self.is_input_focused() => {
                let idx = c.to_digit(10).unwrap_or(0) as usize;
                self.tab = match idx {
                    0 => Tab::Overview,
                    1 => Tab::Memories,
                    2 => Tab::Search,
                    3 => Tab::Eval,
                    _ => self.tab,
                };
                return;
            }
            _ => {}
        }

        // Tab 特定键处理
        match self.tab {
            Tab::Overview => self.handle_overview_key(key),
            Tab::Memories => self.handle_memories_key(key),
            Tab::Search => self.handle_search_key(key),
            Tab::Eval => {}
        }
    }

    fn is_input_focused(&self) -> bool {
        self.session_input_focused || self.search_input_focused
    }

    fn handle_overview_key(&mut self, key: KeyEvent) {
        if self.session_input_focused {
            match key.code {
                KeyCode::Enter => {
                    self.session_input_focused = false;
                    self.status_msg = Some(format!("已选择 session: {}", self.session_input));
                }
                KeyCode::Esc => {
                    self.session_input_focused = false;
                }
                KeyCode::Backspace => {
                    self.session_input.pop();
                }
                KeyCode::Char(c) => {
                    self.session_input.push(c);
                }
                _ => {}
            }
        }
    }

    fn handle_memories_key(&mut self, key: KeyEvent) {
        if self.show_detail {
            match key.code {
                KeyCode::Esc => {
                    self.show_detail = false;
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    // 滚动详情（简化处理）
                }
                KeyCode::Char('k') | KeyCode::Up => {}
                _ => {}
            }
        } else {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    if !self.summaries.is_empty() {
                        self.selected_memory =
                            (self.selected_memory + 1) % self.summaries.len();
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    if !self.summaries.is_empty() {
                        self.selected_memory = if self.selected_memory == 0 {
                            self.summaries.len() - 1
                        } else {
                            self.selected_memory - 1
                        };
                    }
                }
                KeyCode::Enter => {
                    self.show_detail = true;
                }
                _ => {}
            }
        }
    }

    fn handle_search_key(&mut self, key: KeyEvent) {
        if self.search_input_focused {
            match key.code {
                KeyCode::Enter => {
                    self.search_input_focused = false;
                }
                KeyCode::Esc => {
                    self.search_input_focused = false;
                }
                KeyCode::Backspace => {
                    self.search_input.pop();
                }
                KeyCode::Char(c) => {
                    self.search_input.push(c);
                }
                _ => {}
            }
        } else {
            match key.code {
                KeyCode::Char('s') | KeyCode::Char('/') => {
                    self.search_input_focused = true;
                }
                _ => {}
            }
        }
    }

    /// 加载 summaries（异步任务调用后更新）
    pub fn set_summaries(&mut self, summaries: Vec<SummaryItem>) {
        self.summaries = summaries;
        self.loading = false;
        self.error_msg = None;
        self.status_msg = Some(format!("已加载 {} 条记忆", self.summaries.len()));
    }

    pub fn set_error(&mut self, msg: String) {
        self.error_msg = Some(msg);
        self.loading = false;
    }
}
