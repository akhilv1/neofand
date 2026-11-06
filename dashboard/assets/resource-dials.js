(() => {
  'use strict';
  const finite=Number.isFinite;
  const ratio=(used,total)=>finite(used)&&finite(total)&&used>=0&&total>0?Math.min(100,Math.max(0,used/total*100)):null;
  const bytes=value=>{if(!finite(value))return '—';let i=0;while(value>=1024&&i<4){value/=1024;i++;}return `${value.toFixed(i?1:0)} ${['B','KiB','MiB','GiB','TiB'][i]}`;};
  const quantity=(used,total)=>`${bytes(used)} / ${bytes(total)}`;
  function models(latest,memory,storage,proxmox,info={}) {
    const metric=(sub,name)=>latest.find(m=>m.subsystem===sub&&m.name===name)?.value;
    const ts=Math.max(0,...latest.map(m=>m.ts).filter(finite));
    const cpu=metric('host','cpu_pct');
    const totals=storage?.totals;
    const result=[
      {id:'cpu',title:'CPU',value:finite(cpu)?cpu:null,detail:'Total processor utilization',ts,kind:'activity'},
      {id:'memory',title:'Memory',value:ratio(memory?.used_bytes,memory?.total_bytes),detail:quantity(memory?.used_bytes,memory?.total_bytes),note:'In use · excludes reclaimable cache',ts:memory?.timestamp,source:'memory'},
      {id:'storage',title:info.proxmox_available?'Host storage':'Storage',value:ratio(totals?.used_bytes,finite(totals?.used_bytes)&&finite(totals?.available_bytes)?totals.used_bytes+totals.available_bytes:null),detail:finite(totals?.used_bytes)?`${bytes(totals.used_bytes)} used · ${bytes(totals.available_bytes)} free`:'Usage unavailable',note:'Mounted filesystems · host-visible capacity',ts:storage?.timestamp,source:'storage',maxAge:90}
    ];
    if(info.proxmox_available){
      const guests=proxmox?.guest_storage||[],seen=new Set();let used=0,total=0,reporting=0;
      for(const guest of guests){if(guest.status!=='ok')continue;let included=false;for(const fs of guest.filesystems||[]){const key=`${guest.vmid}:${fs.mountpoint}`;if(seen.has(key)||!finite(fs.used_bytes)||!finite(fs.total_bytes)||fs.total_bytes<=0||fs.used_bytes<0||fs.used_bytes>fs.total_bytes)continue;seen.add(key);used+=fs.used_bytes;total+=fs.total_bytes;included=true;}if(included)reporting++;}
      result.push({id:'guest-storage',title:'Guest storage',value:ratio(used,total),detail:total?quantity(used,total):'Guest usage unavailable',note:`${reporting} of ${guests.length} passthrough guests reporting · mirrors counted once`,ts:proxmox?.timestamp,source:'proxmox',maxAge:180});
      const all=proxmox?.guests,active=all?.filter(g=>g.status==='running').length;
      result.push({id:'guests',title:'VMs & LXCs',value:Array.isArray(all)&&all.length?active/all.length*100:null,display:Array.isArray(all)?`${active} / ${all.length}`:'—',detail:'Guests running',note:Array.isArray(all)?`${all.filter(g=>g.type==='qemu').length} VMs · ${all.filter(g=>g.type==='lxc').length} LXCs`:'Inventory unavailable',ts:proxmox?.timestamp,source:'proxmox',maxAge:180,kind:'activity'});
    }else{
      const load=latest.filter(m=>m.subsystem==='gpu'&&/_util_gpu_pct$/.test(m.name)&&finite(m.value));
      if(load.length||(info.gpus||[]).length){
        let used=0,total=0,count=0;
        for(const row of latest.filter(m=>m.subsystem==='gpu'&&/_mem_total_bytes$/.test(m.name))){const amount=metric('gpu',row.name.replace('_total_','_used_'));if(finite(row.value)&&row.value>0&&finite(amount)&&amount>=0){used+=amount;total+=row.value;count++;}}
        result.splice(2,0,{id:'gpu',title:'GPU compute',value:load.length?load.reduce((sum,m)=>sum+m.value,0)/load.length:null,detail:`Average across ${load.length} GPU${load.length===1?'':'s'}`,ts,kind:'activity'},
          {id:'vram',title:'GPU memory',value:ratio(used,total),detail:total?quantity(used,total):'VRAM unavailable',note:`Combined across ${count} GPU${count===1?'':'s'}`,ts});
      }
    }
    return result;
  }
  if(typeof module!=='undefined'&&module.exports){module.exports={models,ratio};return;}
  const make=(tag,cls,text)=>{const n=document.createElement(tag);if(cls)n.className=cls;if(text!==undefined)n.textContent=text;return n;};
  const ns='http://www.w3.org/2000/svg';
  const svg=(tag,attrs)=>{const n=document.createElementNS(ns,tag);for(const [key,value] of Object.entries(attrs))n.setAttribute(key,value);return n;};
  const cache={};let busy=false,last=[],section,grid;
  async function read(key,path,age){
    const item=cache[key];if(item&&Date.now()-item.attempt<age)return;
    const next=cache[key]={...item,attempt:Date.now()};
    const controller=new AbortController(),timer=setTimeout(()=>controller.abort(),5000);
    try{const response=await fetch(path,{cache:'no-store',signal:controller.signal});if(!response.ok)throw Error();next.data=await response.json();next.failed=false;}catch(_){next.failed=true;}finally{clearTimeout(timer);}
  }
  function paint(){
    const overview=document.getElementById('tab-overview');if(!overview)return;
    if(!section){section=make('section','resource-overview card wide');section.setAttribute('aria-label','Current system resources');const heading=make('div','resource-heading');heading.append(make('h2','','System resources'),make('span','resource-caption','Current readings · independent of chart range'));grid=make('div','resource-dials');section.append(heading,grid);overview.prepend(section);}
    const data=models(last,cache.memory?.data,cache.storage?.data,cache.proxmox?.data,window.neofandHostInfo||{});
    const now=Date.now()/1000;
    grid.replaceChildren(...data.map(d=>{
      const missing=!finite(d.value),stale=!missing&&(!finite(d.ts)||now-d.ts>(d.maxAge||30)||cache[d.source]?.failed);
      const card=make('article','resource-dial');card.dataset.status=missing?'unknown':stale?'stale':d.kind==='activity'?'normal':d.value>=95?'high':d.value>=85?'warm':'normal';
      card.append(make('h3','',d.title));
      const face=make('div','resource-face');const art=svg('svg',{viewBox:'0 0 180 150','aria-hidden':'true'});
      // 270-degree arc, leaving breathing room below the number.
      for(const cls of ['dial-track','dial-fill']){const p=svg('path',{d:'M 39.09 130.91 A 72 72 0 1 1 140.91 130.91',pathLength:'100',class:cls});if(cls==='dial-fill')p.setAttribute('stroke-dasharray',`${missing?0:d.value} 100`);art.append(p);}
      const value=make('strong','dial-value',d.display??(missing?'—':`${d.value.toFixed(0)}%`));face.append(art,value);
      const reading=make('div','dial-detail',d.detail);const note=make('div','dial-note',missing?'No reading available':stale?'Delayed · retained reading':d.note||'Latest sample');
      face.setAttribute('role','img');face.setAttribute('aria-label',`${d.title}: ${value.textContent}. ${d.detail}. ${note.textContent}`);
      card.title=finite(d.ts)?`Sampled ${new Date(d.ts*1000).toLocaleString()}`:'No sample';card.append(face,reading,note);return card;
    }));
  }
  window.neofandResourcesUpdate=async latest=>{
    last=latest||[];
    if(document.getElementById('tab-overview')?.hidden||busy)return;
    paint();busy=true;
    try{await Promise.all([read('memory','/hardware/api/host-memory',2000),read('storage','/hardware/api/host-storage',30000),...(window.neofandHostInfo?.proxmox_available?[read('proxmox','/hardware/api/proxmox',30000)]:[])]);paint();}finally{busy=false;if(window.neofandHostInfo?.proxmox_available&&!cache.proxmox)window.neofandResourcesUpdate(last);}
  };
  document.addEventListener('DOMContentLoaded',()=>{if(typeof state!=='undefined')window.neofandResourcesUpdate(state.latest);});
})();
