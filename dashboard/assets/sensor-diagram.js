(() => {
  'use strict';
  const frames = [...document.querySelectorAll('.sensor-diagram .diagram-frame')];
  if (!frames.length) return;
  const freshness = document.querySelectorAll('.sensor-diagram .diagram-live');
  let memoryPending = false;
  function format(value, unit) {
    if (!Number.isFinite(value)) return '—';
    return `${new Intl.NumberFormat(undefined, {maximumFractionDigits:unit === '°C' ? 1 : 0}).format(value)} ${unit}`;
  }
  function setLoad(node, value) {
    node.value = Number.isFinite(value) ? Math.max(0, Math.min(100, value)) : 0;
    node.setAttribute('aria-valuetext', Number.isFinite(value) ? `${value.toFixed(1)} percent` : 'Unavailable');
  }
  async function refreshMemory() {
    if (memoryPending) return;
    memoryPending = true;
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 5000);
    const status = document.getElementById('diagram-memory-status');
    const box = document.querySelector('.system-memory');
    try {
      const response = await fetch('/hardware/api/host-memory', {cache:'no-store', signal:controller.signal});
      if (!response.ok) throw new Error('Memory snapshot unavailable');
      const memory = await response.json();
      document.querySelectorAll('[data-memory-field]').forEach(node => {
        const key = node.dataset.memoryField, value = memory[key];
        node.textContent = !Number.isFinite(value) ? '—' : key.endsWith('_bytes')
          ? `${(value / 1073741824).toFixed(1)} GiB`
          : `${value.toFixed(key.startsWith('pressure_') ? 2 : 1)} %`;
      });
      setLoad(document.getElementById('diagram-memory-load'), memory.used_pct);
      status.textContent = Number.isFinite(memory.pressure_full_avg10)
        ? `Full memory stalls ${memory.pressure_full_avg10.toFixed(2)}% · PSI, 10s`
        : 'Memory pressure unavailable';
      box.dataset.connection = 'live';
    } catch (_error) {
      status.textContent = 'Memory readings unavailable';
      box.dataset.connection = 'error';
    } finally {
      clearTimeout(timeout);
      memoryPending = false;
    }
  }
  window.neofandDiagramUpdate = latest => {
    const values = new Map((latest || []).map(item => [`${item.subsystem}/${item.name}`, item.value]));
    document.querySelectorAll('[data-diagram-sensor]').forEach(node => {
      node.textContent = format(values.get(node.dataset.diagramSensor), node.dataset.unit || '');
    });
    document.querySelectorAll('[data-diagram-load]').forEach(node => setLoad(node, values.get(node.dataset.diagramLoad)));
    const psuTemps = ['Tp0P','Tp1P','Tp2P'].map(name => values.get(`smc_temp/${name}`)).filter(Number.isFinite);
    document.querySelectorAll('[data-psu-temperature]').forEach(node => {
      node.textContent = format(psuTemps.length ? Math.max(...psuTemps) : NaN, '°C');
    });
    frames.forEach(frame => { frame.dataset.connection = 'live'; });
    freshness.forEach(node => {
      node.dataset.error = 'false';
      node.textContent = `Updated ${new Date().toLocaleTimeString([], {hour:'2-digit',minute:'2-digit'})}`;
    });
    refreshMemory();
  };
  window.neofandDiagramError = () => {
    frames.forEach(frame => { frame.dataset.connection = 'error'; });
    freshness.forEach(node => { node.dataset.error = 'true'; node.textContent = 'Readings unavailable'; });
  };
})();
