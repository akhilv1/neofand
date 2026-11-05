(() => {
  const node=id=>document.getElementById('p620-'+id);
  const set=(id,value)=>{const n=node(id);if(n)n.textContent=value;};
  const number=(v,unit='',digits=0)=>Number.isFinite(v)?v.toFixed(digits)+unit:'Unavailable';
  const gib=v=>number(Number.isFinite(v)?v/1073741824:null,' GiB',1);
  // Anchors identify approximate component regions, not exact probe locations.
  const visual=document.querySelector('.p620-visual');
  const chassis=document.querySelector('.p620-base');
  const ns='http://www.w3.org/2000/svg';
  const anchors=[['cpu',.43,.28,'01'],['memory',.57,.41,'02'],['gpu',.455,.60,'03']];
  const overlay=document.createElementNS(ns,'svg');
  overlay.classList.add('p620-connectors');overlay.setAttribute('aria-hidden','true');
  if(visual)visual.append(overlay);
  let frame=0;
  function drawConnectors(){
    frame=0;
    if(!visual||!chassis)return;
    if(document.querySelector('.p620-chassis').hidden){overlay.replaceChildren();return;}
    const area=visual.getBoundingClientRect(),image=chassis.getBoundingClientRect();
    if(!area.width||!area.height||!image.width)return;
    overlay.setAttribute('viewBox',`0 0 ${area.width} ${area.height}`);
    overlay.replaceChildren();
    for(const [id,fx,fy,label] of anchors){
      const card=visual.querySelector(`[data-sensor="${id}"]`);if(!card)continue;
      const box=card.getBoundingClientRect();
      const x=image.left-area.left+image.width*fx,y=image.top-area.top+image.height*fy;
      const endX=box.left-area.left,endY=box.top-area.top+box.height/2;
      const elbow=endX-12;
      const path=document.createElementNS(ns,'path');
      path.setAttribute('d',`M ${x} ${y} H ${elbow} V ${endY} H ${endX}`);
      overlay.append(path);
      const dot=document.createElementNS(ns,'circle');
      dot.setAttribute('cx',x);dot.setAttribute('cy',y);dot.setAttribute('r','12');overlay.append(dot);
      const text=document.createElementNS(ns,'text');
      text.setAttribute('x',x);text.setAttribute('y',y);text.textContent=label;overlay.append(text);
    }
  }
  const scheduleConnectors=()=>{if(!frame)frame=requestAnimationFrame(drawConnectors);};
  if(visual&&chassis){
    if(window.ResizeObserver){const observer=new ResizeObserver(scheduleConnectors);observer.observe(visual);observer.observe(chassis);for(const card of visual.querySelectorAll('.p620-callout'))observer.observe(card);}
    chassis.addEventListener('load',scheduleConnectors);window.addEventListener('resize',scheduleConnectors);scheduleConnectors();
  }
  window.neofandHostConnected=info=>{
    set('host-title',info.host.product_family||info.host.product_version||'Linux host');
    set('host-detail',info.host.hostname+' · '+info.host.cpu);
    set('cpu-name','CPU');set('gpu-name',info.gpus?.[0]?.name||'GPU unavailable');
    const p620=info.host.platform==='lenovo-p620';
    document.querySelector('.p620-chassis').hidden=!p620;
    overlay.hidden=!p620;
    visual.style.gridTemplateColumns=p620?'':'1fr';scheduleConnectors();
  };
  let request=0;
  window.neofandDiagramUpdate=async latest=>{
    scheduleConnectors();
    const own=++request;
    const metric=(sub,name)=>latest.find(m=>m.subsystem===sub&&m.name===name)?.value;
    const cpu=latest.filter(m=>m.subsystem==='hwmon'&&/^(k10temp.*Tctl|coretemp.*Package)/.test(m.name));
    const cpuLoad=metric('host','cpu_pct'),gpuLoad=metric('gpu','gpu0_util_gpu_pct');
    set('cpu-temp',number(cpu[0]?.value,' °C',1));set('cpu-load',number(cpuLoad,' %',1));
    set('gpu-temp',number(metric('gpu','gpu0_temp_c'),' °C'));set('gpu-load',number(gpuLoad,' %',1));
    for(const [id,v] of [['cpu-bar',cpuLoad],['gpu-bar',gpuLoad]]){const n=node(id);if(Number.isFinite(v))n.value=v;else n.removeAttribute('value');}
    set('gpu-power',number(metric('gpu','gpu0_power_w'),' W',1));set('gpu-fan',number(metric('gpu','gpu0_fan_pct'),' %'));
    set('gpu-memory',gib(metric('gpu','gpu0_mem_used_bytes'))+' / '+gib(metric('gpu','gpu0_mem_total_bytes')));
    const ts=Math.max(...latest.map(m=>m.ts));
    set('sample',Date.now()/1000-ts>30?'Readings delayed':'Sampled '+new Date(ts*1000).toLocaleTimeString());
    try{
      const r=await fetch('/hardware/api/host-memory',{cache:'no-store'});if(!r.ok)throw Error();const m=await r.json();if(own!==request)return;
      set('memory-used',gib(m.used_bytes));set('memory-total','used of '+gib(m.total_bytes));node('memory-bar').value=m.used_pct;
      for(const [id,key] of [['free','free_bytes'],['available','available_bytes'],['cache','cache_bytes'],['swap','swap_used_bytes']])set('memory-'+id,gib(m[key]));
      set('memory-pressure',number(m.pressure_some_avg10,' %',2));
    }catch(_){if(own===request){set('memory-used','Unavailable');node('memory-bar').removeAttribute('value');for(const id of ['total','free','available','cache','swap','pressure'])set('memory-'+id,'—');}}
  };
  window.neofandDiagramError=()=>set('sample','Disconnected · retained readings');
})();
