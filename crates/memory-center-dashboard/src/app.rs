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
    /// v2.51：Tag 体系总览（19 类标签全景）
    Tags,
}

impl Tab {
    pub fn title(self) -> &'static str {
        match self {
            Tab::Overview => "概览",
            Tab::Memories => "记忆列表",
            Tab::Search => "检索演示",
            Tab::Eval => "评测对比",
            Tab::Tags => "Tag 体系",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Tab::Overview => Tab::Memories,
            Tab::Memories => Tab::Search,
            Tab::Search => Tab::Eval,
            Tab::Eval => Tab::Tags,
            Tab::Tags => Tab::Overview,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Tab::Overview => Tab::Tags,
            Tab::Memories => Tab::Overview,
            Tab::Search => Tab::Memories,
            Tab::Eval => Tab::Search,
            Tab::Tags => Tab::Eval,
        }
    }

    pub fn index(self) -> usize {
        match self {
            Tab::Overview => 0,
            Tab::Memories => 1,
            Tab::Search => 2,
            Tab::Eval => 3,
            Tab::Tags => 4,
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
    /// v2.51：记忆列表 Tag 过滤（None=全部，Some(tag)=只显示含该 tag 的记忆）
    pub tag_filter: Option<String>,

    // 检索 Tab
    pub search_input: String,
    pub search_input_focused: bool,
    pub search_results: Vec<SearchHit>,
    pub search_mode: String,

    // 评测 Tab
    pub eval_rows: Vec<EvalRow>,

    /// v2.51：Tag 体系 Tab 光标位置（用于 j/k 选择 tag 后回车跳转到记忆列表过滤）
    pub tags_selected: usize,
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
            tag_filter: None,
            search_input: String::new(),
            search_input_focused: false,
            search_results: Vec::new(),
            search_mode: String::new(),
            eval_rows: load_local_eval(&eval_dir),
            tags_selected: 0,
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
                    4 => Tab::Tags,
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
            Tab::Tags => self.handle_tags_key(key),
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
                // v2.51 修复：切换 session 时重置搜索状态
                self.search_input.clear();
                self.search_results.clear();
                self.search_mode.clear();
                self.tag_filter = None;
                self.selected_memory = 0;
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
    } else {
        // v2.51 修复：r 键刷新重新加载
        match key.code {
            KeyCode::Char('r') => {
                if !self.session_input.is_empty() {
                    self.loading = true;
                    self.status_msg = Some("刷新中...".to_string());
                    // 触发主循环重新加载：通过清空 last_session 缓存
                    // main.rs 会在下次循环检测到 loading=true 时重载
                }
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
                    if !self.filtered_summaries().is_empty() {
                        let len = self.filtered_summaries().len();
                        self.selected_memory = (self.selected_memory + 1) % len;
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    if !self.filtered_summaries().is_empty() {
                        let len = self.filtered_summaries().len();
                        self.selected_memory = if self.selected_memory == 0 {
                            len - 1
                        } else {
                            self.selected_memory - 1
                        };
                    }
                }
                KeyCode::Enter => {
                    self.show_detail = true;
                }
                // v2.51：按 t 清除 Tag 过滤
                KeyCode::Char('t') => {
                    if self.tag_filter.is_some() {
                        self.tag_filter = None;
                        self.status_msg = Some("已清除 Tag 过滤".to_string());
                        self.selected_memory = 0;
                    } else {
                        // 跳转到 Tag 体系 Tab 选择
                        self.tab = Tab::Tags;
                        self.status_msg = Some("请选择一个 Tag 进行过滤".to_string());
                    }
                }
                _ => {}
            }
        }
    }

    /// v2.51：Tag 体系 Tab 键盘事件
    ///
    /// - j/↓：下一个 Tag
    /// - k/↑：上一个 Tag
    /// - Enter：选定当前 Tag，自动跳转到记忆列表并应用过滤
    /// - c：清除过滤（若已设置）
    fn handle_tags_key(&mut self, key: KeyEvent) {
        let tag_count = self.tag_stats().len();
        if tag_count == 0 {
            return;
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.tags_selected = (self.tags_selected + 1) % tag_count;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.tags_selected = if self.tags_selected == 0 {
                    tag_count - 1
                } else {
                    self.tags_selected - 1
                };
            }
            KeyCode::Enter => {
                // 选定当前 Tag，跳转记忆列表
                if let Some((tag, _)) = self.tag_stats().get(self.tags_selected).cloned() {
                    self.tag_filter = Some(tag.clone());
                    self.tab = Tab::Memories;
                    self.selected_memory = 0;
                    self.status_msg = Some(format!("已过滤 Tag: {}", tag));
                }
            }
            KeyCode::Char('c') => {
                self.tag_filter = None;
                self.status_msg = Some("已清除 Tag 过滤".to_string());
            }
            _ => {}
        }
    }

    /// v2.51：计算 Tag 分布统计
    ///
    /// 返回按命中次数降序排列的 (tag_name, count) 列表
    pub fn tag_stats(&self) -> Vec<(String, usize)> {
        let mut counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for s in &self.summaries {
            for tag in &s.tags {
                *counts.entry(tag.clone()).or_insert(0) += 1;
            }
        }
        let mut stats: Vec<(String, usize)> = counts.into_iter().collect();
        // 按次数降序，次数相同按 tag 名升序
        stats.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        stats
    }

    /// v2.51：返回过滤后的 summaries 视图
    ///
    /// - tag_filter = None：返回全部
    /// - tag_filter = Some(t)：只返回 tags 含 t 的记忆
    pub fn filtered_summaries(&self) -> Vec<&SummaryItem> {
        match &self.tag_filter {
            None => self.summaries.iter().collect(),
            Some(tag) => self
                .summaries
                .iter()
                .filter(|s| s.tags.iter().any(|t| t == tag))
                .collect(),
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
