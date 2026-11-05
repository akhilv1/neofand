(() => {
  'use strict';
  const el=(tag,cls,text)=>{const e=document.createElement(tag);e.className=cls||'';if(text!==undefined)e.textContent=text;return e;};
  const bytes=n=>{if(!Number.isFinite(n))return 'Unknown';let i=0;while(n>=1024&&i<4){n/=1024;i++;}return `${n.toLocaleString(undefined,{maximumFractionDigits:1})} ${['B','KiB','MiB','GiB','TiB'][i]}`;};
  const key=id=>'storage-group-'+encodeURIComponent(id);
  function badges(parent, roles){for(const role of roles||[]){const b=el('span','storage-role-badge',role);b.title=role==='System'?'Contains the running Linux root filesystem':'Contains a currently mounted boot or EFI filesystem';parent.append(b);}}
  function links(parent,refs){const list=el('div','storage-memberships');for(const ref of refs||[]){const link=el('a','storage-membership',ref.name);link.href='#'+key(ref.id);link.addEventListener('click',e=>{e.preventDefault();document.getElementById(key(ref.id))?.scrollIntoView({behavior:'smooth',block:'center'});});list.append(link);}parent.append(list);}
  function meter(parent,used,available,label){
    if(!Number.isFinite(used)||!Number.isFinite(available)){parent.append(el('p','storage-note','Capacity readings unavailable'));return;}
    const bar=el('div','storage-meter');const fill=el('span','storage-used');fill.style.width=`${used+available?used/(used+available)*100:0}%`;bar.append(fill);bar.setAttribute('role','img');bar.setAttribute('aria-label',`${label}: ${bytes(used)} used, ${bytes(available)} available`);
    parent.append(bar,el('p','storage-note',`${label} · ${bytes(used)} used · ${bytes(available)} available`));
  }
  function groups(data,renderFilesystem){
    const cards=[];
    for(const volume of data.volumes||[]){
      const card=el('article','storage-drive storage-group');card.id=key(volume.filesystem_id);
      const head=el('div','storage-drive-heading'),title=el('div'),line=el('div','storage-title-line');
      const kind=volume.type==='lvm'?'LVM logical volume':volume.type.startsWith('raid')||volume.type==='md'?'Linux RAID':volume.type==='crypt'?'Encrypted volume':'Logical volume';
      line.append(el('h3','',volume.device));badges(line,volume.roles);title.append(line,el('p','storage-note',`${kind} · ${volume.raid?.level || volume.type} · backing drives: ${volume.backing_drives.join(', ') || 'unresolved'}`));
      head.append(title,el('strong','storage-capacity',bytes(volume.size)));card.append(head);
      if(volume.type==='lvm')card.append(el('p','storage-note','Usage belongs to this filesystem. Unallocated volume-group space and thin-pool headroom are not included.'));
      if(volume.raid){const r=volume.raid;card.append(el('p',r.degraded && r.degraded!=='0'?'storage-note storage-warning':'storage-note',`Array: ${r.array_state || 'unknown'} · missing members: ${r.degraded ?? 'unknown'} · sync: ${r.sync_action || 'unknown'}`));}
      if(volume.member_of?.length&&!volume.usage){card.append(el('p','storage-note','Storage is provided to the logical volumes below. Usage is counted at the mounted filesystem.'));links(card,volume.child_ids.map(id=>({id,name:(data.volumes||[]).find(v=>v.filesystem_id===id)?.device || id})));}
      else card.append(renderFilesystem(volume));
      cards.push(card);
    }
    for(const pool of data.pools||[]){
      const card=el('article','storage-drive storage-group');card.id=key(pool.id);
      const line=el('div','storage-title-line');line.append(el('h3','',pool.name),el('span','storage-type-badge','ZFS pool'));badges(line,pool.roles);
      const health=el('span','storage-pool-health',pool.health);if(pool.health!=='ONLINE')health.classList.add('storage-warning');line.append(health);card.append(line);
      if(pool.accounting)meter(card,pool.accounting.used_bytes,pool.accounting.available_bytes,'Pool dataset space (shared once)');
      card.append(el('p','storage-note',`Pool allocation: ${bytes(pool.allocated_bytes)} allocated / ${bytes(pool.physical_size_bytes)} · ${bytes(pool.free_bytes)} unallocated. Pool allocation differs from usable dataset space.`));
      const tree=el('div','storage-topology');
      function nodes(items,depth=0){for(const n of items){const row=el('div','storage-topology-row');row.style.paddingLeft=`${depth*16}px`;row.append(el('span','',n.name),el('span','storage-note',n.state || ''));tree.append(row);nodes(n.children||[],depth+1);}}
      nodes(pool.topology);card.append(tree);
      if(!pool.topology.length)card.append(el('p','storage-note','Vdev topology unavailable.'));
      if(pool.scan)card.append(el('p','storage-note',pool.scan));
      card.append(el('p','storage-note','Datasets share pool capacity. Their available space and parent used totals are not added together.'));
      for(const ds of pool.datasets){
        const section=el('div','storage-filesystem'),heading=el('div','storage-title-line');heading.append(el('h4','',ds.name),el('span','storage-type-badge',ds.type==='volume'?'ZFS zvol':'ZFS dataset'));badges(heading,ds.roles);section.append(heading);
        section.append(el('p','storage-note',`${ds.mounts.join(', ') || 'Not mounted'} · ${bytes(ds.referenced_bytes)} referenced · ${bytes(ds.snapshot_used_bytes)} snapshots · ${bytes(ds.available_bytes)} available (shared)`));
        // Dataset referenced space and availability are context, never a second pool total.
        if(ds.filesystem_id)section.append(window.neofandStorageIndexStatus(ds.filesystem_id,data.index_progress));
        const scan=data.breakdown?.filesystems?.find(f=>f.filesystem_id===ds.filesystem_id);
        if(scan){const detail=el('details','storage-largest');detail.dataset.storageKey=`${pool.id}:${ds.name}`;detail.append(el('summary','','File categories for this dataset'));
          detail.append(window.neofandStorageBreakdown({used_bytes:ds.referenced_bytes||0,available_bytes:ds.available_bytes||0,total_bytes:(ds.referenced_bytes||0)+(ds.available_bytes||0),reserved_bytes:0},scan));
          detail.append(el('p','storage-note','File estimates may differ from referenced space because of compression, snapshots, or shared blocks.'));section.append(detail);}
        card.append(section);
      }
      cards.push(card);
    }
    return cards;
  }
  window.neofandStorageGroups={groups,badges,links};
})();
