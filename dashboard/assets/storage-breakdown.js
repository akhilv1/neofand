(() => {
  'use strict';
  const colors={'Models':'#8b70dd','Photos':'#e8b849','Videos':'#df775d','Audio':'#d979b6','Backups':'#62a7c3','Archives':'#af976c','Apps & Developer':'#568bd5','Containers':'#4eaaa4','Caches & Logs':'#91a36a','Documents':'#b891cf','System':'#7f8ba4','Other':'#a8a8ae','Unclassified & overhead':'#bfc5c6','Reserved':'#d5d9da','Available':'#e9eeec'};
  const make=(tag,cls,text)=>{const el=document.createElement(tag);el.className=cls||'';if(text!==undefined)el.textContent=text;return el;};
  const bytes=n=>{let i=0;while(n>=1024&&i<4){n/=1024;i++;}return `${n.toLocaleString(undefined,{maximumFractionDigits:i?1:0})} ${['B','KiB','MiB','GiB','TiB'][i]}`;};
  function folderList(folders){
    const list=make('ul','storage-folder-list');
    for(const f of folders||[]){const li=make('li');li.append(make('span','',f.path),make('strong','',bytes(f.bytes)));list.append(li);}
    return list;
  }
  function breakdown(u, scan){
    const box=make('div','storage-breakdown');
    if(!scan||scan.error){box.append(make('p','storage-note','Category scan pending or unavailable. Space totals above are current.'));return box;}
    const categories=(scan.categories||[]).filter(c=>c.bytes>0).sort((a,b)=>b.bytes-a.bytes);
    const allocated=categories.reduce((n,c)=>n+c.bytes,0);
    const remainder=Math.max(0,u.used_bytes-allocated);
    const segments=[...categories];
    if(remainder)segments.push({name:'Unclassified & overhead',bytes:remainder});
    if(u.reserved_bytes)segments.push({name:'Reserved',bytes:u.reserved_bytes});
    segments.push({name:'Available',bytes:u.available_bytes});
    const total=segments.reduce((n,c)=>n+c.bytes,0);
    const bar=make('div','storage-category-bar');bar.setAttribute('role','img');
    bar.setAttribute('aria-label',segments.map(c=>`${c.name}: ${bytes(c.bytes)}`).join('; '));
    for(const c of segments){const item=make('span');item.style.background=colors[c.name]||colors.Other;item.style.width=`${total?c.bytes/total*100:0}%`;item.title=`${c.name} · ${bytes(c.bytes)}`;bar.append(item);}
    box.append(bar);
    const legend=make('div','storage-category-legend');
    for(const c of segments){
      const row=make(c.folders?.length?'details':'div','storage-category');
      row.dataset.storageKey=`${scan.filesystem_id}:${c.name}`;
      const line=make(c.folders?.length?'summary':'div','storage-category-line');
      const dot=make('i');dot.style.background=colors[c.name]||colors.Other;
      line.append(dot,make('span','',c.name),make('strong','',bytes(c.bytes)));
      row.append(line);
      if(c.folders?.length){row.append(make('p','storage-note','Largest contributing folders'),folderList(c.folders));}
      legend.append(row);
    }
    box.append(legend);
    const age=Date.now()/1000-scan.timestamp;
    const stamp=make('p','storage-note',`${age>5400?'Outdated scan · ':''}${scan.complete?'Scanned':'Partial scan'} ${new Date(scan.timestamp*1000).toLocaleString()} · ${(scan.files_scanned||0).toLocaleString()} files · updates every 30 minutes`);
    if(age>5400||!scan.complete)stamp.classList.add('storage-warning');box.append(stamp);
    if(scan.unreadable_entries)box.append(make('p','storage-note',`${scan.unreadable_entries.toLocaleString()} entries could not be read. Unclassified space includes protected files and filesystem overhead.`));
    if(allocated>u.used_bytes)box.append(make('p','storage-note storage-warning','Allocated file estimates exceed current filesystem usage. Shared extents or files changing during the scan can cause this; segment proportions are estimates.'));
    if(scan.largest_folders?.length){const details=make('details','storage-largest');details.dataset.storageKey=`${scan.filesystem_id}:largest`;details.append(make('summary','','Largest folders on this filesystem'),folderList(scan.largest_folders));box.append(details);}
    return box;
  }
  window.neofandStorageBreakdown=breakdown;
})();
