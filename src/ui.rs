pub const INDEX: &str = r#"<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Popovic</title>
  <style>
    :root {
      color-scheme: light;
      --bg: #f5f7f8;
      --ink: #16201c;
      --muted: #5d6963;
      --line: #d7ded9;
      --panel: #ffffff;
      --accent: #1f7a5b;
      --warn: #b25f16;
      --bad: #a8323a;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      background: var(--bg);
      color: var(--ink);
      font: 14px/1.45 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }
    header {
      height: 56px;
      display: flex;
      align-items: center;
      justify-content: space-between;
      padding: 0 24px;
      border-bottom: 1px solid var(--line);
      background: var(--panel);
    }
    .brand { font-weight: 750; letter-spacing: 0; }
    nav { display: flex; gap: 8px; }
    nav button, .button {
      border: 1px solid var(--line);
      background: #fff;
      color: var(--ink);
      min-height: 34px;
      padding: 0 12px;
      border-radius: 6px;
      cursor: pointer;
      font: inherit;
    }
    nav button.active, .button.primary { background: var(--accent); border-color: var(--accent); color: #fff; }
    main { padding: 20px 24px 28px; }
    .tab { display: none; }
    .tab.active { display: block; }
    .grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 12px; }
    .wide { grid-column: span 3; }
    .panel {
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 8px;
      padding: 14px;
      min-height: 110px;
    }
    h1, h2, h3 { margin: 0; letter-spacing: 0; }
    h2 { font-size: 15px; margin-bottom: 10px; }
    .metric { font-size: 32px; font-weight: 760; }
    .muted { color: var(--muted); }
    table { width: 100%; border-collapse: collapse; }
    th, td { text-align: left; padding: 8px 6px; border-bottom: 1px solid var(--line); vertical-align: top; }
    th { color: var(--muted); font-weight: 650; }
    .alerts { display: grid; gap: 8px; }
    .alert { border-left: 4px solid var(--warn); background: #fff8ef; padding: 8px 10px; border-radius: 4px; }
    .alert.critical { border-left-color: var(--bad); background: #fff1f2; }
    canvas { width: 100%; height: 220px; display: block; }
    form { display: grid; gap: 10px; max-width: 920px; }
    label { display: grid; gap: 4px; font-weight: 650; }
    input, textarea, select {
      width: 100%;
      min-height: 36px;
      border: 1px solid var(--line);
      border-radius: 6px;
      padding: 8px 10px;
      font: inherit;
      background: #fff;
      color: var(--ink);
    }
    textarea { min-height: 140px; resize: vertical; }
    .row { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 10px; }
    .tasks { display: grid; gap: 12px; margin-top: 16px; }
    .task pre { white-space: pre-wrap; overflow: auto; max-height: 320px; background: #101614; color: #e6f1eb; padding: 10px; border-radius: 6px; }
    .actions { display: flex; gap: 8px; flex-wrap: wrap; }
    @media (max-width: 900px) {
      .grid { grid-template-columns: 1fr; }
      .wide { grid-column: span 1; }
      .row { grid-template-columns: 1fr; }
      header { height: auto; padding: 12px; align-items: flex-start; gap: 10px; flex-direction: column; }
    }
  </style>
</head>
<body>
  <header>
    <div>
      <div class="brand">Popovic</div>
      <div class="muted">Static HTTP deployment console</div>
    </div>
    <nav>
      <button class="active" data-tab="dashboard">Dashboard</button>
      <button data-tab="agent">AI Agent</button>
      <button data-tab="settings">Settings</button>
    </nav>
  </header>
  <main>
    <section id="dashboard" class="tab active">
      <div class="grid">
        <div class="panel"><h2>Running Apps</h2><div id="running-count" class="metric">0</div><div class="muted">static apps managed</div></div>
        <div class="panel"><h2>Tunnel Status</h2><div id="tunnel-status" class="metric">-</div><div id="tunnel-detail" class="muted"></div></div>
        <div class="panel"><h2>CPU Usage</h2><div id="cpu" class="metric">0%</div><div class="muted">host utilization</div></div>
        <div class="panel"><h2>RAM Usage</h2><div id="ram" class="metric">0%</div><div class="muted">host memory</div></div>
        <div class="panel wide"><h2>Alerts</h2><div id="alerts" class="alerts"></div></div>
        <div class="panel wide"><h2>USE / RED</h2><canvas id="chart" width="960" height="220"></canvas></div>
        <div class="panel wide"><h2>Apps</h2><table><thead><tr><th>Name</th><th>Domain</th><th>Status</th><th>Last deploy</th><th>Requests</th><th>Errors</th></tr></thead><tbody id="apps"></tbody></table></div>
      </div>
    </section>
    <section id="agent" class="tab">
      <div class="panel">
        <h2>AI Agent</h2>
        <form id="agent-form">
          <label>App <select id="agent-app" name="app_id"></select></label>
          <label>Request <textarea name="prompt" placeholder="Change the hero copy in index.html, update robots.txt, or suggest a Cloudflare/GitHub action."></textarea></label>
          <button class="button primary" type="submit">Propose Change</button>
        </form>
      </div>
      <div id="tasks" class="tasks"></div>
    </section>
    <section id="settings" class="tab">
      <div class="grid">
        <div class="panel wide">
          <h2>Register Static App</h2>
          <form id="app-form">
            <div class="row">
              <label>Name <input name="name" required placeholder="personal-site"></label>
              <label>Hostnames <input name="hostnames" placeholder="example.com,www.example.com"></label>
            </div>
            <label>GitHub repo URL or local path <input name="repo_url" required placeholder="https://github.com/user/site.git"></label>
            <label>Repo subdirectory <input name="repo_subdir" placeholder="public"></label>
            <button class="button primary" type="submit">Register and Deploy</button>
          </form>
        </div>
        <div class="panel wide">
          <h2>Credentials and MCP</h2>
          <form id="settings-form">
            <div class="row">
              <label>AI Provider <select name="ai_provider"><option value="openai">OpenAI</option><option value="anthropic">Anthropic</option><option value="antigravity">Antigravity</option><option value="cursor">Cursor</option></select></label>
              <label>AI API Key <input name="ai_api_key" type="password" placeholder="leave blank to keep existing"></label>
            </div>
            <div class="row">
              <label>GitHub OAuth Token <input name="github_oauth_token" type="password" placeholder="leave blank to keep existing"></label>
              <label>GitHub MCP URL <input name="github_mcp_url" placeholder="https://.../mcp"></label>
            </div>
            <label>Default GitHub Repo <input name="github_default_repo" placeholder="user/repo"></label>
            <div class="row">
              <label>Cloudflare API Token <input name="cloudflare_api_token" type="password" placeholder="leave blank to keep existing"></label>
              <label>Cloudflare MCP URL <input name="cloudflare_mcp_url" value="https://mcp.cloudflare.com/mcp"></label>
            </div>
            <div class="row">
              <label>Cloudflare Account ID <input name="cloudflare_account_id"></label>
              <label>Cloudflare Zone ID <input name="cloudflare_zone_id"></label>
            </div>
            <label>Cloudflare Tunnel ID <input name="cloudflare_tunnel_id"></label>
            <button class="button primary" type="submit">Save Settings</button>
          </form>
        </div>
      </div>
    </section>
  </main>
  <script>
    const tabs = document.querySelectorAll('nav button');
    tabs.forEach(button => button.addEventListener('click', () => {
      tabs.forEach(tab => tab.classList.remove('active'));
      document.querySelectorAll('.tab').forEach(tab => tab.classList.remove('active'));
      button.classList.add('active');
      document.getElementById(button.dataset.tab).classList.add('active');
    }));

    async function api(path, options) {
      const response = await fetch(path, options);
      if (!response.ok) throw new Error(await response.text());
      return response.json();
    }
    function formBody(form) { return new URLSearchParams(new FormData(form)); }
    function escapeHtml(value) {
      return String(value ?? '').replace(/[&<>"']/g, character => ({
        '&': '&amp;',
        '<': '&lt;',
        '>': '&gt;',
        '"': '&quot;',
        "'": '&#39;'
      })[character]);
    }
    async function refresh() {
      const status = await api('/api/status');
      document.getElementById('running-count').textContent = status.running_apps.length;
      document.getElementById('tunnel-status').textContent = status.tunnel_status.status;
      document.getElementById('tunnel-detail').textContent = `${status.tunnel_status.route_count} routes`;
      document.getElementById('cpu').textContent = `${status.cpu_percent.toFixed(0)}%`;
      document.getElementById('ram').textContent = `${status.ram_percent.toFixed(0)}%`;
      document.getElementById('alerts').innerHTML = status.alerts.length ? status.alerts.map(a => `<div class="alert ${a.severity === 'Critical' ? 'critical' : ''}"><strong>${escapeHtml(a.source)}</strong><br>${escapeHtml(a.message)}</div>`).join('') : '<div class="muted">No alerts</div>';
      document.getElementById('apps').innerHTML = status.running_apps.map(app => {
        const domains = app.domains.map(escapeHtml).join('<br>') || '-';
        return `<tr><td>${escapeHtml(app.name)}</td><td>${domains}</td><td>${escapeHtml(app.status)}</td><td>${escapeHtml(app.last_deploy)}</td><td>${app.requests_5m}</td><td>${app.errors_5m}</td></tr>`;
      }).join('');
      document.getElementById('agent-app').innerHTML = status.running_apps.map(app => `<option value="${escapeHtml(app.id)}">${escapeHtml(app.name)}</option>`).join('');
      drawChart(status.use_red);
      refreshTasks();
    }
    function drawChart(points) {
      const canvas = document.getElementById('chart');
      const ctx = canvas.getContext('2d');
      ctx.clearRect(0, 0, canvas.width, canvas.height);
      ctx.strokeStyle = '#d7ded9'; ctx.beginPath();
      for (let y = 20; y < 220; y += 40) { ctx.moveTo(0, y); ctx.lineTo(960, y); }
      ctx.stroke();
      drawLine(points.map(p => p.host.cpu_percent), '#1f7a5b');
      drawLine(points.map(p => p.host.ram_percent), '#2f5f9f');
      drawLine(points.map(p => Math.min(100, p.red.requests)), '#8c5a18');
      function drawLine(values, color) {
        if (!values.length) return;
        ctx.strokeStyle = color; ctx.lineWidth = 2; ctx.beginPath();
        values.forEach((value, index) => {
          const x = values.length === 1 ? 0 : (index / (values.length - 1)) * 940 + 10;
          const y = 205 - (Math.max(0, Math.min(100, value)) / 100) * 180;
          if (index === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
        });
        ctx.stroke();
      }
    }
    async function refreshTasks() {
      const tasks = await api('/api/tasks');
      document.getElementById('tasks').innerHTML = tasks.map(task => {
        const actions = task.status === 'Proposed'
          ? `<button class="button primary" onclick="approve('${escapeHtml(task.id)}')">Approve</button><button class="button" onclick="rejectTask('${escapeHtml(task.id)}')">Reject</button>`
          : '';
        return `<div class="panel task"><h2>${escapeHtml(task.status)}</h2><p>${escapeHtml(task.plan)}</p>${task.error ? `<p class="alert critical">${escapeHtml(task.error)}</p>` : ''}<pre>${escapeHtml(task.diff || 'No diff')}</pre><p><strong>GitHub:</strong> ${escapeHtml(task.github_action.summary)}</p><p><strong>Cloudflare:</strong> ${escapeHtml(task.cloudflare_action.summary)}</p><div class="actions">${actions}</div></div>`;
      }).join('');
    }
    async function approve(id) { await api(`/api/tasks/${id}/approve`, { method: 'POST' }); refresh(); }
    async function rejectTask(id) { await api(`/api/tasks/${id}/reject`, { method: 'POST' }); refresh(); }
    document.getElementById('settings-form').addEventListener('submit', async event => {
      event.preventDefault();
      await api('/api/settings', { method: 'POST', body: formBody(event.target) });
      event.target.ai_api_key.value = ''; event.target.github_oauth_token.value = ''; event.target.cloudflare_api_token.value = '';
      refresh();
    });
    document.getElementById('app-form').addEventListener('submit', async event => {
      event.preventDefault();
      await api('/api/apps', { method: 'POST', body: formBody(event.target) });
      refresh();
    });
    document.getElementById('agent-form').addEventListener('submit', async event => {
      event.preventDefault();
      await api('/api/tasks', { method: 'POST', body: formBody(event.target) });
      event.target.prompt.value = '';
      refresh();
    });
    refresh(); setInterval(refresh, 5000);
  </script>
</body>
</html>"#;
