//! UI 渲染 - 4 个 Tab 页

use crate::app::{App, Tab};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, List, ListItem, Paragraph, Row, Table, Tabs, Wrap};
use ratatui::Frame;

/// 主渲染入口
pub fn render(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // 标题栏
            Constraint::Length(3), // Tab 栏
            Constraint::Min(1),    // 内容区
            Constraint::Length(3), // 状态栏
        ])
        .split(f.area());

    render_header(f, app, chunks[0]);
    render_tabs(f, app, chunks[1]);
    render_content(f, app, chunks[2]);
    render_status_bar(f, app, chunks[3]);
}

fn render_header(f: &mut Frame, _app: &App, area: Rect) {
    let title = Paragraph::new("MemoryCenter Dashboard")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, area);
}

fn render_tabs(f: &mut Frame, app: &App, area: Rect) {
    let titles = vec!["概览", "记忆列表", "检索演示", "评测对比"];
    let tabs = Tabs::new(titles.iter().map(|t| Span::raw(*t)).collect::<Vec<_>>())
        .block(Block::default().borders(Borders::ALL).title("Tab"))
        .select(app.tab.index())
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    f.render_widget(tabs, area);
}

fn render_content(f: &mut Frame, app: &mut App, area: Rect) {
    match app.tab {
        Tab::Overview => render_overview(f, app, area),
        Tab::Memories => render_memories(f, app, area),
        Tab::Search => render_search(f, app, area),
        Tab::Eval => render_eval(f, app, area),
    }
}

fn render_overview(f: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    // session_id 输入框
    let input_text = if app.session_input_focused {
        format!("> {}_", app.session_input)
    } else {
        format!("session: {}", app.session_input)
    };
    let input = Paragraph::new(input_text)
        .style(if app.session_input_focused {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::White)
        })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("输入 session_id (回车确认)"),
        );
    f.render_widget(input, chunks[0]);

    // 概览信息
    let mut lines = Vec::new();

    if app.loading {
        lines.push(Line::from(vec![Span::styled(
            "加载中...",
            Style::default().fg(Color::Yellow),
        )]));
    }

    if let Some(err) = &app.error_msg {
        lines.push(Line::from(vec![Span::styled(
            format!("错误: {err}"),
            Style::default().fg(Color::Red),
        )]));
    }

    if !app.summaries.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("连接: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&app.base_url, Style::default().fg(Color::Blue)),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("记忆总数: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                app.summaries.len().to_string(),
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
        ]));

        // 按 period 统计
        let mut period_count: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for s in &app.summaries {
            *period_count.entry(&s.period).or_insert(0) += 1;
        }
        lines.push(Line::from(""));
        lines.push(Line::from("周期分布:"));
        for (period, count) in period_count.iter() {
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    format!("{period}: "),
                    Style::default().fg(Color::Magenta),
                ),
                Span::styled(count.to_string(), Style::default().fg(Color::Green)),
            ]));
        }

        // 最近记忆
        lines.push(Line::from(""));
        lines.push(Line::from("最近记忆:"));
        for s in app.summaries.iter().take(5) {
            let title = s.summary_title.chars().take(40).collect::<String>();
            lines.push(Line::from(vec![
                Span::styled("  - ", Style::default().fg(Color::DarkGray)),
                Span::styled(&s.archived_at, Style::default().fg(Color::Blue)),
                Span::styled(" | ", Style::default().fg(Color::DarkGray)),
                Span::styled(title, Style::default().fg(Color::White)),
            ]));
        }
    } else if !app.session_input.is_empty() && !app.session_input_focused {
        lines.push(Line::from(vec![Span::styled(
            "按 'r' 刷新加载记忆列表",
            Style::default().fg(Color::Yellow),
        )]));
    } else {
        lines.push(Line::from(vec![Span::styled(
            "请输入 session_id 开始浏览记忆库",
            Style::default().fg(Color::DarkGray),
        )]));
    }

    let content = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title("记忆库概览"),
    );
    f.render_widget(content, chunks[1]);
}

fn render_memories(f: &mut Frame, app: &mut App, area: Rect) {
    if app.show_detail {
        render_memory_detail(f, app, area);
        return;
    }

    if app.summaries.is_empty() {
        let msg = Paragraph::new("无记忆数据\n请先在「概览」Tab 输入 session_id 并加载")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("记忆列表"));
        f.render_widget(msg, area);
        return;
    }

    let items: Vec<ListItem> = app
        .summaries
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let title = s.summary_title.chars().take(50).collect::<String>();
            let tags = s.tags.join(",");
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(
                        format!("[{}] ", i),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        s.archived_at.chars().take(19).collect::<String>(),
                        Style::default().fg(Color::Blue),
                    ),
                    Span::raw(" "),
                    Span::styled(&s.period, Style::default().fg(Color::Magenta)),
                ]),
                Line::from(vec![
                    Span::styled("    ", Style::default()),
                    Span::styled(title, Style::default().fg(Color::White)),
                ]),
                Line::from(vec![
                    Span::styled("    tags: ", Style::default().fg(Color::DarkGray)),
                    Span::styled(tags, Style::default().fg(Color::Yellow)),
                ]),
            ])
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("记忆列表 (j/k 选择, Enter 查看详情)"),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, area, &mut ratatui::widgets::ListState::default());
}

fn render_memory_detail(f: &mut Frame, app: &App, area: Rect) {
    let mut lines = Vec::new();

    lines.push(Line::from(vec![Span::styled(
        "详情视图 (Esc 返回)",
        Style::default().fg(Color::Yellow),
    )]));

    if let Some(idx) = app.summaries.get(app.selected_memory) {
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Hook ID: ", Style::default().fg(Color::Cyan)),
            Span::styled(&idx.hook_id, Style::default().fg(Color::White)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("归档时间: ", Style::default().fg(Color::Cyan)),
            Span::styled(&idx.archived_at, Style::default().fg(Color::White)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("周期: ", Style::default().fg(Color::Cyan)),
            Span::styled(&idx.period, Style::default().fg(Color::White)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("标签: ", Style::default().fg(Color::Cyan)),
            Span::styled(idx.tags.join(", "), Style::default().fg(Color::Yellow)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("标题: ", Style::default().fg(Color::Cyan)),
            Span::styled(&idx.summary_title, Style::default().fg(Color::White)),
        ]));
        if let Some(abst) = &idx.abstract_text {
            lines.push(Line::from(""));
            lines.push(Line::from("摘要:"));
            lines.push(Line::from(Span::styled(
                abst.chars().take(200).collect::<String>(),
                Style::default().fg(Color::Green),
            )));
        }
    }

    let content = Paragraph::new(lines)
        .wrap(Wrap { trim: true })
        .block(Block::default().borders(Borders::ALL).title("记忆详情"));
    f.render_widget(content, area);
}

fn render_search(f: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    // 搜索输入框
    let input_text = if app.search_input_focused {
        format!("> {}_", app.search_input)
    } else {
        format!("查询: {}", app.search_input)
    };
    let input = Paragraph::new(input_text)
        .style(if app.search_input_focused {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::White)
        })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("检索查询 (s 或 / 开始输入, Enter 搜索)"),
        );
    f.render_widget(input, chunks[0]);

    // 搜索结果
    let mut lines = Vec::new();

    if !app.search_mode.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("检索模式: ", Style::default().fg(Color::Cyan)),
            Span::styled(&app.search_mode, Style::default().fg(Color::Yellow)),
        ]));
        lines.push(Line::from(""));
    }

    if app.search_results.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "输入查询关键词进行语义检索",
            Style::default().fg(Color::DarkGray),
        )]));
    } else {
        lines.push(Line::from(format!(
            "检索结果 ({} 条):",
            app.search_results.len()
        )));
        lines.push(Line::from(""));
        for (i, hit) in app.search_results.iter().enumerate() {
            lines.push(Line::from(vec![
                Span::styled(format!("[{}] ", i), Style::default().fg(Color::DarkGray)),
                Span::styled("score=", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{:.4}", hit.score),
                    Style::default().fg(Color::Green),
                ),
                Span::raw("  "),
                Span::styled(&hit.hook_id, Style::default().fg(Color::Blue)),
            ]));
            if let Some(snippet) = &hit.snippet {
                let s: String = snippet.chars().take(80).collect();
                lines.push(Line::from(vec![
                    Span::styled("    ", Style::default()),
                    Span::styled(s, Style::default().fg(Color::DarkGray)),
                ]));
            }
            lines.push(Line::from(""));
        }
    }

    let content = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title("检索结果"),
    );
    f.render_widget(content, chunks[1]);
}

fn render_eval(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Min(1)])
        .split(area);

    // 评测对比表
    let header_cells = ["评测", "模型", "Baseline", "MemoryCenter", "提升%", "评分方式"];
    let header = Row::new(
        header_cells
            .iter()
            .map(|h| Cell::from(*h).style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
    )
    .height(1)
    .bottom_margin(0);

    let rows: Vec<Row> = app
        .eval_rows
        .iter()
        .map(|r| {
            let improvement_str = if r.improvement >= 999.0 {
                "N/A".to_string()
            } else {
                format!("{:.1}%", r.improvement)
            };
            let mc_str = if r.memory_center >= 1.0 && r.dataset.contains("R@5") {
                "100%".to_string()
            } else if r.dataset.contains("速度") {
                format!("{:.1}s", r.memory_center)
            } else {
                format!("{:.4}", r.memory_center)
            };
            let base_str = if r.dataset.contains("速度") {
                format!("{:.1}s", r.baseline)
            } else {
                format!("{:.4}", r.baseline)
            };
            let imp_color = if r.improvement > 0.0 {
                Color::Green
            } else if r.improvement < 0.0 {
                Color::Red
            } else {
                Color::DarkGray
            };
            Row::new(vec![
                Cell::from(r.dataset.as_str()),
                Cell::from(r.model.as_str()),
                Cell::from(base_str),
                Cell::from(mc_str).style(Style::default().fg(Color::Green)),
                Cell::from(improvement_str).style(Style::default().fg(imp_color)),
                Cell::from(r.judge.as_str()),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(18),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(14),
            Constraint::Length(10),
            Constraint::Length(18),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title("评测对比 (V2.35 + V2.36)"),
    );
    f.render_widget(table, chunks[0]);

    // 结论文字
    let conclusion = vec![
        Line::from(vec![Span::styled(
            "核心结论:",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("1. ", Style::default().fg(Color::Cyan)),
            Span::styled(
                "纯算法评分下 MemoryCenter 优势显著: LoCoMo F1 +41.4%, R@5=100%",
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::from(vec![
            Span::styled("2. ", Style::default().fg(Color::Cyan)),
            Span::styled(
                "LLM-as-Judge 评分因 judge 宽松度被抹平 (V2.3/V2.36 均持平)",
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(vec![
            Span::styled("3. ", Style::default().fg(Color::Cyan)),
            Span::styled(
                "上下文压缩带来 31% 速度提升 (87.6s vs 127.1s)",
                Style::default().fg(Color::Green),
            ),
        ]),
        Line::from(vec![
            Span::styled("4. ", Style::default().fg(Color::Cyan)),
            Span::styled(
                "记忆检索能力是客观可验证的, 非依赖主观评分",
                Style::default().fg(Color::White),
            ),
        ]),
    ];

    let content = Paragraph::new(conclusion).block(
        Block::default()
            .borders(Borders::ALL)
            .title("分析结论"),
    );
    f.render_widget(content, chunks[1]);
}

fn render_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![
        Span::styled(" Tab=切换页  q=退出", Style::default().fg(Color::DarkGray)),
        Span::raw(" | "),
        Span::styled(format!("URL: {} ", app.base_url), Style::default().fg(Color::Blue)),
    ];

    if let Some(msg) = &app.status_msg {
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(msg, Style::default().fg(Color::Green)));
    }
    if let Some(err) = &app.error_msg {
        spans.push(Span::raw(" | "));
        spans.push(Span::styled(err, Style::default().fg(Color::Red)));
    }

    let bar = Paragraph::new(Line::from(spans)).block(Block::default().borders(Borders::ALL));
    f.render_widget(bar, area);
}
