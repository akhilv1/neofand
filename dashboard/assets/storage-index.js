(() => {
  const make=(tag,cls,text)=>{const e=document.createElement(tag);e.className=cls||'';if(text!==undefined)e.textContent=text;return e;};
  window.neofandStorageIndexStatus=(id,data)=>{
    const box=make('div','storage-index-status');
    const record=data?.filesystems?.find(f=>f.filesystem_id===id);
    if(!record){box.append(make('span','storage-note','Index not started · cached breakdown is shown when available.'));return box;}
    const number=n=>(n||0).toLocaleString();
    if(record.state==='ready'){
      box.append(make('span','storage-note',`Indexed ${number(record.files_checked)} files · ${number(record.cached_entries_reused)} cached entries reused`));return box;
    }
    const stale=Date.now()/1000-data.timestamp>120;
    const active=record.state==='indexing'&&!stale;
    const title=record.state==='queued'?'Queued for indexing':active?'Indexing storage':record.state==='paused'?'Index paused · resumes automatically':'Index update delayed';
    box.append(make('strong','',title));
    if(active){const progress=make('progress','storage-index-progress');progress.setAttribute('aria-label','Storage indexing in progress; total files not yet known');box.append(progress);}
    box.append(make('span','storage-note',`${number(record.files_checked)} files checked · ${number(record.folders_pending)} discovered folders remaining · ${number(record.cached_entries_reused)} cache hits`));
    box.append(make('span','storage-note','Showing the last completed breakdown while the metadata index updates.'));
    return box;
  };
})();
