(() => {
 'use strict';
 const make=(tag,text,cls)=>{const n=document.createElement(tag);if(text!==undefined)n.textContent=text;if(cls)n.className=cls;return n;};
 const bytes=n=>{if(!Number.isFinite(n))return '—';let i=0;while(n>=1024&&i<4){n/=1024;i++;}return n.toFixed(i?1:0)+' '+['B','KiB','MiB','GiB','TiB'][i];};
 let pending=null;
 window.neofandPortainerAvailable=null;
 function render(d){
  const target=document.getElementById('portainer-content');if(!target)return;
  const stale=d.status!=='ok'||Date.now()/1000-d.timestamp>90,rows=d.containers||[];
  window.neofandPortainerAvailable=true;
  if(typeof state!=='undefined'&&state.tab==='portainer'){
   if(d.timestamp)window.neofandRuntime?.sample([{ts:d.timestamp}]);
   if(stale)window.neofandRuntime?.error();
   const header=document.getElementById('hdrstate');
   if(header){header.textContent=stale?'Container readings delayed':'Container telemetry connected';header.className=stale?'warn':'good';}
  }
  const stamp=document.getElementById('portainer-status');stamp.textContent=(stale?'Delayed · retained snapshot':'Local Docker inventory')+' · '+(d.timestamp?new Date(d.timestamp*1000).toLocaleString():'No successful sample');
  const link=document.getElementById('portainer-open');link.hidden=!d.portainer_detected;
  target.replaceChildren();
  const running=rows.filter(c=>c.state==='running'),measured=running.filter(c=>Number.isFinite(c.memory_used_bytes));
  const projects=new Map();for(const c of rows)if(c.project)projects.set(c.project,(projects.get(c.project)||0)+1);
  const summary=make('div',undefined,'portainer-summary');
  for(const [label,value,note] of [['Containers',rows.length,`${running.length} running · ${rows.length-running.length} other`],['Compose projects',projects.size,'Grouped by project label'],['Unhealthy',rows.filter(c=>c.health==='unhealthy').length,'Only configured health checks'],['Container RAM',measured.length?bytes(measured.reduce((sum,c)=>sum+c.memory_used_bytes,0)):'—',`${measured.length}/${running.length} running containers reporting`]]){
   const box=make('div',undefined,'portainer-total');box.append(make('span',label),make('strong',String(value)),make('span',note));summary.append(box);
  }
  target.append(summary);
  if(projects.size){const chips=make('div',undefined,'portainer-projects');for(const [name,count] of [...projects].sort((a,b)=>a[0].localeCompare(b[0])))chips.append(make('span',`${name} · ${count}`));target.append(make('h3','Compose projects','portainer-section-title'),chips);}
  target.append(make('h3','Containers','portainer-section-title'));
  const grid=make('div',undefined,'portainer-grid');
  for(const c of [...rows].sort((a,b)=>(a.state==='running'?0:1)-(b.state==='running'?0:1)||a.name.localeCompare(b.name))){
   const card=make('article',undefined,'portainer-container'),badges=make('div',undefined,'portainer-badges');
   badges.append(make('span',c.state,'portainer-badge '+(c.state==='running'?'good':c.state==='restarting'||c.state==='dead'?'bad':'')));
   if(c.health)badges.append(make('span',c.health,'portainer-badge '+(c.health==='healthy'?'good':c.health==='unhealthy'?'bad':'warn')));
   card.append(badges,make('h3',c.name),make('div',c.project||'Standalone container','portainer-project'));
   const stats=make('div',undefined,'portainer-stats');
   for(const [label,value] of [['CPU · Docker %',Number.isFinite(c.cpu_pct)?c.cpu_pct.toFixed(1)+'%':'—'],['RAM in use',bytes(c.memory_used_bytes)]]){const n=make('div');n.append(make('span',label),make('strong',value));stats.append(n);}
   card.append(stats,make('div',c.status,'portainer-note'));
   const detail=make('details');detail.append(make('summary','Image & resource details'));
   for(const text of [`Image: ${c.image}`,`Restarts: ${c.restart_count??'Unavailable'}`,`Memory limit: ${c.memory_limit_bytes>0?bytes(c.memory_limit_bytes):c.memory_limit_bytes===0?'Not explicitly limited':'Unavailable'}`,`Network RX / TX: ${c.network_io||'Unavailable'}`,`Disk read / write: ${c.block_io||'Unavailable'}`])detail.append(make('p',text));
   card.append(detail);grid.append(card);
  }
  if(!rows.length)grid.append(make('p','No containers found.','portainer-note'));target.append(grid);
  target.append(make('p',`Docker ${d.engine_version||'version unavailable'} · CPU can exceed 100% across cores. Memory excludes reclaimable cache; network and disk I/O are cumulative counters.`, 'portainer-status'));
  for(const warning of d.warnings||[])target.append(make('p',warning,'portainer-status'));
  if(d.truncated)target.append(make('p','Inventory limited to the first 128 containers.','portainer-status'));
 }
 window.neofandPortainerRefresh=()=>{
  if(pending)return pending;
  pending=(async()=>{
   const controller=new AbortController(),timer=setTimeout(()=>controller.abort(),5000);
   try{const r=await fetch('/hardware/api/portainer',{cache:'no-store',signal:controller.signal});if(!r.ok)throw Error();const d=await r.json();render(d);}
   catch(_){if(typeof state!=='undefined'&&state.tab==='portainer'){window.neofandRuntime?.error();const header=document.getElementById('hdrstate');if(header){header.textContent='Container readings unavailable';header.className='warn';}}if(window.neofandPortainerAvailable===null)window.neofandPortainerAvailable=false;const status=document.getElementById('portainer-status');if(status)status.textContent='Container snapshot unavailable. Enable the host Docker collector to populate this page.';}
   finally{clearTimeout(timer);pending=null;if(typeof applyVisibility==='function')applyVisibility();}
  })();return pending;
 };
 document.addEventListener('DOMContentLoaded',()=>window.neofandPortainerRefresh());
})();
