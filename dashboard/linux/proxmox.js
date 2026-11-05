(() => {
 const element=(tag,text,cls)=>{const n=document.createElement(tag);if(text!==undefined)n.textContent=text;if(cls)n.className=cls;return n;};
 const gib=n=>Number.isFinite(n)?(n/1073741824).toFixed(1)+' GiB':'—';
 const pct=n=>Number.isFinite(n)?(n*100).toFixed(1)+'%':'—';
 let snapshot=null;
 async function read(){const r=await fetch('/hardware/api/proxmox',{cache:'no-store'});if(!r.ok)throw Error('Proxmox unavailable');snapshot=await r.json();return snapshot;}
 function table(headers,rows){const t=element('table'),head=element('thead'),body=element('tbody'),tr=element('tr');headers.forEach(h=>tr.append(element('th',h)));head.append(tr);rows.forEach(row=>{const r=element('tr');row.forEach(c=>r.append(element('td',c??'—')));body.append(r)});t.append(head,body);return t;}
 function tiles(d){const target=document.getElementById('pve-guest-tiles');if(!target)return;target.replaceChildren();const guests=d.guests||[];
  document.getElementById('pve-guest-count').textContent=`${guests.filter(g=>g.type==='qemu').length} VMs · ${guests.filter(g=>g.type==='lxc').length} LXCs`;
  for(const g of guests){const card=element('article',undefined,'pve-guest-tile');const top=element('div',undefined,'pve-tile-top');
   top.append(element('span',g.type==='qemu'?'VM':'LXC','pve-kind'),element('span',g.status||'unknown','pve-state '+(g.status==='running'?'running':'')));
   card.append(top,element('h3',g.name||'Guest '+g.vmid),element('div','#'+g.vmid,'pve-guest-id'));
   const stats=element('div',undefined,'pve-tile-stats');stats.append(element('span','CPU '+pct(g.cpu)),element('span','RAM '+gib(g.mem)+' / '+gib(g.maxmem)));card.append(stats);
   target.append(card);
  }
 }
 function badges(parent,uses){const box=element('div',undefined,'pve-storage-uses');for(const u of uses){const b=element('span',`${u.type==='qemu'?'VM':'LXC'} ${u.vmid} · ${u.name}${u.status==='stopped'?' (stopped)':''}`,'pve-owner-badge');b.title=u.relation==='passthrough'?(u.status==='running'?'Physical drive passed to running guest':'Physical passthrough configured for a stopped guest'):'Guest virtual disk on this shared pool';box.append(b)}if(uses.length)parent.append(box);}
 function decorateStorage(d){const tab=document.getElementById('tab-storage');if(!tab||!d)return;
  tab.classList.add('pve-storage');const cards=[...tab.querySelectorAll('#storage-drives > .storage-drive')];
  for(const card of cards){const wasOpen=Boolean(card.querySelector(':scope > .pve-detail-toggle[open]'));for(const old of card.querySelectorAll(':scope > .pve-detail-toggle')){old.replaceWith(...[...old.children].filter(n=>n.tagName!=='SUMMARY'))}card.querySelectorAll('.pve-storage-uses,.pve-host-badge').forEach(n=>n.remove());
   const title=card.querySelector(':scope > .storage-drive-heading, :scope > .storage-title-line');if(!title)continue;const ownerContainer=title.classList.contains('storage-drive-heading')?title.firstElementChild:title;
   const meta=title.querySelector('.storage-note')?.textContent||'';
   const device=meta.match(/\/dev\/[a-z0-9]+/)?.[0];
   const pool=card.id?.startsWith('storage-group-zfs%3A')?decodeURIComponent(card.id.slice('storage-group-'.length)).replace(/^zfs:/,''):null;
   const uses=(d.storage_uses||[]).filter(u=>!pool && u.target===device && u.type==='qemu' && u.relation==='passthrough' && u.whole_disk===true);
   if(pool==='rpool')title.querySelector('h3').textContent='PVE Boot Disk';
   if(card.id && !pool && meta.includes('backing drives:') && uses.some(u=>u.relation==='passthrough')){card.style.display='none';continue}
   badges(ownerContainer,uses);
   if(device&&!uses.length&&!pool){const shared=card.querySelector('.storage-membership')?.textContent;const hint=element('span',shared==='rpool'?'PVE Boot Disk':shared?'Shared · '+shared:'Host / unassigned','pve-host-badge');ownerContainer.append(hint)}
   const children=[...card.children].filter(n=>n!==title);
   if(children.length){const detail=element('details',undefined,'pve-detail-toggle');detail.dataset.storageKey=card.id||device||pool||'';detail.open=wasOpen;detail.append(element('summary','Partitions, datasets & usage'),...children);card.append(detail)}
  }
  const note=tab.querySelector('.storage-intro > .storage-note:last-child');if(note)note.textContent='VM ownership badges are shown only for whole physical disks configured for passthrough. PVE Boot Disk is the host boot pool. Usage inside a guest is not visible to this host.';
 }
 window.neofandProxmoxRefresh=async()=>{
  const target=document.getElementById('proxmox-resources'),stamp=document.getElementById('proxmox-status');
  try{const d=await read();const stale=d.status!=='ok'||Date.now()/1000-d.timestamp>180;
   if(stamp)stamp.textContent=(stale?'Retained snapshot · ':'')+d.node+' · '+new Date(d.timestamp*1000).toLocaleString();
   tiles(d);if(target){target.replaceChildren();const h=d.host||{};
    target.append(table(['Host','Value'],[['Proxmox',h.pveversion],['CPU',pct(h.cpu)],['RAM used / total',gib(h.memory?.used)+' / '+gib(h.memory?.total)],['Root filesystem used / total',gib(h.rootfs?.used)+' / '+gib(h.rootfs?.total)]]));
    target.append(element('h3','VMs & containers'),table(['ID','Name','Type','State','CPU','RAM used / limit'],(d.guests||[]).map(g=>[g.vmid,g.name,g.type==='qemu'?'VM':'LXC',g.status,pct(g.cpu),gib(g.mem)+' / '+gib(g.maxmem)])));
    target.append(element('h3','Recent Proxmox tasks'),table(['Started','Task','Guest ID','Result'],(d.tasks||[]).map(t=>[t.starttime?new Date(t.starttime*1000).toLocaleString():'—',t.type,t.id,t.status||'Running'])));}
   decorateStorage(d);
  }catch(_){if(stamp)stamp.textContent='Proxmox snapshot unavailable.';}
 };
 window.neofandDecorateStorage=()=>decorateStorage(snapshot);
 document.addEventListener('DOMContentLoaded',()=>{if(document.getElementById('pve-guest-tiles'))window.neofandProxmoxRefresh()});
})();
