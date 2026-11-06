"""Build a read-only P620 frontend from the shared dashboard, preserving Mac assets."""
from pathlib import Path
import re
import sys

root=Path(__file__).resolve().parent
source=Path(sys.argv[1]);output=Path(sys.argv[2])
html=source.read_text()
html=html.replace('id="hostline">neo-mac-pro','id="hostline">Connecting to hardware')
html=html.replace('</head>','<link rel="stylesheet" href="/hardware/assets/p620.css?v=20261105-drive-alerts">\n</head>')
start=html.index('    <div id="tab-sensors"');end=html.index('    <div id="tab-errors"',start)
html=html[:start]+(root/'p620-sensors.html').read_text()+'\n'+html[end:]
html=html.replace('/hardware/assets/sensor-diagram.js?v=20261104-7','/hardware/assets/p620.js?v=20261105-lxc2')
html=html.replace('/^coretemp/','/^(coretemp|k10temp)/').replace('/^(coretemp|pch|acpitz)/','/^(coretemp|k10temp|pch|acpitz)/')
html=html.replace('const TABS = ["overview", "power", "thermal", "sensors", "storage", "errors", "services", "boot", "portainer"];','const TABS = ["overview", "power", "thermal", "sensors", "storage", "errors", "portainer"];')
# No service/boot/control requests on this read-only Linux adapter.
for tab in ('services','boot'):
 html=re.sub(r'<a data-tab="'+tab+r'".*?</a>','',html)
 html=html.replace('id="tab-'+tab+'"','style="display:none" id="tab-'+tab+'"')
# Summary must not report absent Apple SMC flags as healthy/closed.
a=html.index('function renderSummary()');b=html.index('\nlet eventRequest=',a)
html=html[:a]+'''function renderSummary() {
 window.neofandResourcesUpdate?.(state.latest);
 const val=(s,n)=>latestOf(s,n), f=(v,u)=>v==null?'Unavailable':v.toFixed(1)+' '+u;
 const info=window.neofandHostInfo||{},host=info.host||{},gpu=info.gpus?.[0];
 const rows=[['Platform',host.display_name||host.product_family||host.product_version||'Connecting'],['CPU',host.cpu||'Unavailable'],
 ['CPU load',f(val('host','cpu_pct'),'%')],['Memory in use',f(val('host','mem_used_pct'),'%')],
 ['GPU',gpu?.name||'Unavailable'],['GPU board power',f(val('gpu','gpu0_power_w'),'W')],
 ['GPU temperature',f(val('gpu','gpu0_temp_c'),'°C')],['Total PSU power / chassis fans','Not exposed by the host']];
 put($('summary'),table(['Item','Reading'],rows));
 const fresh=state.latest.length&&Date.now()/1000-Math.max(...state.latest.map(m=>m.ts))<30;
 const s=statusEl(fresh?'good':'warn',fresh?'Telemetry connected':'Readings delayed');
 $('hdrstate').replaceChildren(...s.childNodes);$('hdrstate').className=s.className;
}
''' + html[b:]
# Power view shows GPU board power without presenting it as total system consumption.
start=html.index('    <div id="tab-power"');end=html.index('    <div id="tab-thermal"',start)
html=html[:start]+'''<div id="tab-power" class="grid" hidden><section class="card wide" data-chart="p620power"><h2>GPU board power</h2><div class="body"><p class="sub">GPU only. Total PSU consumption is not available on this host.</p><div class="plot"><canvas id="c-p620power"></canvas><div class="tip" id="t-p620power"></div></div><div class="legend" id="l-p620power"></div><div id="d-p620power" hidden></div></div></section></div>\n''' + html[end:]
a=html.index('      renderHeadroom();',html.index('if (state.tab === "overview")'));b=html.index('\n    }\n    else if (state.tab === "thermal")',a)
html=html[:a]+'''      await panel('p620power','gpu',namesIn('gpu',/power_w$/),{zeroBase:true});'''+html[b:]
html=html.replace('const CARD_REQS = {',"const CARD_REQS = {\n  p620power: () => namesIn('gpu',/power_w$/).length > 0,")
# Shared builder defaults to P620 on this host without touching saved Mac layouts.
# Initialization happens before the first refresh.
inject="""
const historyNote=el('div','p620-history','Collecting history on this host…');
$('tab-overview').prepend(historyNote);
getJSON('/hardware/api/host-info').then(d=>{
 window.neofandHostInfo=d;
 $('hostline').textContent=(d.host.display_name||d.host.product_family||d.host.product_version||'Linux')+' · '+d.host.hostname;
 window.neofandHostConnected?.(d);renderSummary();
 historyNote.textContent='History starts '+fmtTime(d.history_since)+'. Live samples every 2 seconds; chart history every 30 seconds.';
}).catch(()=>historyNote.textContent='History metadata unavailable.');
"""
html=html.replace('const requestedTab = location.hash.slice(1);',inject+'\nconst requestedTab = location.hash.slice(1);')
html=html.replace('Power zones</a>','Power</a>').replace('<div class="grp">System</div>','')
html=html.replace('Package temperature from coretemp, plus the PCH and chassis ambient.','AMD k10temp control and chiplet temperatures.')
html=html.replace('10 GbE PHY/MAC dies and the RX 460 driving the console.','Temperatures exposed by the network interfaces.')
html=html.replace('Every fan as a percentage of its own ceiling, so chassis RPM and card duty share one axis.','GPU fan duty. Chassis fan RPM is not available.')
html=html.replace('name: "CPU package", points: x.points','name: "CPU " + x.name.split("_").pop(), points: x.points')
html=html.replace('See what’s using each drive. Categories estimate Models, Photos, Backups and more from filenames and folders. Expand a category to see its largest contributors.','Physical drives, ZFS pools and mounted filesystem usage on this host.')
html=html.replace('The metadata index reconciles about every 30 minutes; cached categories remain visible during updates. Space totals refresh separately.','File category indexing is not enabled on this host. Space totals refresh separately.')
html=html.replace('<div class="grp">Monitoring</div>','<div class="grp">Monitoring</div><a data-tab="proxmox">Proxmox</a>')
html=html.replace('<div id="tab-storage" hidden>','<div id="tab-proxmox" class="grid" hidden><section class="card wide"><h2>Proxmox</h2><div class="body"><p id="proxmox-status">Loading host inventory…</p><p class="sub">Read-only node, VM/container status and recent tasks. Guest controls are managed in Proxmox.</p><div class="tw" id="proxmox-resources"></div></div></section></div><div id="tab-storage" hidden>')
html=html.replace('<div id="tab-overview" class="grid">','<div id="tab-overview" class="grid"><section class="card wide pve-systems"><h2>Systems <span id="pve-guest-count" class="tail"></span></h2><div class="body"><p class="sub">VMs and LXCs on this Proxmox node</p><div id="pve-guest-tiles"></div></div></section>')
html=html.replace('const TABS = ["overview", "power", "thermal", "sensors", "storage", "errors", "portainer"];','const TABS = ["overview", "power", "thermal", "sensors", "storage", "errors", "portainer", "proxmox"];')
html=html.replace('else if (state.tab === "errors") await loadEvents();','else if (state.tab === "proxmox") await window.neofandProxmoxRefresh();\n    else if (state.tab === "errors") await loadEvents();')
html=html.replace('const SECTION_REQS = {',"const SECTION_REQS = {\n  'proxmox-resources': () => !window.neofandHostInfo || Boolean(window.neofandHostInfo.proxmox_available),")
html=html.replace('window.neofandHostConnected?.(d);renderSummary();','window.neofandHostConnected?.(d);renderSummary();applyVisibility();document.body.classList.toggle("pve-host",Boolean(d.proxmox_available));')
html=html.replace('</head>','<script src="/hardware/assets/proxmox.js?v=20261105-drive-alerts" defer></script></head>')
output.write_text(html)

# Default the GPU builder to this host's platform; saved layouts still take precedence.
library=output.parent/'assets/gpu-library/index.html'
if library.exists():
 text=library.read_text()
 text=text.replace('<html lang="en">','<html lang="en" data-platform="lenovo-p620">')
 library.write_text(text)
