(() => {
 const element=(tag,text,cls)=>{const n=document.createElement(tag);if(text!==undefined)n.textContent=text;if(cls)n.className=cls;return n;};
 const gib=n=>Number.isFinite(n)?(n/1073741824).toFixed(1)+' GiB':'—';
 const pct=n=>Number.isFinite(n)?(n*100).toFixed(1)+'%':'—';
 let snapshot=null, fetchedAt=0;
 async function read(){const r=await fetch('/hardware/api/proxmox',{cache:'no-store'});if(!r.ok)throw Error('Proxmox unavailable');snapshot=await r.json();fetchedAt=Date.now();return snapshot;}
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
 function summaryScope(){const cells=document.querySelectorAll('#storage-summary > div > span');if(cells.length===4){cells[1].textContent='Raw installed · before redundancy';cells[2].textContent='Host-visible used';cells[3].textContent='Host-visible available';}}
 function guestVolumes(d){
  const tab=document.getElementById('tab-storage');if(!tab)return;
  let panel=document.getElementById('pve-guest-storage');if(!panel){panel=element('section',undefined,'storage-drive');panel.id='pve-guest-storage';panel.style.marginTop='18px';tab.insertBefore(panel,document.getElementById('storage-drives'));}
  panel.replaceChildren(element('h3','Guest volume usage'),element('p','Usable filesystem capacity reported by guest agents. Mirrored data is shown once per volume; these readings are separate from the host totals above.','storage-note'));
  const stale=d.status!=='ok'||Date.now()/1000-d.timestamp>180;
  panel.append(element('p',(stale?'Stale readings · ':'Sampled ')+new Date(d.timestamp*1000).toLocaleString(),'storage-note'));
  for(const guest of d.guest_storage||[]){
   const row=element('div',undefined,'storage-filesystem');row.append(element('h4',`${guest.name} · VM ${guest.vmid}`));
   if(guest.status!=='ok')row.append(element('p','Guest agent capacity readings unavailable.','storage-note'));
   for(const fs of guest.filesystems||[]){
    const percent=fs.total_bytes?fs.used_bytes/fs.total_bytes*100:0;
    row.append(element('p',`${fs.mountpoint} · ${fs.filesystem||'filesystem'} · ${(fs.used_bytes/1099511627776).toFixed(2)} TiB used / ${(fs.total_bytes/1099511627776).toFixed(2)} TiB usable · ${percent.toFixed(1)}%`,'storage-values'));
    const meter=element('div',undefined,'storage-meter'),fill=element('span',undefined,'storage-used');fill.style.width=percent+'%';meter.append(fill);meter.setAttribute('role','img');meter.setAttribute('aria-label',`${fs.mountpoint}: ${percent.toFixed(1)} percent used`);row.append(meter);
   }
   panel.append(row);
  }
  if(!d.guest_storage?.length)panel.append(element('p','No guest volume capacity readings available.','storage-note'));
 }

 function mirrorHealth(card,d,pool){
  if(pool!=='rpool')return;
  card.classList.add('pve-boot-card');
  const panel=element('div',undefined,'pve-mirror-health'),health=(d.pool_health||[]).find(p=>p.name===pool);
  if(!health){panel.append(element('p','Mirror health readings unavailable.','storage-note'));card.insertBefore(panel,card.querySelector(':scope > details'));return;}
  const stale=d.status!=='ok'||Date.now()/1000-d.timestamp>180||Date.now()/1000-health.timestamp>180;
  const groups=[];const walk=nodes=>{for(const n of nodes||[]){if(n.name.startsWith('mirror-'))groups.push(`${n.name} · ${n.state||'Unknown'}`);walk(n.children)}};walk(health.topology);
  panel.append(element('p',`${groups.length?'ZFS mirror · '+groups.join(' / '):'ZFS topology'} · ${(health.members||[]).length} member disks${stale?' · Stale readings':''}`,'pve-mirror-title'));
  const disks=element('div',undefined,'pve-mirror-disks');
  for(const member of health.members||[]){const smart=member.smart||{},row=element('section',undefined,'pve-mirror-disk');
   row.append(element('h4',smart.model||member.device));
   row.append(element('p',`${member.device} · ZFS ${member.state||'Unknown'}`,'storage-note'));
   const fmt=(n,suffix='')=>Number.isFinite(n)?n.toLocaleString()+suffix:'—';
   const status={passed:'Passed',warning:'Warning',failed:'Failed',unavailable:'Unavailable'}[smart.state]||'Unavailable';
   row.append(element('p',`SMART ${status} · ${fmt(smart.temperature_c,' °C')} · ${fmt(smart.endurance_used_pct,'%')} endurance used`,smart.state==='passed'&&!stale?'pve-smart-line':'pve-smart-line storage-warning'));
   row.append(element('p',`Media errors ${fmt(smart.media_errors)} · Spare ${fmt(smart.available_spare_pct,'%')} · ${fmt(smart.power_on_hours,' h')} powered on`,'storage-note'));
   if(smart.critical_warning)row.append(element('p',`NVMe critical warning: ${smart.critical_warning}`,'storage-warning'));
   disks.append(row);
  }
  panel.append(disks);
  if(health.scan)panel.append(element('p',health.scan,'storage-note'));
  panel.append(element('p',`SMART sampled ${new Date(d.timestamp*1000).toLocaleTimeString()} · refreshes every minute`,'storage-note'));
  card.insertBefore(panel,card.querySelector(':scope > details'));
 }

 function decorateStorage(d){const tab=document.getElementById('tab-storage');if(!tab||!d)return;
  guestVolumes(d);summaryScope();tab.classList.add('pve-storage');const cards=[...tab.querySelectorAll('#storage-drives > .storage-drive')];
  for(const card of cards){const wasOpen=Boolean(card.querySelector(':scope > .pve-detail-toggle[open]'));for(const old of card.querySelectorAll(':scope > .pve-detail-toggle')){old.replaceWith(...[...old.children].filter(n=>n.tagName!=='SUMMARY'))}card.querySelectorAll('.pve-storage-uses,.pve-host-badge,.pve-mirror-health').forEach(n=>n.remove());
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
   mirrorHealth(card,d,pool);
  }
  const note=tab.querySelector('.storage-intro > .storage-note:last-child');if(note)note.textContent='VM ownership badges are shown only for whole physical disks configured for passthrough. PVE Boot Disk is the host boot pool. Raw installed capacity includes mirror copies; host totals cover filesystems PVE can read. Guest readings appear separately below and are not added to host totals.';
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
 window.neofandStorageSummary=()=>{if(snapshot)summaryScope()};
 const originalStorage=window.neofandStorageRefresh;
 if(originalStorage)window.neofandStorageRefresh=async()=>{if(Date.now()-fetchedAt>30000)try{await read()}catch(_){}await originalStorage()};
 document.addEventListener('DOMContentLoaded',()=>{if(document.getElementById('pve-guest-tiles'))window.neofandProxmoxRefresh()});
})();
