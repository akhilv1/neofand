window.neofandProxmoxRefresh=async()=>{
 const target=document.getElementById('proxmox-resources'),stamp=document.getElementById('proxmox-status');
 const element=(tag,text)=>{const n=document.createElement(tag);if(text!==undefined)n.textContent=text;return n;};
 const gib=n=>Number.isFinite(n)?(n/1073741824).toFixed(1)+' GiB':'—';
 const pct=n=>Number.isFinite(n)?(n*100).toFixed(1)+' %':'—';
 const draw=(headers,rows)=>{const table=element('table'),head=element('thead'),body=element('tbody'),tr=element('tr');for(const h of headers)tr.append(element('th',h));head.append(tr);for(const row of rows){const r=element('tr');for(const cell of row)r.append(element('td',cell??'—'));body.append(r);}table.append(head,body);return table;};
 try{
  const response=await fetch('/hardware/api/proxmox',{cache:'no-store'});if(!response.ok)throw Error();const d=await response.json();
  const stale=d.status!=='ok'||Date.now()/1000-d.timestamp>180;
  stamp.textContent=(stale?'Retained snapshot · ':'')+d.node+' · '+new Date(d.timestamp*1000).toLocaleString();
  target.replaceChildren();
  const h=d.host||{};target.append(draw(['Host','Value'],[['Proxmox',h.pveversion],['CPU',pct(h.cpu)],['RAM used / total',gib(h.memory?.used)+' / '+gib(h.memory?.total)],['Root filesystem used / total',gib(h.rootfs?.used)+' / '+gib(h.rootfs?.total)]]));
  target.append(element('h3','VMs & containers'),draw(['ID','Name','Type','State','CPU','RAM used / limit'],(d.guests||[]).map(g=>[g.vmid,g.name,g.type==='qemu'?'VM':'LXC',g.status,pct(g.cpu),gib(g.mem)+' / '+gib(g.maxmem)])));
  target.append(element('h3','Recent Proxmox tasks'),draw(['Started','Task','Guest ID','Result'],(d.tasks||[]).map(t=>[t.starttime?new Date(t.starttime*1000).toLocaleString():'—',t.type,t.id,t.status||'Running'])));
 }catch(_){stamp.textContent='Proxmox snapshot unavailable.';}
};
