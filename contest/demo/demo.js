/* ============================================================
   MemoryCenter 演示动画 · 核心逻辑
   ============================================================ */

// ============================================================
// 场景数据定义（14 个场景）
// ============================================================
const SCENES = [
  // 第一幕：接入
  { id: 1,  act: 1, actName: "接入", name: "安装 MemoryCenter",      capability: "Agentic 界面",      duration: 9000 },
  { id: 2,  act: 1, actName: "接入", name: "MCP 工具接入",           capability: "MCP 工具",          duration: 6000 },
  { id: 3,  act: 1, actName: "接入", name: "准备开始开发",           capability: "Agentic 界面",      duration: 2000 },

  // 第二幕：归档
  { id: 4,  act: 2, actName: "归档", name: "启动记忆辅助",           capability: "配置 + 可视化",      duration: 7000 },
  { id: 5,  act: 2, actName: "归档", name: "实时归档可视化",         capability: "完整归档 + 可视化",  duration: 8000 },
  { id: 6,  act: 2, actName: "归档", name: "进入生产阶段",           capability: "Agentic 界面",      duration: 3000 },

  // 第三幕：召回
  { id: 7,  act: 3, actName: "召回", name: "时间跳转：第三天",        capability: "跨会话",            duration: 4000 },
  { id: 8,  act: 3, actName: "召回", name: "跨会话记忆检索",         capability: "检索 + 可视化",     duration: 12000 },
  { id: 9,  act: 3, actName: "召回", name: "周期合并机制",           capability: "周期合并 + 可视化", duration: 8000 },

  // 第四幕：闭环
  { id: 10, act: 4, actName: "闭环", name: "跨 Agent 记忆迁移",     capability: "跨 Agent + MCP",    duration: 14000 },
  { id: 11, act: 4, actName: "闭环", name: "记忆标签体系",           capability: "17 类标签 + 跨 Agent", duration: 8500 },
  { id: 12, act: 4, actName: "闭环", name: "阈值压缩演示",           capability: "阈值压缩",          duration: 8000 },
  { id: 13, act: 4, actName: "闭环", name: "开发者能力",             capability: "多语言绑定 + MCP 工具", duration: 13000 },
  { id: 14, act: 4, actName: "闭环", name: "收尾",                  capability: "可视化",            duration: 5000 },
];

// ============================================================
// 全局状态
// ============================================================
const state = {
  currentIndex: 0,
  isPlaying: false,
  speed: 1,
  timer: null,
  sceneTimers: [],   // 场景内部动画的定时器（切换场景时清空）
};

// ============================================================
// DOM 引用
// ============================================================
const $ = (sel) => document.querySelector(sel);
const stage = $("#scene-content");
const captionText = $("#caption-text");
const sceneCurrent = $("#scene-current");
const sceneTotal = $("#scene-total");
const sceneNameEl = $("#scene-name");
const progressFill = $("#progress-fill");
const sceneProgressBar = $("#scene-progress-bar");
const actLabel = $("#act-label");
const btnPrev = $("#btn-prev");
const btnNext = $("#btn-next");
const btnPlay = $("#btn-play");
const btnRestart = $("#btn-restart");
const btnSpeed = $("#btn-speed");
const speedLabel = $("#speed-label");
const iconPlay = $("#icon-play");
const iconPause = $("#icon-pause");
const infoSceneId = $("#info-scene-id");
const infoAct = $("#info-act");
const infoCapability = $("#info-capability");
const infoDuration = $("#info-duration");

// ============================================================
// 初始化
// ============================================================
function init() {
  sceneTotal.textContent = SCENES.length;
  bindControls();
  bindKeyboard();
  renderScene(0);
}

// ============================================================
// 渲染场景
// ============================================================
function renderScene(index) {
  clearSceneTimers();
  const scene = SCENES[index];
  if (!scene) return;

  state.currentIndex = index;

  // 淡出旧内容
  stage.classList.add("fade-out");
  setTimeout(() => {
    stage.classList.remove("fade-out");
    // 渲染新内容
    const renderer = SCENE_RENDERERS[scene.id];
    if (renderer) {
      renderer(stage, scene);
    } else {
      stage.innerHTML = `<div class="loading-hint">场景 ${scene.id} 渲染器待实现</div>`;
    }
    stage.classList.add("fade-in");
    setTimeout(() => stage.classList.remove("fade-in"), 400);
  }, 250);

  // 更新 UI
  updateUI(scene);
}

// ============================================================
// 更新 UI
// ============================================================
function updateUI(scene) {
  // 旁白
  const caption = SCENE_CAPTIONS[scene.id] || scene.name;
  captionText.style.opacity = "0";
  setTimeout(() => {
    captionText.textContent = caption;
    captionText.style.opacity = "1";
  }, 200);

  // 场景计数
  sceneCurrent.textContent = scene.id;
  sceneNameEl.textContent = scene.name;

  // 进度条
  const progress = ((scene.id) / SCENES.length) * 100;
  progressFill.style.width = progress + "%";
  sceneProgressBar.style.width = progress + "%";

  // 幕标签
  actLabel.textContent = `第${["一","二","三","四"][scene.act - 1]}幕 · ${scene.actName}`;

  // 信息面板
  infoSceneId.textContent = `${scene.id} / ${SCENES.length}`;
  infoAct.textContent = `第${["一","二","三","四"][scene.act - 1]}幕 ${scene.actName}`;
  infoCapability.textContent = scene.capability;
  infoDuration.textContent = (scene.duration / 1000).toFixed(1) + "s";
}

// ============================================================
// 播放控制
// ============================================================
function play() {
  if (state.currentIndex >= SCENES.length - 1) {
    state.currentIndex = 0;
    renderScene(0);
  }
  state.isPlaying = true;
  togglePlayIcon();
  const scene = SCENES[state.currentIndex];
  state.timer = setTimeout(() => {
    if (state.currentIndex < SCENES.length - 1) {
      renderScene(state.currentIndex + 1);
      play();
    } else {
      pause();
    }
  }, scene.duration / state.speed);
}

function pause() {
  state.isPlaying = false;
  clearTimeout(state.timer);
  togglePlayIcon();
}

function togglePlayIcon() {
  iconPlay.style.display = state.isPlaying ? "none" : "block";
  iconPause.style.display = state.isPlaying ? "block" : "none";
}

function jumpTo(index) {
  const wasPlaying = state.isPlaying;
  pause();
  renderScene(index);
  if (wasPlaying) play();
}

function next() {
  if (state.currentIndex < SCENES.length - 1) {
    const wasPlaying = state.isPlaying;
    pause();
    renderScene(state.currentIndex + 1);
    if (wasPlaying) play();
  }
}

function prev() {
  if (state.currentIndex > 0) {
    const wasPlaying = state.isPlaying;
    pause();
    renderScene(state.currentIndex - 1);
    if (wasPlaying) play();
  }
}

function restart() {
  pause();
  renderScene(0);
}

function toggleSpeed() {
  const speeds = [1, 1.5, 2, 0.5];
  const i = speeds.indexOf(state.speed);
  state.speed = speeds[(i + 1) % speeds.length];
  speedLabel.textContent = state.speed + "x";
  if (state.isPlaying) {
    pause();
    play();
  }
}

// ============================================================
// 场景内部定时器管理
// ============================================================
function addSceneTimer(timer) {
  state.sceneTimers.push(timer);
}

function clearSceneTimers() {
  state.sceneTimers.forEach(t => clearTimeout(t));
  state.sceneTimers = [];
}

// ============================================================
// 控制绑定
// ============================================================
function bindControls() {
  btnPrev.addEventListener("click", prev);
  btnNext.addEventListener("click", next);
  btnPlay.addEventListener("click", () => state.isPlaying ? pause() : play());
  btnRestart.addEventListener("click", restart);
  btnSpeed.addEventListener("click", toggleSpeed);
}

function bindKeyboard() {
  document.addEventListener("keydown", (e) => {
    if (e.target.tagName === "INPUT" || e.target.tagName === "TEXTAREA") return;
    switch (e.key) {
      case " ": e.preventDefault(); state.isPlaying ? pause() : play(); break;
      case "ArrowLeft": e.preventDefault(); prev(); break;
      case "ArrowRight": e.preventDefault(); next(); break;
      case "Home": e.preventDefault(); restart(); break;
    }
  });
}

// ============================================================
// 通用渲染辅助
// ============================================================

/** 创建占位符（截图未提供时） */
function placeholder(filename, hint) {
  return `<div class="screenshot-placeholder" data-screenshot="${filename}">
    <div class="placeholder-label">[占位] ${filename}</div>
    <div class="placeholder-hint">${hint}</div>
  </div>`;
}

/** Windows 风格窗口控件（— □ ×） */
function windowControls() {
  return `<div class="window-controls">
    <button class="win-ctrl min" aria-label="最小化">—</button>
    <button class="win-ctrl max" aria-label="最大化">□</button>
    <button class="win-ctrl close" aria-label="关闭">×</button>
  </div>`;
}

/** 创建 TRAE 窗口结构（顶栏 = Tab + 侧边栏图标 + 编辑/帮助 + 窗口控件） */
function traeWindow(innerHtml, opts = {}) {
  const activeTab = opts.activeTab || "Code";
  const tabIcons = {
    Work: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none"><rect x="3" y="7" width="18" height="13" rx="2" stroke="currentColor" stroke-width="1.8"/><path d="M9 7V5a2 2 0 012-2h2a2 2 0 012 2v2" stroke="currentColor" stroke-width="1.8"/></svg>',
    Code: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none"><path d="M16 18l6-6-6-6M8 6l-6 6 6 6" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>',
    Design: '<svg width="14" height="14" viewBox="0 0 24 24" fill="none"><circle cx="13.5" cy="6.5" r="2.5" stroke="currentColor" stroke-width="1.8"/><circle cx="17.5" cy="10.5" r="2.5" stroke="currentColor" stroke-width="1.8"/><circle cx="8.5" cy="7.5" r="2.5" stroke="currentColor" stroke-width="1.8"/><circle cx="6.5" cy="12.5" r="2.5" stroke="currentColor" stroke-width="1.8"/><path d="M12 22a10 10 0 010-20 8 8 0 00-8 8c0 4 3 6 6 6h2a4 4 0 014 4 2 2 0 01-2 2z" stroke="currentColor" stroke-width="1.8"/></svg>',
  };
  const tabs = ["Work", "Code", "Design"].map(t => `<span class="trae-tab ${t === activeTab ? 'active' : ''}">${tabIcons[t]}<span>${t}</span></span>`).join("");
  return `<div class="app-window trae">
    <div class="window-titlebar trae-titlebar">
      <div class="trae-tabs-row">
        ${tabs}
        <span class="trae-sidebar-toggle" title="侧边栏"><svg width="14" height="14" viewBox="0 0 24 24" fill="none"><rect x="3" y="4" width="18" height="16" rx="2" stroke="currentColor" stroke-width="1.8"/><path d="M9 4v16" stroke="currentColor" stroke-width="1.8"/></svg></span>
        <span class="trae-menu-text">编辑(E)</span>
        <span class="trae-menu-text">帮助(H)</span>
      </div>
      ${windowControls()}
    </div>
    <div class="window-body">${innerHtml}</div>
  </div>`;
}

/** 创建 OpenCode 窗口结构（Windows 风格控件 + 标题在左） */
function opencodeWindow(innerHtml, opts = {}) {
  const title = opts.title || "OpenCode";
  return `<div class="app-window opencode">
    <div class="window-titlebar">
      <div class="window-title">${title}</div>
      ${windowControls()}
    </div>
    <div class="window-body">${innerHtml}</div>
  </div>`;
}

/** 创建时间标签 */
function timeLabel(text) {
  return `<div class="time-label">${text}</div>`;
}

/** TRAE 左侧栏（任务列表）— projects: [{ name, tasks: [{ title, time, active }] }] */
function traeSidebarHtml(projects, opts = {}) {
  const username = opts.username || "LingTian303";
  const items = [
    { icon: '<svg viewBox="0 0 24 24" fill="none"><path d="M12 5v14M5 12h14" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>', label: "新建任务" },
    { icon: '<svg viewBox="0 0 24 24" fill="none"><path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>', label: "技能" },
    { icon: '<svg viewBox="0 0 24 24" fill="none"><path d="M21 12a9 9 0 11-18 0 9 9 0 0118 0zM12 8v4l3 2" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>', label: "自动化" },
  ];
  const sidebarItems = items.map(it => `<div class="trae-sidebar-item">${it.icon}<span>${it.label}</span></div>`).join("");
  const projectsHtml = projects.map(p => `
    <div class="trae-project-group">
      <div class="trae-project-name">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" style="color: var(--trae-text-mute);"><path d="M3 7a2 2 0 012-2h4l2 2h8a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2V7z" stroke="currentColor" stroke-width="1.8"/></svg>
        <span>${p.name}</span>
      </div>
      ${p.tasks.map(t => `<div class="trae-task-item ${t.active ? 'active' : ''} ${t.removing ? 'removing' : ''}" ${t.removing ? 'data-remove="1"' : ''}>${t.title}</div>`).join("")}
    </div>
  `).join("");
  return `<div class="trae-sidebar">
    ${sidebarItems}
    <div class="trae-sidebar-section-title"><span>任务列表</span>
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" style="color: var(--trae-text-mute);"><path d="M4 6h16M4 12h16M4 18h16" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>
    </div>
    <div style="flex: 1; overflow-y: auto;">${projectsHtml}</div>
    <div class="trae-sidebar-footer">
      <div class="trae-avatar">${username.charAt(0)}</div>
      <span class="trae-username">${username}</span>
      <span class="trae-online-dot"></span>
    </div>
  </div>`;
}

/** TRAE 聊天区头部 */
function traeChatHeader(title, opts = {}) {
  const timer = opts.timer ? `<span class="trae-chat-timer"><svg width="12" height="12" viewBox="0 0 24 24" fill="none"><circle cx="12" cy="12" r="9" stroke="currentColor" stroke-width="1.8"/><path d="M12 7v5l3 2" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/></svg>${opts.timer}</span>` : "";
  return `<div class="trae-chat-header">
    <span class="trae-chat-title">${title}</span>
    ${timer}
  </div>`;
}

/** TRAE 输入区域（整体大圆角框 + 紫色发送按钮 + 工具栏；opts.showFooter 控制底部上下文选择器） */
function traeInputArea(opts = {}) {
  const placeholder = opts.placeholder || "输入消息…";
  const placeholderClass = opts.typing ? "" : "placeholder";
  const typingClass = opts.typing ? "typing" : "";
  const sendClass = opts.typing ? "" : "dim";
  const sendBtnDisabled = opts.typing ? "" : "disabled";
  const footerHtml = opts.showFooter ? `      <div class="trae-input-footer">
        <span class="trae-context-selector">本地 ▾</span>
        <span class="trae-context-selector">MemoryCenter ▾</span>
      </div>` : "";
  return `<div class="trae-input-area">
    <div class="trae-input-area-inner ${typingClass}">
      <div class="trae-input-box ${typingClass} ${placeholderClass}" id="input-box">${placeholder}</div>
      <div class="trae-input-toolbar">
        <span class="trae-tool-btn"><svg width="16" height="16" viewBox="0 0 24 24" fill="none"><path d="M21.44 11.05l-9.19 9.19a6 6 0 01-8.49-8.49l9.19-9.19a4 4 0 015.66 5.66l-9.2 9.19a2 2 0 01-2.83-2.83l8.49-8.48" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg></span>
        <span class="trae-tool-btn"><svg width="16" height="16" viewBox="0 0 24 24" fill="none"><path d="M16 18l6-6-6-6M8 6l-6 6 6 6" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg></span>
        <span class="trae-speed-label"><svg width="10" height="10" viewBox="0 0 24 24" fill="currentColor"><path d="M13 2L3 14h7v8l10-12h-7V2z"/></svg>速通</span>
        <span class="trae-model-selector">Doubao-Seed-2.1-Pro ▾</span>
        <button class="trae-send-btn ${sendClass}" ${sendBtnDisabled}><svg width="16" height="16" viewBox="0 0 24 24" fill="none"><path d="M12 19V5M5 12l7-7 7 7" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg></button>
      </div>
${footerHtml}
    </div>
  </div>`;
}

/** TRAE 用户消息（右对齐浅灰气泡） */
function traeUserMsg(text, opts = {}) {
  const id = opts.id ? `id="${opts.id}"` : "";
  return `<div class="trae-msg user"><div class="trae-msg-bubble" ${id}>${text}</div></div>`;
}

/** TRAE Agent 消息（无气泡白底直排 + Agent 头部） */
function traeAgentMsg(bodyHtml, opts = {}) {
  const id = opts.id ? `id="${opts.id}"` : "";
  return `<div class="trae-msg agent" ${id}>
    <div class="trae-agent-header">
      <div class="trae-agent-icon">&lt;/&gt;</div>
      <span class="trae-agent-name">TRAE Code</span>
    </div>
    <div class="trae-agent-body">${bodyHtml}</div>
  </div>`;
}

/** TRAE 空状态欢迎页 */
function traeWelcomePage() {
  return `<div class="trae-welcome">
    <div class="trae-welcome-title"><svg width="28" height="28" viewBox="0 0 24 24" fill="none"><path d="M16 18l6-6-6-6M8 6l-6 6 6 6" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>&lt;&gt; Code with TRAE</div>
    <div class="trae-welcome-sub">帮你编写代码、调试 Bug、优化性能等开发工作，交付生产级代码产物。</div>
    <div class="trae-quick-actions">
      <button class="trae-quick-btn">应用开发</button>
      <button class="trae-quick-btn">项目理解</button>
      <button class="trae-quick-btn">游戏创意</button>
      <button class="trae-quick-btn">工具脚本</button>
    </div>
  </div>`;
}

/** TRAE 右侧栏（待办 + 上下文 + 文件列表）
 *  opts: { todos: [{text, done}], contextPercent: number, files: [{name, iconColor}], activeContextTab }
 */
function traeRightSidebarHtml(opts = {}) {
  const todos = opts.todos || [
    { text: "创建 demo 目录结构", done: true },
    { text: "实现 index.html 骨架", done: true },
    { text: "实现 demo.css 样式", done: true },
  ];
  const ctxPercent = opts.contextPercent ?? 67;
  const ctxClass = ctxPercent >= 80 ? "danger" : ctxPercent >= 60 ? "warning" : "";
  const files = opts.files || [
    { name: "demo.js", iconColor: "#f7df1e" },
    { name: "demo.css", iconColor: "#264de4" },
    { name: "index.html", iconColor: "#e34c26" },
    { name: "README.md", iconColor: "#888" },
  ];
  const activeTab = opts.activeContextTab || "files";
  const todoHtml = todos.map(t => `
    <div class="trae-todo-item ${t.done ? 'done' : ''}">
      <span class="trae-todo-check">${t.done ? '✓' : ''}</span>
      <span class="trae-todo-text">${t.text}</span>
    </div>
  `).join("");
  const fileHtml = files.map(f => `
    <div class="trae-file-list-item">
      <svg class="trae-file-list-icon" width="14" height="14" viewBox="0 0 24 24" fill="none" style="color: ${f.iconColor || 'var(--trae-blue)'};">
        <path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8l-6-6z" stroke="currentColor" stroke-width="1.8"/>
        <path d="M14 2v6h6" stroke="currentColor" stroke-width="1.8"/>
      </svg>
      <span>${f.name}</span>
    </div>
  `).join("");
  return `<div class="trae-right-panel">
    <div class="trae-right-section">
      <div class="trae-right-title">
        <span>待办</span>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" style="color: var(--trae-text-mute);"><path d="M12 5v14M5 12h14" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>
      </div>
      ${todoHtml}
    </div>
    <div class="trae-right-section" style="flex: 1;">
      <div class="trae-context-header">
        <span class="trae-context-label">上下文</span>
        <button class="trae-compress-btn">压缩</button>
      </div>
      <div class="trae-context-bar-wrap">
        <div class="trae-context-bar"><div id="ctx-fill" class="trae-context-fill ${ctxClass}" style="width: ${ctxPercent}%"></div></div>
        <div id="ctx-percent" class="trae-context-percent">${ctxPercent}%</div>
      </div>
      <div class="trae-context-tabs">
        <span class="trae-context-tab ${activeTab === 'files' ? 'active' : ''}">文件</span>
        <span class="trae-context-tab ${activeTab === 'other' ? 'active' : ''}">其他</span>
      </div>
      <div class="trae-file-list">${fileHtml}</div>
    </div>
  </div>`;
}

/** 打字机效果 */
function typewriter(element, text, speed = 30, onDone) {
  let i = 0;
  element.textContent = "";
  element.classList.add("typing");
  const tick = () => {
    if (i < text.length) {
      element.textContent += text[i++];
      addSceneTimer(setTimeout(tick, speed));
    } else {
      element.classList.remove("typing");
      if (onDone) onDone();
    }
  };
  tick();
}

/** 延迟工具 */
function delay(ms, fn) {
  addSceneTimer(setTimeout(fn, ms / state.speed));
}

/** OpenCode 主界面（项目列表 + 最近会话）
 *  opts: {
 *    projects: [{ name, icon, iconColor, active }],
 *    sessions: [{ name, icon, iconColor }],
 *    activeSession: string
 *  }
 */
function opencodeHomeHtml(opts = {}) {
  const projects = opts.projects || [
    { name: "A", icon: "A", iconColor: "#9b59b6", active: true },
    { name: "HelixBlog", icon: "H", iconColor: "#2dd4bf" },
    { name: "NovaCraft", icon: "N", iconColor: "#2dd4bf" },
  ];
  const sessions = opts.sessions || [
    { name: "秘密项目代号查询", icon: "A", iconColor: "#9b59b6" },
    { name: "记录 PurpleDragon77 项目代号", icon: "A", iconColor: "#9b59b6" },
  ];
  const projectItems = projects.map(p => `
    <div class="oc-home-item ${p.active ? 'active' : ''}">
      <div class="oc-avatar" style="background: ${p.iconColor || '#3a3a48'};">${p.icon || p.name.charAt(0)}</div>
      <span>${p.name}</span>
    </div>
  `).join("");
  const sessionItems = sessions.map(s => `
    <div class="oc-home-item">
      <div class="oc-avatar" style="background: ${s.iconColor || '#3a3a48'};">${s.icon || 'A'}</div>
      <span>${s.name}</span>
    </div>
  `).join("");
  return `<div class="oc-home">
    <div class="oc-search-box">
      <svg viewBox="0 0 24 24" fill="none"><circle cx="11" cy="11" r="7" stroke="currentColor" stroke-width="1.8"/><path d="M16 16l4 4" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/></svg>
      <span>在 AI 中搜索会话</span>
    </div>
    <div class="oc-home-grid">
      <div class="oc-home-col">
        <div class="oc-home-col-title">
          <span>项目</span>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" style="color: var(--oc-text-dim);"><path d="M12 5v14M5 12h14" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>
        </div>
        ${projectItems}
      </div>
      <div class="oc-home-col">
        <div class="oc-home-col-title">
          <span>最近会话</span>
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" style="color: var(--oc-text-dim);"><path d="M12 5v7l4 2M3 12a9 9 0 1118 0 9 9 0 01-18 0z" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/></svg>
        </div>
        ${sessionItems}
      </div>
    </div>
    <div class="oc-home-footer">
      <span><svg width="14" height="14" viewBox="0 0 24 24" fill="none"><circle cx="12" cy="12" r="3" stroke="currentColor" stroke-width="1.8"/><path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 11-2.83 2.83l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 11-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 11-2.83-2.83l.06-.06a1.65 1.65 0 00.33-1.82 1.65 1.65 0 00-1.51-1H3a2 2 0 110-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 112.83-2.83l.06.06a1.65 1.65 0 001.82.33H9a1.65 1.65 0 001-1.51V3a2 2 0 114 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 112.83 2.83l-.06.06a1.65 1.65 0 00-.33 1.82V9a1.65 1.65 0 001.51 1H21a2 2 0 110 4h-.09a1.65 1.65 0 00-1.51 1z" stroke="currentColor" stroke-width="1.5"/></svg>设置</span>
      <span><svg width="14" height="14" viewBox="0 0 24 24" fill="none"><circle cx="12" cy="12" r="10" stroke="currentColor" stroke-width="1.8"/><path d="M9.09 9a3 3 0 015.83 1c0 2-3 3-3 3M12 17h.01" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/></svg>帮助</span>
    </div>
  </div>`;
}

/** OpenCode 空聊天水印 */
function opencodeWatermarkHtml() {
  return `<div class="oc-watermark"><div class="oc-watermark-text">OpenCode</div></div>`;
}

/** OpenCode 顶部栏（左侧菜单/应用网格 + 新建会话 + Tab + 右侧状态/窗口控件）
 *  opts: { title, tabs: [{ name, active }], showNewSession, leftIcon: 'menu' | 'grid' }
 */
function opencodeTopbarHtml(opts = {}) {
  const title = opts.title || "新建会话";
  const tabs = opts.tabs || [];
  const showNewSession = opts.showNewSession !== false;
  const leftIcon = opts.leftIcon || "menu";
  const menuIcon = leftIcon === "grid"
    ? `<svg width="16" height="16" viewBox="0 0 24 24" fill="none"><rect x="3" y="3" width="7" height="7" rx="1" stroke="currentColor" stroke-width="1.8"/><rect x="14" y="3" width="7" height="7" rx="1" stroke="currentColor" stroke-width="1.8"/><rect x="3" y="14" width="7" height="7" rx="1" stroke="currentColor" stroke-width="1.8"/><rect x="14" y="14" width="7" height="7" rx="1" stroke="currentColor" stroke-width="1.8"/></svg>`
    : `<svg width="16" height="16" viewBox="0 0 24 24" fill="none"><path d="M3 12h18M3 6h18M3 18h18" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>`;
  const tabHtml = tabs.map(t => `
    <div class="oc-tab ${t.active ? 'active' : ''}">
      <span class="oc-tab-icon"><svg width="14" height="14" viewBox="0 0 24 24" fill="none"><path d="M3 7l9-4 9 4M3 7v10l9 4 9-4V7M3 7l9 4 9-4" stroke="currentColor" stroke-width="1.5"/></svg></span>
      <span class="oc-tab-name">${t.name}</span>
      <span class="oc-tab-close">×</span>
    </div>
  `).join("");
  const newSessionHtml = showNewSession ? `
    <div class="oc-new-session">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none"><path d="M12 5v14M5 12h14" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>
      <span>${title}</span>
    </div>
  ` : "";
  return `<div class="oc-tabbar">
    <div class="oc-topbar-left">
      <div class="oc-icon-btn">${menuIcon}</div>
      ${newSessionHtml}
    </div>
    <div class="oc-tabs-wrap">${tabHtml}</div>
    <div class="oc-tabbar-right">
      <span class="oc-online-dot"></span>
      ${windowControls()}
    </div>
  </div>`;
}

/** OpenCode 聊天区头部 */
function opencodeChatHeader(title, opts = {}) {
  const right = opts.right || `<div class="oc-chat-titlebar-right"><svg width="16" height="16" viewBox="0 0 24 24" fill="none"><circle cx="12" cy="12" r="3" stroke="currentColor" stroke-width="1.8"/><path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 11-2.83 2.83l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 11-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 11-2.83-2.83l.06-.06a1.65 1.65 0 00.33-1.82 1.65 1.65 0 00-1.51-1H3a2 2 0 110-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 112.83-2.83l.06.06a1.65 1.65 0 001.82.33H9a1.65 1.65 0 001-1.51V3a2 2 0 114 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 112.83 2.83l-.06.06a1.65 1.65 0 00-.33 1.82V9a1.65 1.65 0 001.51 1H21a2 2 0 110 4h-.09a1.65 1.65 0 00-1.51 1z" stroke="currentColor" stroke-width="1.5"/></svg></div>`;
  return `<div class="oc-chat-titlebar">
    <span class="oc-chat-title">${title}</span>
    ${right}
  </div>`;
}

/** OpenCode 用户消息（右对齐深灰气泡） */
function opencodeUserMsg(text, opts = {}) {
  const id = opts.id ? `id="${opts.id}"` : "";
  return `<div class="oc-msg-user" ${id}>${text}</div>`;
}

/** OpenCode Agent 思考过程（灰色可见推理链 + 工具调用行） */
function opencodeThinkingHtml(lines, opts = {}) {
  const id = opts.id ? `id="${opts.id}"` : "";
  const lineHtml = lines.map(l => {
    if (l.type === "tool") {
      return `<div class="oc-tool-call"><span class="oc-code">${l.name}</span><span class="oc-arg">${l.args}</span> ✓</div>`;
    }
    return `<p>${l.text}</p>`;
  }).join("");
  return `<div class="oc-msg-thinking" ${id}>${lineHtml}</div>`;
}

/** OpenCode Agent 最终回复（左对齐浅色文字） */
function opencodeAgentReply(html, opts = {}) {
  const id = opts.id ? `id="${opts.id}"` : "";
  return `<div class="oc-msg-reply" ${id}>${html}</div>`;
}

/** OpenCode 输入区域 */
function opencodeInputArea(opts = {}) {
  const placeholder = opts.placeholder || "Ask anything, '/' for commands, @ for context...";
  const focused = opts.focused ? "focused" : "";
  const typing = opts.typing ? "typing" : "";
  const sendActive = opts.typing ? "active" : "";
  const textId = opts.textId ? `id="${opts.textId}"` : "";
  return `<div class="oc-input-area">
    <div class="oc-input-box ${focused} ${typing}">
      <span class="oc-input-text ${typing ? '' : 'is-placeholder'}" ${textId}>${placeholder}</span>
    </div>
    <div class="oc-input-toolbar">
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none"><path d="M12 5v14M5 12h14" stroke="currentColor" stroke-width="2" stroke-linecap="round"/></svg>
      <span class="oc-model-select">Sisyphus - ultrawide</span>
      <span class="oc-sep">|</span>
      <span class="oc-model-select">Hy3 Free</span>
      <span class="oc-sep">|</span>
      <span class="oc-model-select">High</span>
      <div class="oc-send-btn ${sendActive}"><svg width="14" height="14" viewBox="0 0 24 24" fill="none"><path d="M12 19V5M5 12l7-7 7 7" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg></div>
    </div>
  </div>`;
}

/** 创建工具卡片 */
function toolCard(name, args, status = "running") {
  return `<div class="tool-card">
    <div class="tool-card-header">
      <svg class="tool-icon" viewBox="0 0 24 24" fill="none"><path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>
      <span class="tool-name">${name}</span>
      <span class="tool-status ${status}">${status === "running" ? "运行中" : "完成"}</span>
    </div>
    <div class="tool-args">${args}</div>
  </div>`;
}

// ============================================================
// 场景旁白
// ============================================================
const SCENE_CAPTIONS = {
  1: "打开 TRAE，用户发出安装指令，Agent 自动完成 MemoryCenter 安装",
  2: "在 MCP 配置页确认 MemoryCenter 已接入，展开 21 个工具",
  3: "回到会话，准备开始真正的开发",
  4: "用户决定用 MemoryCenter 辅助整个项目开发，Agent 完成初始配置",
  5: "可视化界面实时显示 Agent 的归档操作与项目信息归档",
  6: "项目进入真实生产阶段，基于已归档信息开始实现功能",
  7: "第三天，用户清理了部分旧会话，继续开发",
  8: "第五天遇到重大问题，MemoryCenter 检索定位到关键信息，问题得以解决",
  9: "第七天周级合并，第三周合并已成为后台常态",
  10: "用户将 OpenCode 项目迁移到 TRAE，跨 Agent 记忆零损耗",
  11: "17 类语义标签让记忆可分类、可检索、可治理，跨 Agent 决策延续",
  12: "伪钩子方案——压缩前完整归档，零信息丢失",
  13: "月级评分淘汰、7 种语言绑定、21 个 MCP 工具，开发者一行代码接入",
  14: "MemoryCenter —— 给 AI Agent 装一块记忆硬盘",
};

// ============================================================
// 场景渲染器（14 个）
// ============================================================
const SCENE_RENDERERS = {
  1: renderScene01,
  2: renderScene02,
  3: renderScene03,
  4: renderScene04,
  5: renderScene05,
  6: renderScene06,
  7: renderScene07,
  8: renderScene08,
  9: renderScene09,
  10: renderScene10,
  11: renderScene11,
  12: renderScene12,
  13: renderScene13,
  14: renderScene14,
};

// 场景渲染函数实现（14 个场景，合并自原 31 个）
// ============================================================

// 场景 1：安装 MemoryCenter（合并原1-4：空状态 → 打字 → 工具卡片 → 完成）
function renderScene01(stage, scene) {
  const projects = [
    { name: "HelixBlog", tasks: [{ title: "首页改版" }] },
    { name: "MemoryCenter", tasks: [
      { title: "安装 MemoryCenter", active: true },
      { title: "MCP 工具接入" },
      { title: "项目初始化配置" },
      { title: "Demo 脚本规划" },
      { title: "能力梳理" },
    ]},
    { name: "NovaCraft", tasks: [{ title: "产品介绍页" }] },
  ];
  const rightPanel = traeRightSidebarHtml({
    todos: [
      { text: "确认 MemoryCenter 版本", done: false },
      { text: "配置 .mcp.json", done: false },
      { text: "验证 MCP 连接", done: false },
    ],
    contextPercent: 32,
    files: [
      { name: ".mcp.json", iconColor: "#888" },
      { name: "Cargo.toml", iconColor: "#dea584" },
    ],
  });
  // Step 1：渲染空状态 TRAE 窗口（桌面背景由 #stage-container.desktop-bg 提供，TRAE 占满 stage）
  stage.innerHTML = `
    <div style="width: 100%; height: 100%; animation: sceneFadeIn 0.6s ease-out;">
      ${traeWindow(`
        <div class="trae-layout">
          ${traeSidebarHtml(projects)}
          <div class="trae-chat">
            ${traeChatHeader("安装 MemoryCenter", { timer: "00:23" })}
            <div class="trae-chat-body" id="chat-msgs">${traeWelcomePage()}</div>
            ${traeInputArea({ showFooter: true })}
          </div>
          ${rightPanel}
        </div>
      `)}
    </div>
  `;
  // Step 2：2.5s 后输入框打字
  delay(2500, () => {
    const inputBox = $("#input-box");
    const chatMsgs = $("#chat-msgs");
    inputBox.classList.remove("placeholder");
    inputBox.classList.add("typing");
    inputBox.textContent = "";
    typewriter(inputBox, "帮我安装 MemoryCenter 工具", 40, () => {
      // Step 3：打字完成 → 显示用户消息 + Agent 思考容器
      delay(300, () => {
        inputBox.textContent = "";
        inputBox.classList.remove("typing");
        inputBox.classList.add("placeholder");
        inputBox.textContent = "输入消息…";
        chatMsgs.innerHTML = `
          ${traeUserMsg("帮我安装 MemoryCenter 工具")}
          ${traeAgentMsg(`
            <div class="trae-collapse-bar">思考过程</div>
            <div id="tool-cards"></div>
          `, { id: "agent-msg" })}
        `;
        // Step 4-6：依次出现 3 个工具卡片
        const toolCards = $("#tool-cards");
        const tools = [
          { name: "RunCommand", args: "cargo install memory-center-mcp", d: 400 },
          { name: "Write", args: ".mcp.json", d: 1400 },
          { name: "RunCommand", args: "memory-center-mcp --version → v2.37", d: 2400 },
        ];
        tools.forEach((t) => {
          delay(t.d, () => {
            toolCards.insertAdjacentHTML("beforeend", toolCard(t.name, t.args, "running"));
            delay(700, () => {
              const last = toolCards.querySelector(".tool-card:last-child .tool-status");
              if (last) { last.classList.remove("running"); last.classList.add("done"); last.textContent = "完成"; }
            });
          });
        });
        // Step 7：显示安装完成状态
        delay(3400, () => {
          toolCards.insertAdjacentHTML("beforeend", `
            <div class="trae-step" style="margin-top: 8px;"><span class="trae-step-check">✓</span>MemoryCenter 已安装完成，版本 v2.37。你可以在 MCP 配置页查看。</div>
            <div class="trae-task-done"><svg width="14" height="14" viewBox="0 0 24 24" fill="none"><path d="M9 12l2 2 4-4M21 12a9 9 0 11-18 0 9 9 0 0118 0z" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>任务完成</div>
          `);
        });
      });
    });
  });
}

// 场景 2：MCP 工具接入（合并原5-6：MCP 设置弹窗 + 工具列表展开）
function renderScene02(stage, scene) {
  const projects = [
    { name: "MemoryCenter", tasks: [{ title: "安装 MemoryCenter", active: true }] },
  ];
  stage.innerHTML = traeWindow(`
    <div class="trae-layout" style="position: relative;">
      ${traeSidebarHtml(projects)}
      <div class="trae-chat">
        ${traeChatHeader("安装 MemoryCenter", { timer: "01:15" })}
        <div class="trae-chat-body">
          ${traeUserMsg("帮我安装 MemoryCenter 工具")}
          ${traeAgentMsg(`<div style="font-size: 13px;">MemoryCenter 已安装完成，版本 v2.37。你可以在 MCP 配置页查看。</div>`)}
        </div>
        ${traeInputArea()}
      </div>
      <div class="trae-modal-overlay">
        <div class="trae-modal">
          <div class="trae-modal-top">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" style="color: var(--trae-text-dim);"><circle cx="12" cy="12" r="3" stroke="currentColor" stroke-width="1.8"/><path d="M19.4 15a1.65 1.65 0 00.33 1.82l.06.06a2 2 0 11-2.83 2.83l-.06-.06a1.65 1.65 0 00-1.82-.33 1.65 1.65 0 00-1 1.51V21a2 2 0 11-4 0v-.09A1.65 1.65 0 009 19.4a1.65 1.65 0 00-1.82.33l-.06.06a2 2 0 11-2.83-2.83l.06-.06a1.65 1.65 0 00.33-1.82 1.65 1.65 0 00-1.51-1H3a2 2 0 110-4h.09A1.65 1.65 0 004.6 9a1.65 1.65 0 00-.33-1.82l-.06-.06a2 2 0 112.83-2.83l.06.06a1.65 1.65 0 001.82.33H9a1.65 1.65 0 001-1.51V3a2 2 0 114 0v.09a1.65 1.65 0 001 1.51 1.65 1.65 0 001.82-.33l.06-.06a2 2 0 112.83 2.83l-.06.06a1.65 1.65 0 00-.33 1.82V9a1.65 1.65 0 001.51 1H21a2 2 0 110 4h-.09a1.65 1.65 0 00-1.51 1z" stroke="currentColor" stroke-width="1.5"/></svg>
            <span style="font-size: 14px; font-weight: 600; color: var(--trae-text);">设置</span>
            <span style="margin-left: auto; cursor: pointer; color: var(--trae-text-mute); font-size: 18px;">×</span>
          </div>
          <div class="trae-modal-body">
            <div class="trae-settings-menu">
              <div class="trae-settings-item">常规</div>
              <div class="trae-settings-item">编辑器</div>
              <div class="trae-settings-item active">MCP</div>
              <div class="trae-settings-item">关于</div>
            </div>
            <div class="trae-settings-content">
              <div class="trae-settings-title">MCP</div>
              <div class="trae-settings-tabs">
                <span class="trae-settings-tab active">服务器</span>
                <span class="trae-settings-tab">本地资源</span>
              </div>
              <div class="trae-mcp-section-title">
                <h3>MCP 服务器</h3>
                <button class="trae-mcp-add-btn">+ 添加 MCP Server</button>
              </div>
              <div class="trae-mcp-desc">MCP 服务器可为 Agent 提供工具能力。</div>
              <div class="trae-mcp-list" id="mcp-list"></div>
            </div>
          </div>
        </div>
      </div>
    </div>
  `);
  const list = $("#mcp-list");
  const items = [
    { name: "filesystem", highlight: false, color: "#888" },
    { name: "MemoryCenter", highlight: true, version: "v2.37", color: "#7c3aed", expanded: true },
    { name: "git", highlight: false, color: "#f1502f" },
  ];
  items.forEach((item, i) => {
    delay(i * 400, () => {
      const toolsHtml = item.expanded ? `<div class="trae-mcp-tools" id="tool-list"></div>` : "";
      list.insertAdjacentHTML("beforeend", `
        <div class="trae-mcp-item ${item.highlight ? 'expanded' : ''}">
          <span class="trae-mcp-expand">▸</span>
          <div class="trae-mcp-icon" style="background: ${item.color};">${item.name.charAt(0).toUpperCase()}</div>
          <span class="trae-mcp-name">${item.name}</span>
          ${item.version ? `<span style="font-size: 11px; color: var(--trae-text-mute); padding: 2px 6px; background: var(--trae-card-bg); border-radius: 4px;">${item.version}</span>` : ''}
          <span class="trae-mcp-check">✓</span>
          <div class="trae-mcp-toggle"></div>
          ${toolsHtml}
        </div>
      `);
      if (item.expanded) {
        const toolList = $("#tool-list");
        const tools = [
          { name: "prompt", desc: "获取历史记忆" },
          { name: "archive", desc: "归档对话" },
          { name: "semantic_search", desc: "语义检索" },
          { name: "pre_compress_hook", desc: "压缩前归档" },
          { name: "retrieve", desc: "检索记忆" },
          { name: "compaction", desc: "周期合并" },
        ];
        tools.forEach((t, j) => {
          delay(300 + j * 250, () => {
            toolList.insertAdjacentHTML("beforeend", `
              <div class="trae-mcp-tool-row">
                <span class="trae-mcp-tool-name">${t.name}</span>
                <span style="color: var(--trae-text-mute);">${t.desc}</span>
              </div>
            `);
          });
        });
        delay(300 + 6 * 250 + 400, () => {
          toolList.insertAdjacentHTML("beforeend", `
            <div style="text-align: center; font-size: 11px; color: var(--trae-text-mute); margin-top: 8px;">+ 15 个更多工具</div>
          `);
        });
      }
    });
  });
}

// 场景 3：准备开始开发（原7：关闭设置回到会话，待办依次勾选）
function renderScene03(stage, scene) {
  const projects = [
    { name: "MemoryCenter", tasks: [{ title: "安装 MemoryCenter", active: true }] },
  ];
  // 初始渲染：待办全部未完成
  const rightPanel = traeRightSidebarHtml({
    todos: [
      { text: "确认 MemoryCenter 版本", done: false },
      { text: "配置 .mcp.json", done: false },
      { text: "验证 MCP 连接", done: false },
    ],
    contextPercent: 40,
    files: [
      { name: ".mcp.json", iconColor: "#888" },
      { name: "Cargo.toml", iconColor: "#dea584" },
    ],
  });
  stage.innerHTML = traeWindow(`
    <div class="trae-layout">
      ${traeSidebarHtml(projects)}
      <div class="trae-chat">
        ${traeChatHeader("安装 MemoryCenter", { timer: "01:18" })}
        <div class="trae-chat-body">
          ${traeUserMsg("帮我安装 MemoryCenter 工具")}
          ${traeAgentMsg(`
            <div style="font-size: 13px;">MemoryCenter 已安装完成，版本 v2.37。你可以在 MCP 配置页查看。</div>
            <div class="trae-task-done" style="margin-top: 8px;"><svg width="14" height="14" viewBox="0 0 24 24" fill="none"><path d="M9 12l2 2 4-4M21 12a9 9 0 11-18 0 9 9 0 0118 0z" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>任务完成</div>
          `)}
        </div>
        ${traeInputArea()}
      </div>
      ${rightPanel}
    </div>
  `);
  // 依次勾选待办项
  const todoItems = document.querySelectorAll(".trae-todo-item");
  todoItems.forEach((item, i) => {
    delay(300 + i * 500, () => {
      item.classList.add("done");
      const check = item.querySelector(".trae-todo-check");
      if (check) check.textContent = "✓";
    });
  });
}

// 场景 4：启动记忆辅助（合并原8-9：打字发送 → Agent 执行初始配置）
function renderScene04(stage, scene) {
  const projects = [
    { name: "HelixBlog", tasks: [{ title: "首页改版" }] },
    { name: "MemoryCenter", tasks: [
      { title: "MCP 工具接入" },
      { title: "用 MC 辅助开发", active: true },
    ]},
    { name: "NovaCraft", tasks: [{ title: "产品介绍页" }] },
  ];
  stage.innerHTML = `<div class="desktop-scene">${traeWindow(`
    <div class="trae-layout">
      ${traeSidebarHtml(projects)}
      <div class="trae-chat">
        ${traeChatHeader("用 MC 辅助开发", { timer: "00:08" })}
        <div class="trae-chat-body" id="chat-body"></div>
        ${traeInputArea({ typing: true })}
      </div>
    </div>
  `)}</div>`;
  const inputBox = $("#input-box");
  const chatBody = $("#chat-body");
  typewriter(inputBox, "接下来都将用 MemoryCenter 辅助我们进行开发项目", 35, () => {
    delay(300, () => {
      inputBox.classList.remove("typing");
      inputBox.classList.add("placeholder");
      inputBox.textContent = "输入消息…";
      chatBody.insertAdjacentHTML("beforeend", traeUserMsg("接下来都将用 MemoryCenter 辅助我们进行开发项目"));
      // Agent 执行初始配置
      delay(500, () => {
        chatBody.insertAdjacentHTML("beforeend", traeAgentMsg(`
          <div class="trae-collapse-bar">思考过程</div>
          <div id="tool-cards"></div>
        `));
        const toolCards = $("#tool-cards");
        const tools = [
          { name: "prompt", args: "session_id=trae-mc-20260713", d: 200 },
          { name: "install_rules", args: "安装记忆协议规则到项目", d: 1000 },
          { name: "get_config", args: "查询运行时配置", d: 1800 },
          { name: "preset_build", args: "Agent=Trae, Scenario=coding", d: 2600 },
        ];
        tools.forEach(t => {
          delay(t.d, () => {
            toolCards.insertAdjacentHTML("beforeend", toolCard(t.name, t.args, "running"));
            delay(600, () => {
              const last = toolCards.querySelector(".tool-card:last-child .tool-status");
              if (last) { last.classList.remove("running"); last.classList.add("done"); last.textContent = "完成"; }
            });
          });
        });
        delay(3400, () => {
          toolCards.insertAdjacentHTML("beforeend", `<div class="trae-step" style="margin-top: 8px;"><span class="trae-step-check">✓</span>MemoryCenter 初始配置完成。我将在对话过程中自动归档关键信息。</div>`);
        });
      });
    });
  });
}

// 场景 5：实时归档可视化（合并原10-12：分屏 archive + MC 仪表盘日志流 + 标签云）
function renderScene05(stage, scene) {
  const projects = [
    { name: "MemoryCenter", tasks: [{ title: "用 MC 辅助开发", active: true }] },
  ];
  stage.innerHTML = `
    <div class="split-screen">
      <div class="split-pane">${traeWindow(`
        <div class="trae-layout">
          <div class="trae-chat">
            ${traeChatHeader("用 MC 辅助开发", { timer: "00:25" })}
            <div class="trae-chat-body">
              ${traeUserMsg("接下来都将用 MemoryCenter 辅助我们进行开发项目")}
              ${traeAgentMsg(`
                <div class="trae-collapse-bar">思考过程</div>
                <div id="tool-cards"></div>
              `)}
            </div>
            ${traeInputArea()}
          </div>
        </div>
      `)}</div>
      <div class="split-pane">
        <div class="split-pane-header"><svg width="14" height="14" viewBox="0 0 24 24" fill="none" style="color: var(--accent);"><circle cx="12" cy="12" r="3" fill="currentColor"/><circle cx="12" cy="12" r="8" stroke="currentColor" stroke-width="1.5"/></svg>MemoryCenter 记忆库</div>
        <div style="flex: 1; padding: 8px;">
          <div class="mc-dashboard" style="padding: 8px; gap: 8px;">
            <div class="mc-header" style="padding-bottom: 4px; grid-column: 1/-1;"><div class="mc-title">记忆库仪表盘</div><span class="mc-status-badge active">归档中</span></div>
            <div class="mc-tagcloud" id="mc-tags" style="grid-column: 1/-1;"></div>
            <div class="mc-log-stream" id="mc-log" style="grid-column: 1/-1; max-height: 120px;"></div>
          </div>
        </div>
      </div>
    </div>
  `;
  const toolCards = $("#tool-cards");
  const mcLog = $("#mc-log");
  const mcTags = $("#mc-tags");
  // Phase 1: archive 工具卡片 + 日志流
  delay(200, () => {
    toolCards.insertAdjacentHTML("beforeend", toolCard("archive", "session_id=trae-mc-20260713", "running"));
  });
  delay(1200, () => {
    const last = toolCards.querySelector(".tool-card:last-child .tool-status");
    if (last) { last.classList.remove("running"); last.classList.add("done"); last.textContent = "完成"; }
  });
  const logs1 = [
    { time: "10:23", action: "archive", target: "trae-memory-center-20260713", d: 600 },
    { time: "10:23", action: "write", target: "daily/2026-07-13_102300.json", d: 1400 },
    { time: "10:24", action: "update_index", target: "daily_index.json", d: 2200 },
  ];
  logs1.forEach(l => {
    delay(l.d, () => {
      mcLog.insertAdjacentHTML("beforeend", `<div class="log-line"><span class="log-time">${l.time}</span><span class="log-action">${l.action}</span> → <span class="log-target">${l.target}</span></div>`);
      mcLog.scrollTop = mcLog.scrollHeight;
    });
  });
  // Phase 2: update_project_memory + 标签云
  delay(3000, () => {
    toolCards.insertAdjacentHTML("beforeend", toolCard("update_project_memory", "section=task_state, decisions, risks", "running"));
  });
  delay(4000, () => {
    const last = toolCards.querySelector(".tool-card:last-child .tool-status");
    if (last) { last.classList.remove("running"); last.classList.add("done"); last.textContent = "完成"; }
  });
  const tags = [
    { name: "task_state", count: 3, d: 3200 },
    { name: "decisions", count: 2, d: 3600 },
    { name: "risks", count: 1, d: 4000 },
    { name: "architecture", count: 2, d: 4400 },
    { name: "progress", count: 4, d: 4800 },
    { name: "next_steps", count: 2, d: 5200 },
  ];
  tags.forEach(t => {
    delay(t.d, () => {
      mcTags.insertAdjacentHTML("beforeend", `<span class="tag-chip">${t.name}<span class="tag-count">${t.count}</span></span>`);
    });
  });
  delay(3500, () => {
    mcLog.insertAdjacentHTML("beforeend", `<div class="log-line"><span class="log-time">10:25</span><span class="log-action">update_project_memory</span> → <span class="log-target">task_state</span></div>`);
    mcLog.scrollTop = mcLog.scrollHeight;
  });
  delay(4500, () => {
    mcLog.insertAdjacentHTML("beforeend", `<div class="log-line"><span class="log-time">10:26</span><span class="log-action">update_project_memory</span> → <span class="log-target">decisions</span></div>`);
    mcLog.scrollTop = mcLog.scrollHeight;
  });
  delay(5500, () => {
    mcLog.insertAdjacentHTML("beforeend", `<div class="log-line"><span class="log-time">10:27</span><span class="log-action">update_project_memory</span> → <span class="log-target">risks</span></div>`);
    mcLog.scrollTop = mcLog.scrollHeight;
  });
}

// 场景 6：进入生产阶段（原13：Agent 基于 已归档信息开始实现功能）
function renderScene06(stage, scene) {
  const projects = [
    { name: "MemoryCenter", tasks: [{ title: "用 MC 辅助开发", active: true }] },
  ];
  stage.innerHTML = traeWindow(`
    <div class="trae-layout" style="position: relative;">
      ${timeLabel("第一天")}
      ${traeSidebarHtml(projects)}
      <div class="trae-chat">
        ${traeChatHeader("用 MC 辅助开发", { timer: "00:38" })}
        <div class="trae-chat-body">
          ${traeUserMsg("开始实现用户登录模块")}
          ${traeAgentMsg(`<div id="agent-reply" style="font-size: 13px; line-height: 1.7;"></div>`)}
        </div>
        ${traeInputArea()}
      </div>
    </div>
  `);
  typewriter($("#agent-reply"), "好的，基于已归档的项目信息（task_state + decisions + architecture），我来实现用户登录模块。会沿用已有的认证中间件分层和 Argon2 密码哈希约定…", 30);
}

// 场景 7：时间跳转：第三天（原14：侧栏任务依次出现 → 旧会话淡出）
function renderScene07(stage, scene) {
  const projects = [
    { name: "MemoryCenter", tasks: [
      { title: "安装 MemoryCenter", time: "2天前", removing: true },
      { title: "用 MC 辅助开发", time: "2天前", removing: true },
      { title: "实现登录模块", time: "1天前", removing: true },
      { title: "优化登录鉴权", time: "昨天" },
      { title: "继续优化登录模块", time: "刚刚", active: true },
    ]},
  ];
  stage.innerHTML = traeWindow(`
    <div class="trae-layout" style="position: relative;">
      ${timeLabel("第三天")}
      ${traeSidebarHtml(projects)}
      <div class="trae-chat">
        ${traeChatHeader("继续优化登录模块", { timer: "00:05" })}
        <div class="trae-chat-body">
          ${traeUserMsg("继续优化登录模块")}
        </div>
        ${traeInputArea()}
      </div>
    </div>
  `);
  // 先隐藏所有任务条目，逐个淡入
  const taskItems = document.querySelectorAll('.trae-task-item');
  taskItems.forEach(el => { el.style.opacity = '0'; el.style.transition = 'opacity 0.3s'; });
  taskItems.forEach((el, i) => {
    delay(i * 200, () => { el.style.opacity = '1'; });
  });
  // 旧会话淡出
  delay(taskItems.length * 200 + 600, () => {
    document.querySelectorAll('[data-remove="1"]').forEach(el => {
      el.classList.add("removing");
      delay(400, () => el.remove());
    });
  });
}

// 场景 8：跨会话记忆检索（合并原15-18：第五天遇问题 → 检索 → 定位 → 修复）
function renderScene08(stage, scene) {
  const projects = [
    { name: "MemoryCenter", tasks: [
      { title: "优化登录鉴权", time: "2天前" },
      { title: "登录鉴权问题", time: "刚刚", active: true },
    ]},
  ];
  // Phase 1: 单屏 TRAE，第五天
  stage.innerHTML = traeWindow(`
    <div class="trae-layout" style="position: relative;">
      ${timeLabel("第五天")}
      <div class="trae-chat">
        ${traeChatHeader("登录鉴权问题", { timer: "00:08" })}
        <div class="trae-chat-body" id="chat-body">
          ${traeUserMsg(`<span id="user-msg-1"></span>`)}
        </div>
        ${traeInputArea()}
      </div>
    </div>
  `);
  // 0.5s: 打字第一条用户消息
  typewriter($("#user-msg-1"), "这个登录鉴权方案有问题，记得第二天我们讨论过一个关键的安全策略，但我找不到那个会话了", 30);
  // 2.5s: Agent 提示检索
  delay(2500, () => {
    $("#chat-body").insertAdjacentHTML("beforeend", traeAgentMsg(`<div style="font-size: 13px;">该会话可能在第三天被清理了，但 MemoryCenter 中应已归档。让我检索…</div>`));
  });
  // 3.5s: 打字第二条用户消息
  delay(3500, () => {
    $("#chat-body").insertAdjacentHTML("beforeend", traeUserMsg(`<span id="user-msg-2"></span>`));
    typewriter($("#user-msg-2"), "找找第二天关于登录鉴权安全策略的历史记忆", 30);
  });
  // 5s: 切换到分屏（检索可视化）
  delay(5000, () => {
    stage.innerHTML = `
      <div class="split-screen" style="position: relative;">
        ${timeLabel("第五天")}
        <div class="split-pane">${traeWindow(`
          <div class="trae-layout">
            <div class="trae-chat">
              ${traeChatHeader("登录鉴权问题", { timer: "00:15" })}
              <div class="trae-chat-body">
                ${traeUserMsg("找找第二天关于登录鉴权安全策略的历史记忆")}
                ${traeAgentMsg(`
                  <div class="trae-collapse-bar">思考过程</div>
                  <div id="tool-cards"></div>
                `)}
              </div>
              ${traeInputArea()}
            </div>
          </div>
        `)}</div>
        <div class="split-pane">
          <div class="split-pane-header"><svg width="14" height="14" viewBox="0 0 24 24" fill="none" style="color: var(--accent);"><circle cx="12" cy="12" r="3" fill="currentColor"/><circle cx="12" cy="12" r="8" stroke="currentColor" stroke-width="1.5"/></svg>MemoryCenter 检索中</div>
          <div style="flex: 1; padding: 12px; display: flex; flex-direction: column; gap: 8px;" id="search-area"></div>
        </div>
      </div>
    `;
    delay(300, () => {
      $("#tool-cards").insertAdjacentHTML("beforeend", toolCard("semantic_search", 'query="登录鉴权安全策略" top_k=5', "running"));
    });
    const searchArea = $("#search-area");
    // 5.5s: 阶段 1：扫描 12 条
    delay(500, () => {
      searchArea.innerHTML = `<div style="font-size: 11px; color: var(--text-mute); margin-bottom: 4px;">扫描记忆库…</div>`;
      for (let i = 0; i < 12; i++) {
        searchArea.insertAdjacentHTML("beforeend", `<div style="height: 24px; background: var(--surface-2); border: 1px solid var(--border); border-radius: 4px; opacity: ${0.3 + Math.random() * 0.4}; animation: msgSlideIn 0.3s;"></div>`);
      }
    });
    // 7s: 阶段 2：收缩到 5 条
    delay(2000, () => {
      searchArea.innerHTML = `<div style="font-size: 11px; color: var(--text-mute); margin-bottom: 4px;">缩小到 5 条候选…</div>`;
      for (let i = 0; i < 5; i++) {
        searchArea.insertAdjacentHTML("beforeend", `<div style="height: 32px; background: var(--surface-2); border: 1px solid var(--border); border-radius: 6px; padding: 6px 10px; font-size: 11px; color: var(--text-dim); display: flex; align-items: center;">候选 ${i+1} · 相关度 ${(0.95 - i * 0.08).toFixed(2)}</div>`);
      }
    });
    // 8.5s: 阶段 3：定位到 1 条
    delay(3500, () => {
      searchArea.innerHTML = `<div style="font-size: 11px; color: var(--accent); margin-bottom: 4px; font-weight: 600;">已定位到关键记忆：</div>`;
      searchArea.insertAdjacentHTML("beforeend", `
        <div style="background: var(--surface); border: 2px solid var(--accent); border-radius: 8px; padding: 12px; box-shadow: 0 0 0 4px var(--accent-glow); animation: msgSlideIn 0.4s;">
          <div style="font-size: 11px; color: var(--text-mute); margin-bottom: 4px;">钩子ID: 3a7b... · 标签: security, decisions · 时间: 第二天 14:32</div>
          <div style="font-size: 13px; color: var(--text); font-weight: 500;">"采用 Argon2 密码哈希 + JWT 双 token 刷新机制…"</div>
        </div>
      `);
      const last = $("#tool-cards .tool-card:last-child .tool-status");
      if (last) { last.classList.remove("running"); last.classList.add("done"); last.textContent = "完成"; }
      // 检测记忆与当前实现是否有冲突
      delay(600, () => {
        $("#tool-cards").insertAdjacentHTML("beforeend", toolCard("detect_conflicts", "当前实现 vs 历史决策", "running"));
      });
      delay(1400, () => {
        const cLast = $("#tool-cards .tool-card:last-child .tool-status");
        if (cLast) { cLast.classList.remove("running"); cLast.classList.add("done"); cLast.textContent = "完成"; }
        searchArea.insertAdjacentHTML("beforeend", `
          <div style="margin-top: 8px; padding: 8px 12px; background: rgba(220,38,38,0.08); border: 1px solid rgba(220,38,38,0.3); border-radius: 6px; font-size: 12px; color: var(--danger); animation: msgSlideIn 0.3s;">
            检测到冲突：当前使用 bcrypt，历史决策为 Argon2
          </div>
        `);
      });
    });
  });
  // 10.5s: 切回单屏 TRAE，Agent 找到方案并修复
  delay(10500, () => {
    stage.innerHTML = traeWindow(`
      <div class="trae-layout" style="position: relative;">
        ${timeLabel("第五天")}
        <div class="trae-chat">
          ${traeChatHeader("登录鉴权问题", { timer: "00:21" })}
          <div class="trae-chat-body">
            ${traeAgentMsg(`<div style="font-size: 13px; line-height: 1.7;">找到了。第二天讨论的安全策略是：<strong>Argon2 哈希 + JWT 双 token</strong>。建议据此调整当前实现…</div>`)}
            ${traeUserMsg("好的，按这个方案修复")}
            ${traeAgentMsg(`
              <div class="trae-collapse-bar">思考过程</div>
              ${toolCard("Edit", "crates/server/src/middleware/auth.rs", "done")}
              <div class="trae-step" style="margin-top: 8px;"><span class="trae-step-check">✓</span>正在修改 auth.rs 中的密码哈希逻辑…</div>
            `)}
          </div>
          ${traeInputArea()}
        </div>
      </div>
    `);
  });
}

// 场景 9：周期合并机制（合并原19-20：第七天周合并 → 第三周进度）
function renderScene09(stage, scene) {
  const projects = [
    { name: "MemoryCenter", tasks: [{ title: "周总结", active: true }]},
  ];
  // Phase 1: 分屏，第七天周合并
  stage.innerHTML = `
    <div class="split-screen" style="position: relative;">
      ${timeLabel("第七天")}
      <div class="split-pane">${traeWindow(`
        <div class="trae-layout">
          <div class="trae-chat">
            ${traeChatHeader("周总结", { timer: "00:03" })}
            <div class="trae-chat-body" id="chat-body"></div>
            ${traeInputArea()}
          </div>
        </div>
      `)}</div>
      <div class="split-pane">
        <div class="split-pane-header"><svg width="14" height="14" viewBox="0 0 24 24" fill="none" style="color: var(--accent);"><circle cx="12" cy="12" r="3" fill="currentColor"/><circle cx="12" cy="12" r="8" stroke="currentColor" stroke-width="1.5"/></svg>MemoryCenter · 周合并</div>
        <div style="flex: 1; padding: 12px;">
          <div class="mc-dashboard" style="padding: 8px; gap: 8px; grid-template-rows: auto auto 1fr;">
            <div class="mc-header" style="padding-bottom: 4px; grid-column: 1/-1;"><div class="mc-title">周合并进行中</div><span class="mc-status-badge active">处理中</span></div>
            <div class="mc-timeline" style="grid-column: 1/-1;">
              <div class="mc-section-title">时间轴</div>
              <div class="mc-timeline-track">
                <div class="timeline-tier"><div class="timeline-tier-label">日</div><div class="timeline-tier-nodes" id="day-nodes"></div></div>
                <div class="timeline-tier"><div class="timeline-tier-label">周</div><div class="timeline-tier-nodes" id="week-nodes"></div></div>
                <div class="timeline-tier"><div class="timeline-tier-label">月</div><div class="timeline-tier-nodes"><div class="timeline-tier-node"></div></div></div>
              </div>
            </div>
            <div class="mc-log-stream" id="mc-log" style="grid-column: 1/-1;"></div>
          </div>
        </div>
      </div>
    </div>
  `;
  const dayNodes = $("#day-nodes");
  for (let i = 0; i < 7; i++) {
    dayNodes.insertAdjacentHTML("beforeend", `<div class="timeline-tier-node ${i < 6 ? 'active' : 'processing'}"></div>`);
  }
  delay(500, () => { $("#week-nodes").innerHTML = `<div class="timeline-tier-node processing"></div>`; });
  delay(1000, () => { $("#week-nodes").innerHTML = `<div class="timeline-tier-node active"></div>`; });
  const mcLog = $("#mc-log");
  const logs = [
    { time: "00:00", action: "compaction", target: 'period="weekly"', d: 300 },
    { time: "00:01", action: "merge", target: "7 日钩子 → 1 周钩子", d: 1200 },
    { time: "00:02", action: "merge", target: "7 日记忆 → 1 周记忆", d: 2200 },
    { time: "00:03", action: "done", target: "低价值记忆已淘汰", d: 3200 },
  ];
  logs.forEach(l => {
    delay(l.d, () => {
      mcLog.insertAdjacentHTML("beforeend", `<div class="log-line"><span class="log-time">${l.time}</span><span class="log-action">${l.action}</span> → <span class="log-target">${l.target}</span></div>`);
      mcLog.scrollTop = mcLog.scrollHeight;
    });
  });
  // Agent 周合并完成回复
  delay(3300, () => {
    const chatBody = $("#chat-body");
    if (chatBody) {
      chatBody.insertAdjacentHTML("beforeend", traeAgentMsg(`<div style="font-size: 13px;">本周记忆合并完成：日钩子 7 → 周钩子 1，日记忆 7 → 周记忆 1。低价值记忆已淘汰。</div>`));
    }
  });
  // Phase 2: 过渡到第三周（全屏 MC 仪表盘）
  delay(3500, () => {
    stage.innerHTML = `
      <div class="mc-dashboard" style="height: 100%;">
        <div class="mc-header">
          <div class="mc-title">MemoryCenter 仪表盘</div>
          <span class="mc-status-badge active">第三周合并中</span>
        </div>
        <div class="mc-timeline">
          <div class="mc-section-title">时间轴（3 周进度）</div>
          <div class="mc-timeline-track">
            <div class="timeline-tier">
              <div class="timeline-tier-label">日</div>
              <div class="timeline-tier-nodes" id="day-nodes-2"></div>
            </div>
            <div class="timeline-tier">
              <div class="timeline-tier-label">周</div>
              <div class="timeline-tier-nodes">
                <div class="timeline-tier-node active"></div>
                <div class="timeline-tier-node active"></div>
                <div class="timeline-tier-node processing"></div>
              </div>
            </div>
            <div class="timeline-tier">
              <div class="timeline-tier-label">月</div>
              <div class="timeline-tier-nodes"><div class="timeline-tier-node"></div></div>
            </div>
          </div>
        </div>
        <div class="mc-tagcloud">
          <div class="mc-section-title" style="width: 100%;">已合并统计</div>
          <span class="tag-chip">周钩子文件<span class="tag-count">2</span></span>
          <span class="tag-chip">周记忆文件<span class="tag-count">2</span></span>
          <span class="tag-chip">处理中<span class="tag-count">第3周</span></span>
        </div>
        <div class="mc-log-stream" id="mc-log-2"></div>
      </div>
    `;
    const dayNodes2 = $("#day-nodes-2");
    for (let i = 0; i < 21; i++) {
      dayNodes2.insertAdjacentHTML("beforeend", `<div class="timeline-tier-node ${i < 14 ? 'active' : 'processing'}"></div>`);
    }
    delay(300, () => {
      $("#mc-log-2").insertAdjacentHTML("beforeend", `<div class="log-line"><span class="log-time">--:--</span><span class="log-action">compaction</span> → <span class="log-target">第3周合并中…</span></div>`);
    });
  });
}

// 场景 10：跨 Agent 记忆迁移（合并原21-24：TRAE+OpenCode → OpenCode 检索 → TRAE 召回钩子表）
function renderScene10(stage, scene) {
  const traeProjects = [
    { name: "MemoryCenter", tasks: [{ title: "项目迁移", active: true }]},
  ];
  const question = "这个项目要迁移到 TRAE，用不用做什么操作？";
  // Phase 1: 分屏 TRAE + OpenCode 主界面（0-3s）
  stage.innerHTML = `
    <div class="split-screen" style="position: relative;">
      ${timeLabel("第四周 第一天")}
      <div class="split-pane">${traeWindow(`
        <div class="trae-layout">
          <div class="trae-chat">
            ${traeChatHeader("项目迁移", { timer: "00:05" })}
            <div class="trae-chat-body">
              ${traeUserMsg(`<span id="user-msg"></span>`)}
            </div>
            ${traeInputArea()}
          </div>
        </div>
      `)}</div>
      <div class="split-pane">${opencodeWindow(`
        <div class="opencode-app">
          ${opencodeTopbarHtml({ title: "新建会话", tabs: [{ name: "NovaCraft" }], leftIcon: "grid" })}
          ${opencodeHomeHtml()}
        </div>
      `, { title: "OpenCode" })}</div>
    </div>
  `;
  typewriter($("#user-msg"), "突然想起来 OpenCode 那边有个项目要迁移到 TRAE，但怕细节丢失…", 30);
  // Phase 2: 切换到 OpenCode 聊天界面（3-8.5s：空聊天+水印 → 打字 → 思考 → 回复）
  delay(3000, () => {
    stage.innerHTML = opencodeWindow(`
      <div class="opencode-app">
        ${opencodeTopbarHtml({ title: "新建会话", tabs: [{ name: "NovaCraft", active: true }], showNewSession: true })}
        <div class="oc-body">
          <div class="oc-chat-view">
            ${opencodeChatHeader("NovaCraft")}
            <div class="oc-chat-body" id="chat-body">
              ${opencodeWatermarkHtml()}
            </div>
            ${opencodeInputArea({ typing: true, textId: "oc-input-text" })}
          </div>
        </div>
      </div>
    `, { title: "OpenCode" });
    // 3.4s: 开始打字
    delay(400, () => {
      const ocInput = $("#oc-input-text");
      if (ocInput) {
        typewriter(ocInput, question, 30, () => {
          // ~4.5s: 打字完成，显示用户消息 + Agent 思考
          delay(300, () => {
            const chatBody = $("#chat-body");
            chatBody.innerHTML = `${opencodeUserMsg(question)}${opencodeThinkingHtml([], { id: "thinking-area" })}${opencodeAgentReply("", { id: "reply-area" })}`;
            const thinking = $("#thinking-area");
            delay(200, () => {
              thinking.insertAdjacentHTML("beforeend", `<p>用户询问跨 Agent 迁移是否需要额外操作。我需要先检索该项目的 MemoryCenter 钩子表，确认已有哪些归档记忆。</p>`);
            });
            delay(1000, () => {
              thinking.insertAdjacentHTML("beforeend", `<div class="oc-tool-call"><span class="oc-code">retrieve</span><span class="oc-arg">hook_id="项目钩子表"</span> ✓</div>`);
            });
            delay(2000, () => {
              thinking.insertAdjacentHTML("beforeend", `<p>钩子表显示该项目在 OpenCode 中有完整开发历史。接下来用语义搜索确认跨 Agent 迁移的约定。</p>`);
            });
            delay(2800, () => {
              thinking.insertAdjacentHTML("beforeend", `<div class="oc-tool-call"><span class="oc-code">semantic_search</span><span class="oc-arg">query="跨 Agent 迁移" top_k=5</span> ✓</div>`);
            });
            delay(3800, () => {
              $("#reply-area").innerHTML = `不用做额外操作。因为两个项目都安装了 MemoryCenter，且使用同一 <span class="oc-code">session_id</span> 命名空间。<br>直接在 TRAE 新会话中打开项目文件夹，输入：<br><strong style="color: var(--oc-accent);">"请阅读本项目的 MemoryCenter 钩子表"</strong> 发送即可。`;
            });
          });
        });
      }
    });
  });
  // Phase 3: 切换到 TRAE 聊天（8.5-14s）
  delay(8500, () => {
    const projects = [
      { name: "MemoryCenter", tasks: [{ title: "项目迁移到 TRAE", active: true }]},
      { name: "NovaCraft", tasks: [
        { title: "产品介绍页" },
        { title: "用户反馈收集" },
      ]},
    ];
    stage.innerHTML = traeWindow(`
      <div class="trae-layout">
        ${traeSidebarHtml(projects)}
        <div class="trae-chat">
          ${traeChatHeader("项目迁移到 TRAE", { timer: "00:12" })}
          <div class="trae-chat-body">
            ${traeUserMsg("请阅读本项目的 MemoryCenter 钩子表")}
            ${traeAgentMsg(`
              <div class="trae-collapse-bar">思考过程</div>
              <div id="tool-cards"></div>
            `)}
          </div>
          ${traeInputArea()}
        </div>
      </div>
    `);
    const toolCards = $("#tool-cards");
    const tools = [
      { name: "prompt", args: 'session_id="cross-agent-demo"', d: 300 },
      { name: "summaries", args: "获取所有周期摘要", d: 1200 },
      { name: "retrieve", args: "逐条读取钩子", d: 2100 },
    ];
    tools.forEach(t => {
      delay(t.d, () => {
        toolCards.insertAdjacentHTML("beforeend", toolCard(t.name, t.args, "running"));
        delay(700, () => {
          const last = toolCards.querySelector(".tool-card:last-child .tool-status");
          if (last) { last.classList.remove("running"); last.classList.add("done"); last.textContent = "完成"; }
        });
      });
    });
    delay(3200, () => {
      toolCards.insertAdjacentHTML("beforeend", `<div class="trae-step" style="margin-top: 8px;"><span class="trae-step-check">✓</span>已召回 MemoryCenter 钩子表，共 N 条记忆：<br>· 项目背景：XXX（来自 OpenCode, 第二天）<br>· 架构决策：XXX（来自 OpenCode, 第三天）<br>· 已实现模块：XXX<br>我已掌握该项目的完整开发周期与信息。</div>`);
    });
  });
}

// 场景 11：记忆标签体系（合并原25-26：17 类标签全景 → 跨 Agent 决策延续流程图）
function renderScene11(stage, scene) {
  const tags = [
    { name: "task_state", count: 12, time: "2h前" },
    { name: "decisions", count: 8, time: "1h前" },
    { name: "risks", count: 5, time: "3h前" },
    { name: "progress", count: 15, time: "刚刚" },
    { name: "architecture", count: 6, time: "昨天" },
    { name: "data_model", count: 4, time: "昨天" },
    { name: "api_contract", count: 7, time: "2天前" },
    { name: "dependencies", count: 3, time: "3天前" },
    { name: "deployment", count: 2, time: "上周" },
    { name: "testing", count: 9, time: "今天" },
    { name: "performance", count: 4, time: "2天前" },
    { name: "security", count: 6, time: "今天" },
    { name: "ux_decisions", count: 3, time: "昨天" },
    { name: "known_issues", count: 7, time: "1h前" },
    { name: "lessons_learned", count: 5, time: "上周" },
    { name: "next_steps", count: 11, time: "刚刚" },
    { name: "glossary", count: 8, time: "3天前" },
  ];
  // Phase 1: 17 类标签全景
  stage.innerHTML = `
    <div class="tags-showcase">
      <div class="tags-showcase-title">17 类语义标签 · 钩子分类统计</div>
      <div class="tags-grid" id="tags-grid"></div>
    </div>
  `;
  const grid = $("#tags-grid");
  tags.forEach((t, i) => {
    delay(i * 150, () => {
      grid.insertAdjacentHTML("beforeend", `
        <div class="tag-card">
          <div class="tag-card-name">${t.name}</div>
          <div class="tag-card-count">${t.count}</div>
          <div class="tag-card-time">${t.time}</div>
        </div>
      `);
    });
  });
  // Phase 2: 过渡到跨 Agent 决策延续流程图
  delay(4500, () => {
    stage.innerHTML = `
      <div class="decision-flow">
        <div class="flow-pane">
          <div class="flow-pane-title"><span class="agent-badge opencode">OpenCode</span> 归档的决策摘要</div>
          <div class="flow-item"><strong>架构决策</strong>：采用 Axum + SQLite<br><span style="color: var(--text-mute); font-size: 10px;">第二天 14:32 归档</span></div>
          <div class="flow-item"><strong>数据模型</strong>：users 表 + sessions 表<br><span style="color: var(--text-mute); font-size: 10px;">第三天 10:15 归档</span></div>
          <div class="flow-item"><strong>API 契约</strong>：/api/v1/auth/*<br><span style="color: var(--text-mute); font-size: 10px;">第三天 16:40 归档</span></div>
        </div>
        <div class="flow-connector">
          <div class="flow-line"></div>
          <div class="flow-arrow"><svg width="12" height="12" viewBox="0 0 24 24" fill="none"><path d="M9 6l6 6-6 6" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg></div>
        </div>
        <div class="flow-pane">
          <div class="flow-pane-title"><span class="agent-badge trae">TRAE</span> 当前决策延续</div>
          <div class="flow-item"><strong>已落地</strong>：Axum 路由已实现<br><span style="color: var(--accent); font-size: 10px;">基于 OpenCode 决策</span></div>
          <div class="flow-item"><strong>已落地</strong>：migration 已创建<br><span style="color: var(--accent); font-size: 10px;">基于 OpenCode 数据模型</span></div>
          <div class="flow-item"><strong>实现中</strong>：auth handler 编码<br><span style="color: var(--accent); font-size: 10px;">基于 OpenCode API 契约</span></div>
        </div>
      </div>
    `;
  });
}

// 场景 12：阈值压缩演示（原27：水位条 30%→60%→80%→pre_compress_hook→30%）
function renderScene12(stage, scene) {
  const projects = [
    { name: "MemoryCenter", tasks: [{ title: "长会话进行中", active: true }] },
  ];
  const rightPanel = traeRightSidebarHtml({
    todos: [
      { text: "实现归档逻辑", done: true },
      { text: "阈值压缩处理", done: false },
      { text: "压缩后记忆召回", done: false },
    ],
    contextPercent: 30,
    files: [
      { name: "archive.rs", iconColor: "#dea584" },
      { name: "compress.rs", iconColor: "#dea584" },
      { name: "context.rs", iconColor: "#dea584" },
    ],
  });
  stage.innerHTML = traeWindow(`
    <div class="trae-layout" style="position: relative;">
      ${traeSidebarHtml(projects)}
      <div class="trae-chat">
        ${traeChatHeader("长会话进行中", { timer: "08:23" })}
        <div class="trae-chat-body" id="chat-body"></div>
        ${traeInputArea()}
      </div>
      ${rightPanel}
    </div>
  `);
  const fill = $("#ctx-fill");
  const label = $("#ctx-percent");
  const chatBody = $("#chat-body");
  delay(500, () => { fill.style.width = "60%"; fill.classList.add("warning"); label.textContent = "60%"; });
  delay(1500, () => { fill.style.width = "80%"; fill.classList.remove("warning"); fill.classList.add("danger"); label.textContent = "80% 接近阈值"; });
  delay(2000, () => {
    chatBody.insertAdjacentHTML("beforeend", traeAgentMsg(`<div style="font-size: 13px;">检测到上下文接近阈值（80%），正在归档完整原始会话到 MemoryCenter…</div>`));
  });
  delay(2400, () => {
    chatBody.insertAdjacentHTML("beforeend", traeAgentMsg(`<div id="tool-cards-27"></div>`));
  });
  delay(2700, () => {
    const tc = $("#tool-cards-27");
    if (tc) tc.insertAdjacentHTML("beforeend", toolCard("pre_compress_hook", "raw_context + 解析 turns 双轨处理", "running"));
  });
  delay(3700, () => {
    const last = $("#tool-cards-27 .tool-card:last-child .tool-status");
    if (last) { last.classList.remove("running"); last.classList.add("done"); last.textContent = "完成"; }
  });
  delay(4500, () => {
    fill.style.width = "30%"; fill.classList.remove("danger"); label.textContent = "30%";
    chatBody.insertAdjacentHTML("beforeend", traeAgentMsg(`<div style="font-size: 13px;">归档完成，继续工作。完整上下文已备份到 MemoryCenter。</div>`));
  });
  // 压缩后 prompt 召回
  delay(5200, () => {
    chatBody.insertAdjacentHTML("beforeend", traeAgentMsg(`
      <div class="trae-collapse-bar">思考过程</div>
      <div id="post-compress-tools"></div>
    `));
    const tc = $("#post-compress-tools");
    tc.insertAdjacentHTML("beforeend", toolCard("prompt", "session_id=trae-mc-20260713", "running"));
    delay(600, () => {
      const last = tc.querySelector(".tool-card:last-child .tool-status");
      if (last) { last.classList.remove("running"); last.classList.add("done"); last.textContent = "完成"; }
    });
  });
  delay(6200, () => {
    chatBody.insertAdjacentHTML("beforeend", traeAgentMsg(`<div style="font-size: 13px;">已从 MemoryCenter 召回历史记忆，上下文已校准。继续工作无需重复提问。</div>`));
  });
}

// 场景 13：开发者能力（合并原28-30：月级评分淘汰 → 7 语言绑定 → MCP 21 工具列表）
function renderScene13(stage, scene) {
  // Phase 1: 月级 4 维评分淘汰
  stage.innerHTML = `
    <div class="score-scene" style="position: relative;">
      ${timeLabel("第一个月末")}
      <div class="score-header">月级合并 · 4 维加权评分</div>
      <div style="font-size: 12px; color: var(--text-mute);">评分维度：时序近因 · 访问频次 · 语义重要性 · 标签匹配度</div>
      <div class="score-list" id="score-list"></div>
    </div>
  `;
  const items = [
    { name: "项目架构决策", detail: "时序 0.9 · 频次 0.8 · 语义 0.95 · 标签 0.9", score: "巩固", d: 300 },
    { name: "用户鉴权方案", detail: "时序 0.7 · 频次 0.9 · 语义 0.9 · 标签 0.85", score: "巩固", d: 700 },
    { name: "早期调试记录", detail: "时序 0.2 · 频次 0.3 · 语义 0.4 · 标签 0.3", score: "淘汰", d: 1100 },
    { name: "数据库迁移脚本", detail: "时序 0.6 · 频次 0.7 · 语义 0.8 · 标签 0.8", score: "巩固", d: 1500 },
    { name: "临时测试输出", detail: "时序 0.1 · 频次 0.1 · 语义 0.2 · 标签 0.2", score: "淘汰", d: 1900 },
    { name: "部署配置", detail: "时序 0.8 · 频次 0.6 · 语义 0.85 · 标签 0.9", score: "巩固", d: 2300 },
  ];
  const list = $("#score-list");
  items.forEach(it => {
    delay(it.d, () => {
      list.insertAdjacentHTML("beforeend", `
        <div class="score-item ${it.score === '巩固' ? 'consolidated' : 'deprecated'}">
          <svg class="score-radar" viewBox="0 0 32 32" fill="none">
            <polygon points="16,4 28,12 24,26 8,26 4,12" stroke="${it.score === '巩固' ? 'var(--accent)' : 'var(--danger)'}" stroke-width="1" fill="${it.score === '巩固' ? 'rgba(22,163,74,0.1)' : 'rgba(220,38,38,0.1)'}"/>
          </svg>
          <div class="score-text">
            <div class="score-name">${it.name}</div>
            <div class="score-detail">${it.detail}</div>
          </div>
          <span class="score-tag ${it.score === '巩固' ? 'consolidated' : 'deprecated'}">${it.score}</span>
        </div>
      `);
    });
  });
  delay(2800, () => {
    list.insertAdjacentHTML("beforeend", `<div style="text-align: center; padding: 12px; font-size: 13px; color: var(--text);">巩固记忆 <strong style="color: var(--accent);">4</strong> 条 · 淘汰低价值记忆 <strong style="color: var(--danger);">2</strong> 条</div>`);
  });
  // Phase 2: 7 种语言绑定代码示例
  delay(4000, () => {
    const snippets = [
      { lang: "Rust", crate: "memory-center-core", code: `<span class="kw">let</span> mc = <span class="fn">MemoryCenter::new</span>(path);\nmc.<span class="fn">archive</span>(session_id, turns).<span class="kw">await</span>?;` },
      { lang: "Python", crate: "memory-center-python", code: `<span class="kw">from</span> memory_center <span class="kw">import</span> MemoryCenter\nmc = <span class="fn">MemoryCenter</span>(path)\nmc.<span class="fn">archive</span>(session_id, turns)` },
      { lang: "Node.js", crate: "memory-center-node", code: `<span class="kw">const</span> mc = <span class="kw">new</span> <span class="fn">MemoryCenter</span>(path);\n<span class="kw">await</span> mc.<span class="fn">archive</span>(sessionId, turns);` },
      { lang: "Java", crate: "memory-center-java", code: `<span class="fn">MemoryCenter</span> mc = <span class="kw">new</span> <span class="fn">MemoryCenter</span>(path);\nmc.<span class="fn">archive</span>(sessionId, turns);` },
      { lang: "Go", crate: "memory-center-go", code: `mc := memorycenter.<span class="fn">New</span>(path)\nmc.<span class="fn">Archive</span>(sessionID, turns)` },
      { lang: "C ABI", crate: "memory-center-ffi", code: `<span class="fn">memory_center_archive</span>(handle, session_id, turns_json);` },
      { lang: "WASM", crate: "memory-center-wasm", code: `<span class="com">// 浏览器内直接调用，零后端依赖</span>\n<span class="kw">const</span> mc = <span class="fn">MC_WASM.new</span>(path);` },
    ];
    stage.innerHTML = `
      <div class="code-editor">
        <div class="editor-titlebar">
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none"><path d="M8 9l3 3-3 3M13 15h3" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/><rect x="3" y="4" width="18" height="16" rx="2" stroke="currentColor" stroke-width="1.5"/></svg>
          多语言绑定 · 一行代码接入
        </div>
        <div class="editor-body" id="editor-body"></div>
      </div>
    `;
    const body = $("#editor-body");
    snippets.forEach((s, i) => {
      delay(i * 500, () => {
        body.insertAdjacentHTML("beforeend", `
          <div class="code-block">
            <div class="code-lang-label"><strong>${s.lang}</strong> <span style="color: #6c7086; font-family: monospace; font-size: 10px;">${s.crate}</span></div>
            <div class="code-content">${s.code}</div>
          </div>
        `);
      });
    });
  });
  // Phase 3: MCP 21 工具列表
  delay(8000, () => {
    const projects = [
      { name: "MemoryCenter", tasks: [{ title: "用 MC 辅助开发", active: true }] },
    ];
    stage.innerHTML = traeWindow(`
      <div class="trae-layout">
        ${traeSidebarHtml(projects)}
        <div class="trae-chat">
          ${traeChatHeader("用 MC 辅助开发", { timer: "01:15" })}
          <div class="trae-chat-body">
            ${traeAgentMsg(`<div style="font-size: 13px;">MemoryCenter 共提供 21 个 MCP 工具，覆盖归档、检索、治理、配置、批量、预设、概览七大类。</div>`)}
          </div>
          ${traeInputArea()}
        </div>
        <div class="trae-modal-overlay">
          <div class="trae-modal">
            <div class="trae-modal-top">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" style="color: var(--trae-text-dim);"><circle cx="12" cy="12" r="3" stroke="currentColor" stroke-width="1.8"/></svg>
              <span style="font-size: 14px; font-weight: 600;">设置</span>
              <span style="margin-left: auto; cursor: pointer; color: var(--trae-text-mute); font-size: 18px;">×</span>
            </div>
            <div class="trae-modal-body">
              <div class="trae-settings-menu">
                <div class="trae-settings-item">常规</div>
                <div class="trae-settings-item">编辑器</div>
                <div class="trae-settings-item active">MCP</div>
                <div class="trae-settings-item">关于</div>
              </div>
              <div class="trae-settings-content">
                <div class="trae-settings-title">MCP</div>
                <div class="trae-mcp-section-title">
                  <h3>MemoryCenter 工具列表（21 个）</h3>
                  <button class="trae-mcp-add-btn">+ 添加 MCP Server</button>
                </div>
                <div class="trae-mcp-desc">7 大类工具，覆盖 Agent 记忆全生命周期。</div>
                <div class="trae-mcp-list">
                  <div class="trae-mcp-item expanded">
                    <span class="trae-mcp-expand">▾</span>
                    <div class="trae-mcp-icon" style="background: #7c3aed;">M</div>
                    <span class="trae-mcp-name">MemoryCenter</span>
                    <span style="font-size: 11px; color: var(--trae-text-mute); padding: 2px 6px; background: var(--trae-card-bg); border-radius: 4px;">v2.37</span>
                    <span class="trae-mcp-check">✓</span>
                    <div class="trae-mcp-toggle"></div>
                    <div class="trae-mcp-tools" id="tool-list" style="max-height: 380px; overflow-y: auto;"></div>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    `);
    const toolList = $("#tool-list");
    const groups = [
      { group: "归档类", tools: [{ n: "prompt", d: "获取历史记忆" }, { n: "archive", d: "归档对话" }, { n: "pre_compress_hook", d: "压缩前归档" }] },
      { group: "检索类", tools: [{ n: "semantic_search", d: "语义检索" }, { n: "retrieve", d: "检索记忆" }, { n: "find_hook_by_prefix", d: "按前缀查找" }] },
      { group: "治理类", tools: [{ n: "compaction", d: "周期合并" }, { n: "detect_conflicts", d: "检测冲突" }, { n: "get_conflicts", d: "获取冲突" }] },
      { group: "配置类", tools: [{ n: "get_config", d: "查询配置" }, { n: "install_rules", d: "安装规则" }, { n: "update_project_memory", d: "更新项目记忆" }, { n: "get_project_memory", d: "读取项目记忆" }] },
      { group: "批量类", tools: [{ n: "batch_retrieve", d: "批量检索" }, { n: "batch_delete", d: "批量删除" }, { n: "batch_update", d: "批量更新" }] },
      { group: "预设类", tools: [{ n: "preset_build", d: "构建预设" }, { n: "preset_list_agents", d: "列出 Agent" }, { n: "preset_list_scenarios", d: "列出场景" }, { n: "preset_list_models", d: "列出型号" }] },
      { group: "概览类", tools: [{ n: "summaries", d: "周期摘要" }] },
    ];
    groups.forEach((g, i) => {
      delay(i * 400, () => {
        toolList.insertAdjacentHTML("beforeend", `
          <div style="margin-bottom: 8px; padding-top: 4px;">
            <div style="font-size: 10px; color: var(--trae-text-mute); text-transform: uppercase; margin-bottom: 4px; letter-spacing: 0.5px;">${g.group}</div>
            ${g.tools.map(t => `
              <div class="trae-mcp-tool-row">
                <span class="trae-mcp-tool-name">${t.n}</span>
                <span style="color: var(--trae-text-mute);">${t.d}</span>
              </div>
            `).join('')}
          </div>
        `);
      });
    });
  });
}

// 场景 14：收尾（原31：MemoryCenter 仪表盘全景）
function renderScene14(stage, scene) {
  stage.innerHTML = `
    <div class="mc-dashboard">
      <div class="mc-header">
        <div class="mc-title">MemoryCenter 记忆库仪表盘</div>
        <span class="mc-status-badge active">运行中</span>
      </div>
      <div class="mc-timeline">
        <div class="mc-section-title">时间轴</div>
        <div class="mc-timeline-track">
          <div class="timeline-tier"><div class="timeline-tier-label">日</div><div class="timeline-tier-nodes" id="day-nodes"></div></div>
          <div class="timeline-tier"><div class="timeline-tier-label">周</div><div class="timeline-tier-nodes"><div class="timeline-tier-node active"></div><div class="timeline-tier-node active"></div><div class="timeline-tier-node active"></div><div class="timeline-tier-node active"></div></div></div>
          <div class="timeline-tier"><div class="timeline-tier-label">月</div><div class="timeline-tier-nodes"><div class="timeline-tier-node active"></div></div></div>
        </div>
      </div>
      <div class="mc-tagcloud" id="mc-tags"></div>
      <div class="mc-session-list">
        <div class="mc-section-title">会话列表</div>
        <div class="mc-session-item"><span class="agent-badge trae">TRAE</span> trae-mc-20260713</div>
        <div class="mc-session-item"><span class="agent-badge opencode">OC</span> opencode-mc-20260715</div>
        <div class="mc-session-item"><span class="agent-badge trae">TRAE</span> trae-mc-20260720</div>
        <div class="mc-session-item"><span class="agent-badge opencode">OC</span> opencode-mc-20260722</div>
      </div>
      <div class="mc-status-card">
        <div class="status-icon"><svg width="16" height="16" viewBox="0 0 24 24" fill="none"><path d="M9 12l2 2 4-4M21 12a9 9 0 11-18 0 9 9 0 0118 0z" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg></div>
        <div class="status-text"><div class="status-label">当前状态</div><div class="status-value">全部正常运行</div></div>
      </div>
      <div class="mc-log-stream" style="grid-column: 1/-1;">
        <div class="log-line"><span class="log-time">--:--</span><span class="log-action">system</span> → <span class="log-target">MemoryCenter —— Agent 的时序记忆基础设施</span></div>
      </div>
    </div>
  `;
  const dayNodes = $("#day-nodes");
  for (let i = 0; i < 30; i++) {
    dayNodes.insertAdjacentHTML("beforeend", `<div class="timeline-tier-node ${i < 28 ? 'active' : 'processing'}"></div>`);
  }
  const mcTags = $("#mc-tags");
  const tags = ["task_state", "decisions", "risks", "architecture", "security", "testing", "progress", "deployment", "api_contract", "data_model"];
  tags.forEach((t, i) => {
    delay(i * 100, () => {
      mcTags.insertAdjacentHTML("beforeend", `<span class="tag-chip">${t}<span class="tag-count">${Math.floor(Math.random() * 15 + 1)}</span></span>`);
    });
  });
}

// ============================================================
// 启动
// ============================================================
document.addEventListener("DOMContentLoaded", init);
