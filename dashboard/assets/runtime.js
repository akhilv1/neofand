(() => {
  'use strict';
  const auto = document.getElementById('auto');
  const live = document.getElementById('live');
  const liveControl = document.getElementById('live-control');
  const cadence = document.getElementById('refresh-cadence');
  const status = document.getElementById('runtime-mode');
  const sample = document.getElementById('runtime-sample');
  let timer, lastSample = 0, connected = false;
  try {
    const saved = JSON.parse(localStorage.getItem('neofand-refresh-mode') || 'null');
    if (saved) { auto.checked = saved.auto !== false; live.checked = Boolean(saved.live); }
  } catch (_error) { /* preferences are optional */ }
  const canLive = () => state.tab === 'overview' || state.tab === 'sensors';
  const isLive = () => auto.checked && live.checked && canLive();
  function paintStatus() {
    const delayed = lastSample && Date.now() / 1000 - lastSample > (state.tab === 'storage' ? 60 : 30);
    const mode = !auto.checked ? 'Paused' : !connected ? lastSample ? 'Disconnected' : 'Connecting' : delayed ? 'Delayed' : isLive() ? 'Live' : 'Auto';
    status.textContent = mode;
    status.dataset.live = String(mode === 'Live');
    if (lastSample) {
      const date = new Date(lastSample * 1000);
      sample.textContent = date.toLocaleTimeString([], {hour:'2-digit',minute:'2-digit',second:'2-digit'});
      sample.dateTime = date.toISOString();
      sample.title = `Last recorded sample: ${date.toLocaleString()}`;
    }
  }
  function sampleReceived(latest) {
    const timestamps = latest.map(item => item.ts).filter(Number.isFinite);
    if (timestamps.length) lastSample = Math.max(...timestamps);
    connected = true;
    paintStatus();
  }
  function schedule() {
    clearTimeout(timer);
    liveControl.hidden = !canLive();
    cadence.textContent = auto.checked ? isLive() ? 'Readings 2s · charts 10s' : 'Refresh every 30s' : 'Manual refresh';
    paintStatus();
    if (!auto.checked) return;
    timer = setTimeout(async () => {
      if (!document.hidden && !state.refreshing) {
        if (isLive() && !(state.tab === 'overview' && Date.now() - (state.lastChartRefresh || 0) >= 10000)) {
          state.refreshing = true;
          const controller = new AbortController();
          const timeout = setTimeout(() => controller.abort(), 8000);
          try {
            state.latest = await getJSON('/hardware/api/latest', {cache:'no-store', signal:controller.signal});
            renderSummary();
            window.neofandDiagramUpdate?.(state.latest);
            sampleReceived(state.latest);
          } catch (_error) {
            connected = false;
            status.textContent = 'Disconnected';
            status.dataset.live = 'false';
            window.neofandDiagramError?.();
          } finally { clearTimeout(timeout); state.refreshing = false; }
        } else {
          await refresh();
        }
      }
      schedule();
    }, isLive() ? 2000 : 30000);
  }
  function saveMode() { try { localStorage.setItem('neofand-refresh-mode', JSON.stringify({auto:auto.checked,live:live.checked})); } catch (_error) {} }
  auto.addEventListener('change', () => { if (!auto.checked) live.checked = false; saveMode(); schedule(); });
  live.addEventListener('change', () => { if (live.checked) auto.checked = true; saveMode(); schedule(); });
  window.neofandRuntime = {
    sample:sampleReceived,
    modeChanged:schedule,
    error:() => { connected = false; status.textContent = 'Disconnected'; status.dataset.live = 'false'; }
  };
  fetch('/hardware/assets/build-info.json', {cache:'no-store'}).then(r => {
    if (!r.ok) throw new Error('Build info unavailable');
    return r.json();
  }).then(build => {
    const host = document.getElementById('runtime-build');
    const label = document.createElement('span');
    label.textContent = build.pr_number ? `PR #${build.pr_number} · ` : 'Branch build · ';
    const link = document.createElement('a');
    link.href = `https://github.com/akhilv1/neofand/commit/${encodeURIComponent(build.commit)}`;
    link.textContent = String(build.commit).slice(0,7);
    link.target = '_blank'; link.rel = 'noopener noreferrer';
    host.replaceChildren(label, link);
    document.getElementById('runtime-branch').textContent = build.branch || '';
  }).catch(() => { document.getElementById('runtime-build').textContent = 'Build information unavailable'; });
  if (state.latest.length) sampleReceived(state.latest);
  schedule();
})();
