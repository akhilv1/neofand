(() => {
  'use strict';
  const frame = document.getElementById('sensor-diagram-frame');
  if (!frame) return;
  const image = document.getElementById('sensor-diagram-photo');
  const freshness = document.getElementById('diagram-freshness');
  const sources = {gpu:'/hardware/assets/macpro-gpu-approved.png',cooling:'/hardware/assets/macpro-fan-side.png'};
  document.querySelectorAll('[data-diagram-side]').forEach(button => {
    button.addEventListener('click', () => {
      const side = button.dataset.diagramSide;
      frame.dataset.side = side;
      image.src = sources[side];
      image.alt = side === 'gpu' ? 'Mac Pro interior with three spaced RTX A6000 cards and hidden mounting tabs' : 'Reverse side of the Mac Pro with the PSU blower and memory banks';
      document.querySelectorAll('[data-diagram-side]').forEach(b => b.setAttribute('aria-pressed', String(b === button)));
    });
  });
  function format(value, unit) {
    if (!Number.isFinite(value)) return '—';
    return `${new Intl.NumberFormat(undefined, {maximumFractionDigits:unit === '°C' ? 1 : 0}).format(value)} ${unit}`;
  }
  window.neofandDiagramUpdate = latest => {
    const values = new Map((latest || []).map(item => [`${item.subsystem}/${item.name}`, item.value]));
    document.querySelectorAll('[data-diagram-sensor]').forEach(node => {
      const raw = values.get(node.dataset.diagramSensor);
      node.textContent = format(typeof raw === 'number' ? raw : NaN, node.dataset.unit || '');
    });
    const psuTemps = ['Tp0P','Tp1P','Tp2P'].map(name => values.get(`smc_temp/${name}`)).filter(Number.isFinite);
    document.getElementById('diagram-psu-temp').textContent = format(psuTemps.length ? Math.max(...psuTemps) : NaN, '°C');
    frame.dataset.connection = 'live';
    freshness.dataset.error = 'false';
    freshness.textContent = `Updated ${new Date().toLocaleTimeString([], {hour:'2-digit',minute:'2-digit'})}`;
  };
  window.neofandDiagramError = () => {
    frame.dataset.connection = 'error';
    freshness.dataset.error = 'true';
    freshness.textContent = 'Readings unavailable';
  };
})();
