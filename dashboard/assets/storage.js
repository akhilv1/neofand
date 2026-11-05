(() => {
  'use strict';
  const make = (tag, cls, text) => { const e=document.createElement(tag); e.className=cls || ''; if(text!==undefined)e.textContent=text; return e; };
  const bytes = n => {
    if (!Number.isFinite(n)) return '—';
    let i=0; while(n>=1024 && i<4){n/=1024;i++;}
    return `${n.toLocaleString(undefined,{maximumFractionDigits: i ? 1 : 0})} ${['B','KiB','MiB','GiB','TiB'][i]}`;
  };
  const flatten = n => [n,...(n.children || []).flatMap(flatten)];
  const filesystemName = n => ({zfs_member:'ZFS member',linux_raid_member:'RAID member',LVM2_member:'LVM member',apfs:'APFS',exfat:'exFAT',ext4:'ext4',xfs:'XFS',vfat:'FAT',ntfs:'NTFS',btrfs:'Btrfs',crypto_LUKS:'LUKS',swap:'Swap'}[n.fstype] || n.fstype || n.parttypename || 'Unknown type');
  function unknownUsage(parent, n) {
    const legend=make('div','storage-values storage-unknown-values');
    legend.append(make('strong','','Unknown storage usage'),make('span','',`${bytes(n.size)} partition`));
    const bar=make('div','storage-category-bar storage-unknown-bar');
    bar.setAttribute('role','img');
    bar.setAttribute('aria-label',`${filesystemName(n)} partition, ${bytes(n.size)} capacity; used and available space unknown`);
    parent.append(legend,bar,make('p','storage-note',n.usage_status==='unmounted'?'Not mounted · used and available space are unknown.':'Filesystem usage cannot be read · used and available space are unknown.'));
  }
  function usage(parent, u, showMeter=true) {
    const pct = u.total_bytes ? Math.min(100,Math.max(0,u.used_bytes/u.total_bytes*100)) : 0;
    const meter=make('div','storage-meter'); meter.setAttribute('role','meter');
    meter.setAttribute('aria-label','Filesystem space used'); meter.setAttribute('aria-valuemin','0'); meter.setAttribute('aria-valuemax','100'); meter.setAttribute('aria-valuenow',pct.toFixed(1));
    const used=make('span',pct>=90?'storage-used high':'storage-used'); used.style.width=`${pct}%`; meter.append(used);
    const reserved=make('span','storage-reserved'); reserved.style.width=`${u.total_bytes ? u.reserved_bytes/u.total_bytes*100 : 0}%`; meter.append(reserved);
    const legend=make('div','storage-values');
    legend.append(make('strong','',`${bytes(u.used_bytes)} used · ${pct.toFixed(1)}%`),make('span','',`${bytes(u.available_bytes)} available`));
    if(showMeter)parent.append(meter);
    parent.append(legend);
    if(u.reserved_bytes) parent.append(make('div','storage-note',`${bytes(u.reserved_bytes)} reserved by filesystem`));
  }
  function header(ok, text) {
    const h=document.getElementById('hdrstate'); h.className=ok?'st good':'st off';
    h.replaceChildren(make('i'),make('span','',text));
  }
  window.neofandStorageRefresh = async () => {
    const host=document.getElementById('storage-drives'), status=document.getElementById('storage-status');
    const controller=new AbortController(), timeout=setTimeout(()=>controller.abort(),8000);
    try {
      const response=await fetch('/hardware/api/host-storage',{cache:'no-store',signal:controller.signal});
      if(!response.ok)throw new Error('Storage readings unavailable');
      const data=await response.json();
      if(!Array.isArray(data.drives)||!Number.isFinite(data.timestamp))throw new Error('Invalid storage snapshot');
      const stale=Date.now()/1000-data.timestamp>60;
      status.textContent=`${stale?'Stale snapshot · ':''}Sampled ${new Date(data.timestamp*1000).toLocaleString()} · host samples every 15s`;
      status.classList.toggle('storage-warning',stale);
      header(!stale,stale?'Storage delayed':'Storage online');
      const totals={raw:0,used:0,available:0}, seen=new Set();
      function renderFilesystem(n){

          const row=make('div','storage-filesystem');
          const partitionHeading=make('div','storage-partition-heading');
          const badge=make('span','storage-type-badge',filesystemName(n));
          badge.title=[n.parttypename,n.parttype].filter(Boolean).join(' · ');
          partitionHeading.append(make('h4','',n.mounts.join(', ') || n.parttypename || 'Data partition'),badge);
          row.append(partitionHeading,make('div','storage-note storage-partition-meta',`${n.device} · ${bytes(n.size)}${n.mounts.length && n.parttypename ? ' · '+n.parttypename : ''}`));
          if(n.usage)row.append(window.neofandStorageIndexStatus(n.filesystem_id,data.index_progress));
          if(n.member_of?.length && !n.usage){
            row.append(make('p','storage-note','Member device · shared usage is shown with its storage group.'));
            const refs=[...(n.child_ids||[]).map(id=>({id,name:(data.volumes||[]).find(v=>v.filesystem_id===id)?.device || id})),...(n.pool_ids||[]).map(id=>({id,name:id.replace(/^zfs:/,'')}))];
            window.neofandStorageGroups.links(row,refs);
          } else if(n.usage){
            const scan=data.breakdown?.filesystems?.find(f=>f.filesystem_id===n.filesystem_id);
            usage(row,n.usage,!scan || Boolean(scan.error));
            row.append(window.neofandStorageBreakdown(n.usage,scan));
            if(!seen.has(n.filesystem_id)){seen.add(n.filesystem_id); totals.used+=n.usage.used_bytes;totals.available+=n.usage.available_bytes;}
          } else unknownUsage(row,n);
          return row;
      }
      const cards=data.drives.map(d=>{
        totals.raw+=d.size || 0;
        const card=make('article','storage-drive');
        const heading=make('div','storage-drive-heading');
        const title=make('div'),titleLine=make('div','storage-title-line');titleLine.append(make('h3','',d.model?.trim() || d.name));window.neofandStorageGroups.badges(titleLine,d.roles);title.append(titleLine,make('div','storage-note',`${d.device} · ${d.tran?.toUpperCase() || (d.name.startsWith('nvme')?'NVMe':'Block device')} · ${d.rota?'HDD':'SSD'}`));
        heading.append(title,make('strong','storage-capacity',bytes(d.size))); card.append(heading);
        window.neofandStorageGroups.links(card,d.memberships);
        const nodes=(d.display_nodes || flatten(d)).filter(n=>!n.auxiliary && (n.type==='part'||n.fstype||n.mounts.length));
        if(!nodes.length){
          if(!d.children?.length)unknownUsage(card,d);
          else card.append(make('p','storage-note','No data partitions to display.'));
        }
        nodes.forEach(n=>card.append(renderFilesystem(n)));
        return card;
      });
      const shared=window.neofandStorageGroups.groups(data,renderFilesystem);
      if(shared.length)cards.unshift(...shared);
      if(data.totals){totals.used=data.totals.used_bytes;totals.available=data.totals.available_bytes;}
      for(const notice of data.notices||[])cards.unshift(make('p','storage-note storage-warning',notice));
      const expanded=new Set([...host.querySelectorAll('details[open]')].map(e=>e.dataset.storageKey));
      host.replaceChildren(...cards);
      window.neofandDecorateStorage?.();
      host.querySelectorAll('details').forEach(e=>{e.open=expanded.has(e.dataset.storageKey);});
      if(!cards.length)host.append(make('p','','No physical drives detected.'));
      const summary=document.getElementById('storage-summary'); summary.replaceChildren();
      for(const [label,value] of [['Physical drives',String(data.drives.length)],['Raw capacity',bytes(totals.raw)],['Accounted used',bytes(totals.used)],['Available (shared space counted once)',bytes(totals.available)]]){
        const item=make('div'); item.append(make('span','storage-note',label),make('strong','',value));summary.append(item);
      }
      window.neofandRuntime?.sample([{ts:data.timestamp}]);
      if(stale)window.neofandRuntime?.error();
    } catch(error) {
      header(false,'Storage unavailable');
      status.textContent='Storage unavailable — refresh to retry.';status.classList.add('storage-warning');
      host.replaceChildren(make('p','empty','Unable to read the host storage snapshot.'));
      document.getElementById('storage-summary').replaceChildren();
      window.neofandRuntime?.error();
    } finally {clearTimeout(timeout);}
  };
})();
