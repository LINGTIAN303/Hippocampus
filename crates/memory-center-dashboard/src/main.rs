//! mc-dashboard - MemoryCenter 交互式 TUI 工具
//!
//! 用法:
//!   mc-dashboard                              # 默认连接 127.0.0.1:8765
//!   mc-dashboard --url http://162.211.183.236:8088/api/v1 --key <API_KEY>
//!   mc-dashboard --eval-dir ./eval/results   # 指定评测数据目录

use clap::Parser;
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use memory_center_dashboard::app::App;
use memory_center_dashboard::client::McClient;
use memory_center_dashboard::ui;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, stdout};

/// MemoryCenter Dashboard CLI 参数
#[derive(Parser, Debug)]
#[command(name = "mc-dashboard", about = "MemoryCenter 交互式 TUI 工具")]
struct Args {
    /// MemoryCenter REST API 地址
    #[arg(long, env = "MEMORY_CENTER_BASE_URL", default_value = "http://127.0.0.1:8765")]
    url: String,

    /// API Key (Bearer token)
    #[arg(long, env = "MEMORY_CENTER_API_KEY")]
    key: Option<String>,

    /// 评测数据目录 (包含 longmemeval_summary_v2.36.json)
    #[arg(long, env = "MC_EVAL_DIR", default_value = "eval/results")]
    eval_dir: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // 初始化 App
    let mut app = App::new(args.url.clone(), args.key.clone(), args.eval_dir.clone());

    // 健康检查
    {
        let client = McClient::new(args.url.clone(), args.key.clone());
        match client.health_check().await {
            Ok(true) => {
                app.status_msg = Some("已连接 MemoryCenter".to_string());
            }
            Ok(false) | Err(_) => {
                app.error_msg = Some(format!("无法连接: {}", args.url));
            }
        }
    }

    // 初始化终端
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;

    // 主循环
    let result = run(&mut terminal, &mut app).await;

    // 恢复终端
    disable_raw_mode()?;
    execute!(stdout(), LeaveAlternateScreen)?;

    result
}

async fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut last_session = String::new();

    loop {
        terminal.draw(|f| ui::render(f, app))?;

        // 异步检查 session_id 变化, 自动加载 summaries
        let current_session = app.session_input.clone();
        if !app.session_input_focused && current_session != last_session && !current_session.is_empty() {
            last_session = current_session.clone();
            app.loading = true;
            let client = app.client();
            match client.get_summaries(&current_session).await {
                Ok(summaries) => app.set_summaries(summaries),
                Err(e) => app.set_error(e),
            }
            continue;
        }

        // 检查搜索请求
        if !app.search_input_focused && !app.search_input.is_empty() && app.search_results.is_empty() && !app.session_input.is_empty() {
            let client = app.client();
            match client.search(&app.session_input, &app.search_input, 10).await {
                Ok(resp) => {
                    app.search_results = resp.results;
                    app.search_mode = resp.mode;
                    app.status_msg = Some(format!("检索完成: {} 条结果", app.search_results.len()));
                }
                Err(e) => {
                    app.error_msg = Some(format!("检索失败: {e}"));
                }
            }
            continue;
        }

        // 等待按键事件 (100ms 超时, 让异步任务有机会运行)
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                app.handle_key(key);
            }
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}
