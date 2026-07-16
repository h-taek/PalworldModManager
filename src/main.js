const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// ── Icon constants (ported from docs/mockups/m4-ui-mockup.html .row .ic) ──
const ICON_LUA = `<svg viewBox="0 0 24 24" fill="none" stroke="#f4f4f6" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M9.5 8 5.5 12l4 4"/><path d="M14.5 8l4 4-4 4"/></svg>`;
const ICON_PAK = `<svg viewBox="0 0 24 24" fill="none" stroke="#f4f4f6" stroke-width="1.6" stroke-linejoin="round"><path d="M12 2.6 20.5 7v10L12 21.4 3.5 17V7z"/><path d="M3.7 7 12 11.6 20.3 7"/><path d="M12 11.6v9.8"/></svg>`;
const ICON_GITHUB = `<svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true"><path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82A7.6 7.6 0 0 1 8 3.86c.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8Z"/></svg>`;
const PALWORLD_WALLPAPER_VIDEO_SOURCES = [
  "/assets/Palworld_1_upscaling_24fps.mp4",
  "/assets/Palworld_2_upscaling_24fps.mp4",
  "/assets/Palworld_3_upscaling_24fps.mp4",
  "/assets/Palworld_4_24fps.mp4",
  "/assets/Palworld_5_24fps.mp4",
];
const PALWORLD_WALLPAPER_VIDEO_SRC = PALWORLD_WALLPAPER_VIDEO_SOURCES[Math.floor(Math.random() * PALWORLD_WALLPAPER_VIDEO_SOURCES.length)];
const PALWORLD_WALLPAPER_CROSSFADE_SECONDS = 0.85;
const REPO_URL = "https://github.com/h-taek/PalworldModManager";
const MOD_REPOSITORY_URL = "https://github.com/h-taek/PalworldMod";
const PLAY_ICON = `<svg viewBox="0 0 24 24" fill="currentColor"><path d="M8 5v14l11-7z"/></svg>`;
const PAUSE_ICON = `<svg viewBox="0 0 24 24" fill="currentColor"><path d="M7 5h4v14H7zM13 5h4v14h-4z"/></svg>`;
const STOP_ICON = `<svg viewBox="0 0 24 24" fill="currentColor"><path d="M7 7h10v10H7z"/></svg>`;

// ── HTML escape helper ──
function esc(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

const views = document.querySelectorAll(".view");
const rail = document.querySelectorAll(".rbtn");
let current = "play";
let gameProcess = { pid: null, mode: "idle", pollTimer: 0, forceTimer: 0 };
let gameDetected = false;

async function setView(v) {
  current = v;
  views.forEach((s) => s.classList.toggle("on", s.dataset.view === v));
  rail.forEach((b) => b.classList.toggle("on", b.dataset.view === v));
  // 업데이트 팝업 스택은 모드 뷰에서만 노출
  const stack = document.querySelector("#update-stack");
  if (stack) stack.hidden = v !== "mods";
  if (v === "play") return renderPlay();
  else if (v === "mods") return renderMods();
  else if (v === "settings") return renderSettings();
}
rail.forEach((b) => b.addEventListener("click", () => setView(b.dataset.view)));

function setupPalworldWallpaperVideo(playEl) {
  const videos = Array.from(playEl.querySelectorAll(".Palworld_wallpaper"));
  if (videos.length < 2) return;
  let activeIndex = 0;
  let crossfading = false;
  let raf = 0;

  const current = () => videos[activeIndex];
  const standby = () => videos[1 - activeIndex];
  videos.forEach((video) => {
    video.style.setProperty("--Palworld_wallpaper_fade_duration", `${PALWORLD_WALLPAPER_CROSSFADE_SECONDS}s`);
    video.addEventListener("ended", () => {
      if (!crossfading) startCrossfade();
    });
  });

  function finishCrossfade(from, to, fade) {
    window.setTimeout(() => {
      if (!playEl.isConnected) return;
      from.pause();
      try { from.currentTime = 0; } catch {}
      from.classList.remove("is-active", "is-fading-in");
      to.classList.remove("is-fading-in");
      to.classList.add("is-active");
      activeIndex = videos.indexOf(to);
      crossfading = false;
    }, fade * 1000);
  }

  function startCrossfade() {
    if (crossfading) return;
    const from = current();
    const to = standby();
    const duration = Number.isFinite(from.duration) ? from.duration : 0;
    if (duration <= 0) return;
    const fade = Math.min(PALWORLD_WALLPAPER_CROSSFADE_SECONDS, duration / 4);

    crossfading = true;
    to.style.setProperty("--Palworld_wallpaper_fade_duration", `${fade}s`);
    from.style.setProperty("--Palworld_wallpaper_fade_duration", `${fade}s`);
    try { to.currentTime = 0; } catch {}
    to.play().catch(() => {});
    to.classList.remove("is-active", "is-fading-in");
    void to.offsetWidth;
    to.classList.add("is-fading-in");
    finishCrossfade(from, to, fade);
  }

  const tick = () => {
    if (!playEl.isConnected) return;
    const video = current();
    const duration = Number.isFinite(video.duration) ? video.duration : 0;
    if (!crossfading && duration > 0) {
      const fade = Math.min(PALWORLD_WALLPAPER_CROSSFADE_SECONDS, duration / 4);
      if (duration - video.currentTime <= fade) startCrossfade();
    }
    raf = requestAnimationFrame(tick);
  };
  const start = () => {
    cancelAnimationFrame(raf);
    tick();
  };
  current().addEventListener("loadedmetadata", start, { once: true });
  current().addEventListener("play", start, { once: true });
  if (current().readyState >= 1) start();
}

async function renderPlay() {
  const el = document.querySelector("#view-play");
  let detected = false;
  try { const r = await invoke("detect_game"); detected = !!(r && r.game_installed); } catch {}
  gameDetected = detected;
  const canUsePlayButton = detected || !!gameProcess.pid;
  el.innerHTML = `
    <div class="play">
      <video class="Palworld_wallpaper is-active" autoplay muted playsinline preload="auto" src="${esc(PALWORLD_WALLPAPER_VIDEO_SRC)}"></video>
      <video class="Palworld_wallpaper" muted playsinline preload="auto" src="${esc(PALWORLD_WALLPAPER_VIDEO_SRC)}"></video>
      <div class="scrim"></div>
      <button class="playbtn" id="play-btn" ${canUsePlayButton ? "" : "disabled title='Game not detected'"}></button>
      <span id="staging-status" hidden></span>
    </div>`;
  setupPalworldWallpaperVideo(el.querySelector(".play"));
  const btn = el.querySelector("#play-btn");
  updatePlayButton();
  if (canUsePlayButton) btn.addEventListener("click", () => handlePlayButton(detected));
}

function playButtonContent() {
  if (gameProcess.mode === "running") return `${PAUSE_ICON} STOP`;
  if (gameProcess.mode === "stopping") return `${PAUSE_ICON} ing..`;
  if (gameProcess.mode === "force") return `${STOP_ICON} FORCE`;
  return `${PLAY_ICON} PLAY`;
}

function updatePlayButton() {
  const btn = document.querySelector("#play-btn");
  if (!btn) return;
  btn.innerHTML = playButtonContent();
  btn.classList.toggle("force", gameProcess.mode === "force");
  btn.disabled = (!gameDetected && !gameProcess.pid) || gameProcess.mode === "launching" || gameProcess.mode === "stopping";
}

function clearGameProcessTimers() {
  if (gameProcess.pollTimer) window.clearInterval(gameProcess.pollTimer);
  if (gameProcess.forceTimer) window.clearTimeout(gameProcess.forceTimer);
  gameProcess.pollTimer = 0;
  gameProcess.forceTimer = 0;
}

function resetGameProcess() {
  clearGameProcessTimers();
  gameProcess.pid = null;
  gameProcess.mode = "idle";
  updatePlayButton();
}

function startGameProcessPolling() {
  if (gameProcess.pollTimer) window.clearInterval(gameProcess.pollTimer);
  gameProcess.pollTimer = window.setInterval(async () => {
    if (!gameProcess.pid) return resetGameProcess();
    try {
      const running = await invoke("is_game_process_running", { pid: gameProcess.pid });
      if (!running) resetGameProcess();
    } catch {
      resetGameProcess();
    }
  }, 1000);
}

function startForceStopTimer() {
  if (gameProcess.forceTimer) window.clearTimeout(gameProcess.forceTimer);
  gameProcess.forceTimer = window.setTimeout(async () => {
    if (!gameProcess.pid) return;
    try {
      const running = await invoke("is_game_process_running", { pid: gameProcess.pid });
      if (running) {
        gameProcess.mode = "force";
        updatePlayButton();
      } else {
        resetGameProcess();
      }
    } catch {
      resetGameProcess();
    }
  }, 10000);
}

async function handlePlayButton(detected) {
  if (!gameProcess.pid) {
    if (!detected) return;
    gameProcess.mode = "launching";
    updatePlayButton();
    try {
      gameProcess.pid = await invoke("launch_game");
      gameProcess.mode = "running";
      startGameProcessPolling();
    } catch (e) {
      resetGameProcess();
      const _ss = document.getElementById("staging-status");
      if (_ss) _ss.hidden = true;
      toast("err", String(e));
      return;
    }
    updatePlayButton();
    return;
  }

  if (gameProcess.mode === "force") {
    try { await invoke("force_stop_game", { pid: gameProcess.pid }); resetGameProcess(); }
    catch (e) { toast("err", String(e)); }
    return;
  }

  if (gameProcess.mode === "running") {
    gameProcess.mode = "stopping";
    updatePlayButton();
    try { await invoke("stop_game", { pid: gameProcess.pid }); }
    catch (e) { toast("err", String(e)); }
    startForceStopTimer();
  }
}

// ── Mods state ──
let modsState = { mods: [], updates: {}, query: "", sort: "recent", show: "all" };
let profiles = [], activeProfile = null;

async function loadMods() {
  try { modsState.mods = await invoke("list_mods"); }
  catch (e) { toast("err", String(e)); modsState.mods = []; }
}

function filteredMods() {
  const q = modsState.query.trim().toLowerCase();
  return modsState.mods
    .filter((m) => modsState.show === "all" ? true : modsState.show === "enabled" ? m.enabled : !m.enabled)
    .filter((m) => !q || m.name.toLowerCase().includes(q) || m.id.toLowerCase().includes(q))
    .slice()
    .sort((a, b) => (modsState.sort === "name" ? a.name.localeCompare(b.name) : 0));
}

function kindLabel(t) { return t === "lua" ? "UE4SS" : t === "pak" ? "PAK" : t === "hybrid" ? "HYBRID" : "?"; }

async function openExternal(url) {
  try { await invoke("plugin:opener|open_url", { url }); }
  catch (e) { toast("err", String(e)); }
}

function rowHtml(m) {
  const sw = `<label class="sw ${m.enabled ? "on" : ""} ${m.deployable ? "" : "dis"}">
      <input type="checkbox" data-tog="${esc(m.id)}" ${m.enabled ? "checked" : ""} ${m.deployable ? "" : "disabled"} hidden>
      <span class="t"></span></label>`;
  const src = m.status ? `${m.id} · ${m.status}` : m.id;
  return `<div class="row ${m.enabled ? "" : "off"}">
    <div class="ic">${m.mod_type === "lua" ? ICON_LUA : ICON_PAK}</div>
    <div class="body"><div class="name">${esc(m.name)}</div><div class="src">${esc(src)}</div></div>
    <div class="cluster"><span class="kind ${m.mod_type === "lua" ? "" : "pak"}">${kindLabel(m.mod_type)}</span>${sw}<button class="rm" data-rm="${esc(m.id)}">✕</button></div>
  </div>`;
}

// ── Sidebar: profile dropdown + Show filter (Task 11) ──
async function renderSidebar() {
  try { profiles = await invoke("list_profiles"); activeProfile = profiles.find((p) => p.active) || profiles[0] || null; }
  catch (e) { toast("err", String(e)); profiles = []; }
  let det = {};
  try { det = await invoke("detect_game"); } catch {}
  const found = !!(det && det.game_installed);
  const containerOk = !!(det && det.container_exists);
  const updateStatuses = Object.values(modsState.updates);
  const checkedUpdates = updateStatuses.length > 0;
  const availableUpdates = updateStatuses.filter((u) => u && u.has_update).length;
  const updateLabel = checkedUpdates ? `${availableUpdates} update${availableUpdates === 1 ? "" : "s"}` : "Not checked";
  const side = document.querySelector("#mods-side");
  if (!side) return;
  side.innerHTML = `
    <div class="ssection">
      <div class="slabel">Game</div>
      <button class="sideitem" id="side-redetect">
        ${found
          ? `<span class="si-text"><span class="si-title">Palworld</span><span class="si-sub">${containerOk ? "Installed · UE4SS ready" : "Installed · run once for container"}</span></span>`
          : `<span>Not Found Game</span>`}
      </button>
    </div>

    <div class="ssection">
      <div class="slabel">Profile</div>
      <div class="profile-dd">
        <button class="select" id="pf-select" aria-haspopup="listbox" aria-expanded="false">
          <span class="v">${esc(activeProfile?.name ?? "Default")}</span><span class="chev">▾</span>
        </button>
        <div class="pmenu" id="pf-menu" role="listbox" hidden>${profiles.map((p) => `<button class="pmi ${p.active ? "on" : ""}" data-pf="${p.id}" role="option" aria-selected="${p.active ? "true" : "false"}"><span class="pn">${esc(p.name)}</span><span class="pc">${p.mod_count} on</span></button>`).join("")}</div>
      </div>
      <div class="actrow profile-actions">
        <button class="ab add" id="pf-new"><span class="plus">＋</span>New</button>
        <button class="ab" id="pf-rename">Rename</button>
        <button class="ab danger" id="pf-del" ${profiles.length <= 1 ? "disabled" : ""}>Delete</button>
      </div>
      <div class="actrow profile-secondary">
        <button class="ab" id="pf-dup">⧉ Dup</button>
        <button class="ab" id="side-import"><span class="plus">＋</span>Import</button>
      </div>
      <p class="import-hint">다운로드한 폴더 구조 그대로 가져오세요. <code>LogicMods</code>·<code>~mods</code> 폴더는 자동으로 유지됩니다.</p>
    </div>

    <div class="ssection">
      <div class="slabel">View</div>
      <div class="seg" id="pf-show">
        <button data-show="all" class="${modsState.show === "all" ? "on" : ""}">All</button>
        <button data-show="enabled" class="${modsState.show === "enabled" ? "on" : ""}">Enabled</button>
        <button data-show="disabled" class="${modsState.show === "disabled" ? "on" : ""}">Disabled</button>
      </div>
    </div>

    <div class="ssection">
      <div class="slabel">Updates</div>
      <div class="side-status">${esc(updateLabel)}</div>
      <button class="sideitem" id="side-check"><span>Check updates</span><span class="si-ico">↻</span></button>
    </div>

    <div class="ssection">
      <div class="slabel">Links</div>
      <button class="sideitem" id="side-github"><span>GitHub repository</span><span class="si-ico svgico">${ICON_GITHUB}</span></button>
      <button class="sideitem" id="side-mod-repo"><span>Mod repository</span><span class="si-ico">↗</span></button>
    </div>
`;
  const title = document.querySelector("#mods-title"); if (title) title.textContent = activeProfile?.name ?? "Mods";
  wireSidebar(side);
}

function wireSidebar(side) {
  const dropdown = side.querySelector(".profile-dd");
  const menu = side.querySelector("#pf-menu");
  const select = side.querySelector("#pf-select");
  let outsideHandler = null;
  const closeMenu = () => {
    menu.hidden = true;
    select.setAttribute("aria-expanded", "false");
    if (outsideHandler) document.removeEventListener("click", outsideHandler);
    outsideHandler = null;
  };
  select.addEventListener("click", () => {
    const shouldOpen = menu.hidden;
    if (shouldOpen) {
      menu.hidden = false;
      outsideHandler = (e) => { if (!dropdown.contains(e.target)) closeMenu(); };
      setTimeout(() => document.addEventListener("click", outsideHandler), 0);
    } else {
      closeMenu();
    }
    select.setAttribute("aria-expanded", menu.hidden ? "false" : "true");
  });
  side.querySelectorAll("[data-pf]").forEach((b) => b.addEventListener("click", async () => {
    closeMenu();
    if (b.dataset.pf === activeProfile?.id) return;
    try { await invoke("switch_profile", { id: b.dataset.pf }); await renderMods(); } catch (e) { toast("err", String(e)); }
  }));
  side.querySelector("#pf-new").addEventListener("click", () => askName("New profile", "", "Create", async (name) => {
    try { const p = await invoke("create_profile", { name }); await invoke("switch_profile", { id: p.id }); await renderMods(); } catch (e) { toast("err", String(e)); }
  }));
  side.querySelector("#pf-dup").addEventListener("click", () => askName("Duplicate profile", `${activeProfile?.name ?? "Profile"} copy`, "Duplicate", async (name) => {
    try { const p = await invoke("duplicate_profile", { srcId: activeProfile.id, name }); await invoke("switch_profile", { id: p.id }); await renderMods(); } catch (e) { toast("err", String(e)); }
  }));
  side.querySelector("#pf-rename").addEventListener("click", () => askName("Rename profile", activeProfile?.name ?? "", "Rename", async (name) => {
    try { await invoke("rename_profile", { id: activeProfile.id, name }); await renderMods(); } catch (e) { toast("err", String(e)); }
  }));
  side.querySelector("#pf-del").addEventListener("click", () => {
    if (!activeProfile || profiles.length <= 1) return;
    const other = profiles.find((p) => p.id !== activeProfile.id);
    askConfirm("Delete profile", `Delete '${activeProfile.name}'? Mods stay in the library, but this profile's on/off set will be removed. The active profile will switch to '${other.name}'.`, "Delete", async () => {
      try { await invoke("switch_profile", { id: other.id }); await invoke("delete_profile", { id: activeProfile.id }); await renderMods(); } catch (e) { toast("err", String(e)); }
    });
  });
  side.querySelector("#side-redetect").addEventListener("click", renderSidebar);
  side.querySelector("#side-import").addEventListener("click", () => importViaPicker());
  side.querySelector("#side-check").addEventListener("click", () => checkAllUpdates(true));
  side.querySelector("#side-github").addEventListener("click", () => openExternal(REPO_URL));
  side.querySelector("#side-mod-repo").addEventListener("click", () => openExternal(MOD_REPOSITORY_URL));
  side.querySelectorAll("[data-show]").forEach((b) => b.addEventListener("click", () => { modsState.show = b.dataset.show; renderMods(); }));
}

async function renderMods() {
  await loadMods();
  const el = document.querySelector("#view-mods");
  const list = filteredMods();
  const enabled = modsState.mods.filter((m) => m.enabled).length;
  el.innerHTML = `<div class="mods">
    <aside class="side" id="mods-side"></aside>
    <div class="main"><div class="content">
      <div class="head"><div class="ptitle" id="mods-title">Mods</div>
        <div class="subhead"><div class="meta">${modsState.mods.length} mods · ${enabled} enabled</div>
          <div class="hctrls">
            <div class="search"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><circle cx="11" cy="11" r="7"/><path d="m20 20-3.2-3.2"/></svg><input id="mods-search" placeholder="Search mods" value="${esc(modsState.query)}"></div>
          </div></div></div>
      <div class="list">${list.map(rowHtml).join("") || `<div class="src" style="padding:24px 4px">No mods.</div>`}</div>
    </div></div></div>`;
  renderSidebar(); // Task 11
  wireMods(el);
}

function wireMods(el) {
  el.querySelector("#mods-search").addEventListener("input", (e) => { modsState.query = e.target.value; rerenderList(); });
  el.querySelectorAll("[data-tog]").forEach((cb) => cb.addEventListener("change", async (e) => {
    const id = cb.dataset.tog;
    try { await invoke("set_mod_enabled", { id, enabled: cb.checked }); await renderMods(); }
    catch (err) { toast("err", String(err)); await renderMods(); }
  }));
  el.querySelectorAll("[data-rm]").forEach((b) => b.addEventListener("click", () => confirmRemove(b.dataset.rm)));
}
function rerenderList() { renderMods(); }

function notifyImported(v) {
  toast("ok", `Imported ${v.name}`);
  if (v && Array.isArray(v.removed) && v.removed.length) {
    showInfo("변환 중 파일 제거됨", `이 pak을 3종으로 변환하는 과정에서 다음 파일이 제거되었습니다(비에셋 또는 구버전 UE4 에셋). 모드 동작에 문제가 있으면 원본 배포본을 확인하세요.\n\n${v.removed.join("\n")}`);
  }
}

async function importViaPicker() {
  // 네이티브 NSOpenPanel 하나로 파일(zip/pak) 또는 폴더를 선택해 임포트.
  try { const v = await invoke("pick_mod_path"); if (v) { notifyImported(v); await renderMods(); } }
  catch (e) { reportModError(e); }
}
async function doUpdate(id) {
  try {
    const v = await invoke("update_mod", { id });
    delete modsState.updates[id];
    toast("ok", `Updated ${id}`);
    if (v && Array.isArray(v.removed) && v.removed.length) {
      showInfo("변환 중 파일 제거됨", `이 pak을 3종으로 변환하는 과정에서 다음 파일이 제거되었습니다(비에셋 또는 구버전 UE4 에셋). 모드 동작에 문제가 있으면 원본 배포본을 확인하세요.\n\n${v.removed.join("\n")}`);
    }
    await renderMods();
  }
  catch (e) { reportModError(e); }
}
function confirmRemove(id) {
  askConfirm("Remove mod", `Remove '${id}' from the library?`, "Remove", async () => {
    try { await invoke("remove_mod", { id }); toast("ok", "Removed"); await renderMods(); }
    catch (e) { toast("err", String(e)); }
  });
}

async function renderSettings() {
  const el = document.querySelector("#view-settings");
  let det = {};
  try { det = await invoke("detect_game"); } catch (e) { /* 표시만 */ }
  const found = !!(det && det.game_installed);
  const containerOk = !!(det && det.container_exists);
  el.innerHTML = `<div class="main"><div class="content">
    <div class="ptitle">Settings</div>
    <div class="ssub">Game install · UE4SS · Game log</div>
    <div class="grp"><div class="gl">Game</div><div class="cardlist">
      <div class="srow"><div class="rl"><div class="rt">Palworld install</div>
        <div class="rd">${found ? "Game install found" : "Game not found"}</div></div>
        <div class="rc"><button class="sbtn" id="set-redetect">Re-detect</button><button class="sbtn" id="set-manual-game">Manual</button></div></div>
    </div></div>
    <div class="grp"><div class="gl">UE4SS loader</div><div class="cardlist">
      <div class="srow"><div class="rl"><div class="rt">Status</div>
        <div class="rd">${containerOk ? "Container found · DYLD injection ready" : found ? "Container missing (run the game once)" : "Game not detected"}</div></div>
        <div class="rc"></div></div>
      <div class="srow"><div class="rl"><div class="rt">UE4SS version</div>
        <div class="rd" id="ue4ss-ver">Checking…</div></div>
        <div class="rc"></div></div>
      <div class="srow"><div class="rl"><div class="rt">앱 버전</div>
        <div class="rd" id="app-ver">Checking…</div></div>
        <div class="rc"><button class="sbtn" id="app-check">앱 업데이트 확인</button></div></div>
    </div></div>
    <div class="grp"><div class="gl">Game log</div><div class="cardlist">
      <div class="srow"><div class="rl"><div class="rt">UE4SS.log</div><div class="rd">Last 64KB of the container log</div></div>
        <div class="rc"><button class="sbtn" id="log-show">Show</button><button class="sbtn" id="log-refresh">Refresh</button></div></div>
      <pre class="gamelog" id="gamelog" hidden></pre>
    </div></div>
  </div></div>`;
  el.querySelector("#set-redetect").addEventListener("click", renderSettings);
  el.querySelector("#set-manual-game").addEventListener("click", async () => {
    try {
      const picked = await invoke("pick_game_binary");
      if (picked) toast("ok", "Game path saved");
      await renderSettings();
    } catch (e) { toast("err", String(e)); }
  });
  // UE4SS 버전 표시(설치/체크는 좌하단 팝업 카드로 이동 — 여기선 버전 확인만).
  const ue4ssVer = el.querySelector("#ue4ss-ver");
  (async () => {
    try {
      const s = await invoke("ue4ss_status");
      ue4ssVer.textContent = s.error ? `Current ${s.current} · check failed`
        : s.update_available ? `Current ${s.current} · latest ${s.latest} — 좌하단 알림에서 설치`
        : `Current ${s.current} · up to date`;
    } catch (e) { ue4ssVer.textContent = String(e); }
  })();

  // 앱 버전 + 수동 업데이트 확인(결과는 공통 팝업 카드로 표시).
  const appVer = el.querySelector("#app-ver");
  async function refreshApp(viaButton) {
    appVer.textContent = "Checking…";
    try {
      const s = await invoke("app_status");
      appVer.textContent = s.error ? `Current ${s.current} · check failed`
        : s.update_available ? `Current ${s.current} · latest ${s.latest} — update available`
        : `Current ${s.current} · up to date`;
      if (s.update_available) {
        if (viaButton) dismissedUpdates.delete("app");
        if (!dismissedUpdates.has("app")) renderAppCard(s);
      } else if (viaButton) toast("ok", "앱이 최신입니다");
      if (s.error && viaButton) toast("err", s.error);
    } catch (e) { appVer.textContent = String(e); if (viaButton) toast("err", String(e)); }
  }
  el.querySelector("#app-check").addEventListener("click", () => refreshApp(true));
  refreshApp(false); // Settings 진입 시 버전 표시(있으면 카드도, 자동설치 없음)

  const show = async () => {
    const pre = el.querySelector("#gamelog"); pre.hidden = false;
    try { pre.textContent = await invoke("read_log"); pre.scrollTop = pre.scrollHeight; }
    catch (e) { pre.textContent = String(e); }
  };
  el.querySelector("#log-show").addEventListener("click", show);
  el.querySelector("#log-refresh").addEventListener("click", show);
}

// ── Toast helper ──
function toast(kind, msg) {
  const host = document.querySelector("#toasts");
  const el = document.createElement("div");
  el.className = `toast ${kind}`; el.textContent = msg;
  host.appendChild(el);
  setTimeout(() => el.remove(), kind === "err" ? 5200 : 2400);
}

// ── Update notification cards (좌하단 지속형 팝업 스택) ──
// 세 축(모드·UE4SS·앱)을 앱 실행 시 자동 체크 → 있는 것만 카드로. 닫으면 이번 실행만 숨김.
const dismissedUpdates = new Set(); // 이번 실행에 사용자가 닫은 종류(자동 재팝업 방지)

function removeUpdateCard(key) { document.querySelector(`#uc-${key}`)?.remove(); }
function dismissUpdateCard(key) { dismissedUpdates.add(key); removeUpdateCard(key); }

// 카드 1장 생성/교체(종류당 최대 1장). innerHtml = head+actions(+body). 반환 el.
function upsertUpdateCard(key, innerHtml) {
  const host = document.querySelector("#update-stack");
  if (!host) return null;
  let el = document.querySelector(`#uc-${key}`);
  if (!el) { el = document.createElement("div"); el.className = "ucard"; el.id = `uc-${key}`; host.appendChild(el); }
  el.innerHTML = innerHtml;
  el.querySelector("[data-uc-x]")?.addEventListener("click", () => dismissUpdateCard(key));
  return el;
}
// 버전 표기 통일: 숫자로 시작하면 v 접두사를 붙여 "v0.2.1" 형태로 맞춤(이미 v면 그대로).
function fmtVer(v) {
  const s = String(v ?? "").trim();
  return /^\d/.test(s) ? "v" + s : s;
}
function cardHead(title) {
  return `<div class="uc-head"><span class="uc-title">${esc(title)}</span><button class="uc-x" data-uc-x aria-label="닫기">✕</button></div>`;
}

// 모드 카드: modsState.updates 기준(펼치면 모드별 개별 업데이트). 없으면 카드 제거.
function renderModCard() {
  const ups = Object.values(modsState.updates).filter((u) => u && u.has_update);
  if (!ups.length) { removeUpdateCard("mods"); return; }
  const items = ups.map((u) => `<div class="uc-item"><span class="uc-item-name">${esc(u.name || u.id)} → ${esc(fmtVer(u.latest))}</span><button class="uc-mini" data-uc-mod="${esc(u.id)}">업데이트</button></div>`).join("");
  const html = cardHead(`모드 ${ups.length}개 업데이트`)
    + `<div class="uc-actions"><button class="uc-toggle" data-uc-exp>상세 <span class="uc-caret">▾</span></button><button class="uc-btn" data-uc-all>모두 업데이트</button></div>`
    + `<div class="uc-body" hidden>${items}</div>`;
  const el = upsertUpdateCard("mods", html);
  if (!el) return;
  el.querySelector("[data-uc-exp]")?.addEventListener("click", (e) => {
    const body = el.querySelector(".uc-body");
    body.hidden = !body.hidden;
    e.currentTarget.innerHTML = body.hidden ? '상세 <span class="uc-caret">▾</span>' : '상세 <span class="uc-caret">▴</span>';
  });
  el.querySelector("[data-uc-all]")?.addEventListener("click", async () => {
    const ids = Object.values(modsState.updates).filter((u) => u && u.has_update).map((u) => u.id);
    for (const id of ids) { await doUpdate(id); }
    renderModCard();
  });
  el.querySelectorAll("[data-uc-mod]").forEach((b) => b.addEventListener("click", async () => {
    await doUpdate(b.dataset.ucMod);
    renderModCard();
  }));
}

// UE4SS 카드: 설치는 여기서(설정에서 이동). 반환 없음.
function renderUe4ssCard(s) {
  if (!s || !s.update_available) { removeUpdateCard("ue4ss"); return; }
  const html = cardHead(`UE4SS ${fmtVer(s.current) || "?"} → ${fmtVer(s.latest)}`)
    + `<div class="uc-actions"><button class="uc-btn" data-uc-ue>업데이트</button></div>`;
  const el = upsertUpdateCard("ue4ss", html);
  el?.querySelector("[data-uc-ue]")?.addEventListener("click", async (e) => {
    const btn = e.currentTarget; btn.disabled = true; btn.textContent = "설치 중…";
    try {
      const v = await invoke("ue4ss_install_update");
      toast("ok", `UE4SS ${v} 설치됨`);
      dismissedUpdates.add("ue4ss"); removeUpdateCard("ue4ss");
    } catch (err) { toast("err", String(err)); btn.disabled = false; btn.textContent = "업데이트"; }
  });
}

// 앱 카드: ad-hoc 서명이라 자동설치 불가 → 릴리즈 페이지로 안내.
function renderAppCard(s) {
  if (!s || !s.update_available) { removeUpdateCard("app"); return; }
  const html = cardHead(`앱 ${fmtVer(s.current)} → ${fmtVer(s.latest)}`)
    + `<div class="uc-actions"><button class="uc-btn" data-uc-app>릴리즈 페이지</button></div>`;
  const el = upsertUpdateCard("app", html);
  el?.querySelector("[data-uc-app]")?.addEventListener("click", () => openExternal(s.release_url || `${REPO_URL}/releases/latest`));
}

// 공통 오케스트레이터: 3종 병렬 체크(실패 격리). viaButton=수동 재체크(닫은 것 초기화+결과 토스트).
async function checkAllUpdates(viaButton) {
  if (viaButton) { dismissedUpdates.clear(); toast("ok", "업데이트 확인 중…"); }
  const tasks = [
    (async () => {
      const list = await invoke("check_updates");
      modsState.updates = {}; list.forEach((u) => (modsState.updates[u.id] = u));
      await renderMods();
      if (dismissedUpdates.has("mods")) removeUpdateCard("mods"); else renderModCard();
      return { count: list.filter((u) => u.has_update).length, error: null };
    })(),
    (async () => {
      const s = await invoke("ue4ss_status");
      if (dismissedUpdates.has("ue4ss")) removeUpdateCard("ue4ss"); else renderUe4ssCard(s);
      return { count: s && s.update_available ? 1 : 0, error: s && s.error };
    })(),
    (async () => {
      const s = await invoke("app_status");
      if (dismissedUpdates.has("app")) removeUpdateCard("app"); else renderAppCard(s);
      return { count: s && s.update_available ? 1 : 0, error: s && s.error };
    })(),
  ];
  const results = await Promise.allSettled(tasks);
  if (!viaButton) return;
  const totals = results.reduce((n, r) => n + (r.status === "fulfilled" ? r.value.count : 0), 0);
  const fails = results.filter((r) => r.status === "rejected").length
    + results.filter((r) => r.status === "fulfilled" && r.value.error).length;
  if (totals === 0 && fails === 0) toast("ok", "모두 최신 상태입니다");
  else if (fails) toast("err", "일부 업데이트 확인에 실패했어요");
}

// ── Modal helpers (WKWebView prompt 금지 — 인앱 모달로 대체) ──
function askName(title, initial, confirmLabel, onConfirm) { openModal({ kind: "input", title, value: initial, confirmLabel, onConfirm }); }
function askConfirm(title, msg, confirmLabel, onConfirm) { openModal({ kind: "confirm", title, msg, confirmLabel, danger: true, onConfirm }); }
function showInfo(title, msg) { openModal({ kind: "info", title, msg, confirmLabel: "Close", onConfirm: () => {} }); }

// import/update 에러 공통 처리: pak 변환 실패면 안내 모달, 그 외는 토스트.
function reportModError(e) {
  const s = String(e && e.message ? e.message : e);
  const PFX = "PAK_CONVERT_NEEDS_DECISION:";
  if (s.startsWith(PFX)) {
    let removed = [], err = "";
    try { const d = JSON.parse(s.slice(PFX.length)); removed = d.removed || []; err = d.error || ""; } catch { /* 파싱 실패 시 원문 */ }
    const rl = removed.length ? `\n제거된 비에셋 파일: ${removed.join(", ")}` : "";
    showInfo("변환 실패", `이 pak은 비에셋 파일을 제거한 뒤에도 IoStore로 변환하지 못했습니다.${rl}\n\nretoc: ${err || "(상세 없음)"}\n\n에셋 자체 문제일 수 있습니다. 모드 제작자에게 문의하거나 다른 배포본을 받아 다시 시도하세요.`);
  } else {
    toast("err", s);
  }
}

function openModal(d) {
  const host = document.querySelector("#modal-host");
  const isInfo = d.kind === "info";
  host.innerHTML = `<div class="mbg"><div class="modal">
    <div class="mt">${esc(d.title)}</div>
    ${d.kind === "input" ? `<input class="minput" id="m-in" value="${esc(d.value || "")}">` : `<div class="mmsg"${isInfo ? ' style="white-space:pre-line"' : ""}>${esc(d.msg)}</div>`}
    <div class="mact">${isInfo ? "" : `<button class="mcancel" id="m-cancel">Cancel</button>`}
      <button class="mok ${d.danger ? "danger" : ""}" id="m-ok">${esc(d.confirmLabel)}</button></div>
  </div></div>`;
  const close = () => (host.innerHTML = "");
  const ok = () => {
    if (d.kind === "input") {
      const v = host.querySelector("#m-in").value.trim();
      if (!v) return; close(); d.onConfirm(v);
    } else { close(); d.onConfirm(); }
  };
  const cancelBtn = host.querySelector("#m-cancel");
  if (cancelBtn) cancelBtn.addEventListener("click", close);
  host.querySelector("#m-ok").addEventListener("click", ok);
  host.querySelector(".mbg").addEventListener("click", (e) => { if (e.target.classList.contains("mbg")) close(); });
  const inp = host.querySelector("#m-in");
  if (inp) { inp.focus(); inp.select(); inp.addEventListener("keydown", (e) => { if (e.key === "Enter") ok(); if (e.key === "Escape") close(); }); }
}

// ── Drop overlay + import (onDragDropEvent — WKWebView HTML5 drag 이벤트 미지원) ──
if (window.__TAURI__.webview) {
  const { getCurrentWebview } = window.__TAURI__.webview;
  const overlay = document.querySelector("#drop-overlay");
  getCurrentWebview().onDragDropEvent(async (event) => {
    const t = event.payload.type;
    if (t === "enter" || t === "over") { if (overlay) overlay.hidden = false; return; }
    if (t === "leave") { if (overlay) overlay.hidden = true; return; }
    if (t === "drop") {
      if (overlay) overlay.hidden = true;
      for (const path of event.payload.paths) {
        try { const v = await invoke("import_mod", { path }); notifyImported(v); }
        catch (e) { reportModError(e); }
      }
      if (current === "mods") await renderMods();
    }
  });
}

// Set up mod-staging event listener
listen("mod-staging", (e) => {
  const el = document.getElementById("staging-status");
  if (!el) return;
  if (e.payload === "start") {
    el.textContent = "모드 설치 중…";
    el.hidden = false;
  } else if (e.payload === "done") {
    el.hidden = true;
  }
});

setView("play");

// 시작 시 3종(모드·UE4SS·앱) 자동 체크 → 있는 것만 좌하단 팝업 카드로. 없으면 조용.
checkAllUpdates(false);
