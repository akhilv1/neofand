(() => {
  const $ = id => document.getElementById(id);
  let models = [], selected, view = 'face';
  function sourceLink(value) { const u=new URL(value); return u.protocol === 'https:' ? u.href : '#'; }
  function occupancy() {
    if (!selected) return;
    const length=selected.length_mm, bay=Number($('bay-length').value);
    if (!length || !Number.isFinite(bay) || bay<=0) { $('bay-fill').style.width='0'; $('occupancy').textContent='MPX body dimensions need measurement before a dimensional bay comparison.'; return; }
    const pct=length/bay*100, remaining=bay-length;
    $('bay-fill').style.width=`${Math.min(100,pct)}%`;
    $('bay-fill').style.background=remaining<0 ? '#b46c47' : 'var(--accent)';
    $('occupancy').textContent=`${pct.toFixed(1)}% of ${bay} mm bay length · ${Math.abs(remaining).toFixed(1)} mm ${remaining>=0?'remaining':'over length'} · cables and brackets may need extra clearance`;
  }
  function renderSelected(model) {
    selected=model;
    $('family').textContent=`${model.vendor} · ${model.category}`;
    $('name').textContent=model.name; $('variant').textContent=model.variant;
    const path=`models/${model.id}-${view}.svg`;
    $('model-image').src=path; $('model-image').alt=`${model.name} ${view} schematic`;
    $('source').href=sourceLink(model.source); $('download').href=path;
    for (const [id,key] of [['render-preview','render_preview'],['chassis-reference','chassis_reference']]) {
      const link=$(id); link.hidden=!model[key]; if(model[key])link.href=model[key];
    }
    $('render-preview').textContent=model.render_preview_label || 'Rendered sample ↗';
    $('notes').textContent=model.notes || (model.thickness_mm ? 'Manufacturer dimensions recorded for this board variant.' : 'Exact cooler thickness is unpublished here. Edge view uses a labeled nominal slot envelope.');
    if(model.installation_layout) $('notes').textContent+=' Chassis reference: up to two full-height modules, shown with the supplied three-slot spacing. Afterburner omitted; this visual spacing is not a measured thickness.';
    if(model.visual_profile_note) $('notes').textContent+=' '+model.visual_profile_note;
    const specs=[['Length',model.length_mm,'mm'],['Board height',model.height_mm,'mm'],['Cooler thickness',model.thickness_mm,'mm'],['Published slots',model.slots,'']];
    $('specs').replaceChildren(...specs.map(([name,value,unit])=>{ const d=document.createElement('div');d.className='spec';const s=document.createElement('small');s.textContent=name;const n=document.createElement('strong');n.textContent=value==null?'Not published':`${Number(value.toFixed(2))} ${unit}`;d.append(s,n);return d;}));
    document.querySelectorAll('.model-button').forEach(b=>b.setAttribute('aria-pressed',String(b.dataset.id===model.id)));
    occupancy();
    history.replaceState(null,'',`#${encodeURIComponent(model.id)}`);
  }
  function renderList() {
    const term=$('search').value.toLowerCase();
    const filtered=models.filter(m=>(!$('vendor').value||m.vendor===$('vendor').value)&&(!$('category').value||m.category===$('category').value)&&`${m.name} ${m.variant}`.toLowerCase().includes(term));
    $('count').textContent=`${filtered.length} of ${models.length} profiles`;
    $('models').replaceChildren(...filtered.map(m=>{const b=document.createElement('button');b.className='model-button';b.dataset.id=m.id;b.setAttribute('aria-pressed',String(selected?.id===m.id));b.textContent=m.name;const s=document.createElement('span');s.textContent=m.variant;b.append(s);b.addEventListener('click',()=>renderSelected(m));return b;}));
    if(filtered.length&&!filtered.includes(selected))renderSelected(filtered[0]);
  }
  ['face','edge'].forEach(v=>$(v).addEventListener('click',()=>{view=v;['face','edge'].forEach(x=>$(x).setAttribute('aria-pressed',String(x===v)));if(selected)renderSelected(selected);}));
  ['search','vendor','category'].forEach(id=>$(id).addEventListener(id==='search'?'input':'change',renderList));
  $('bay-length').addEventListener('input',occupancy);
  fetch('catalog.json').then(r=>{if(!r.ok)throw new Error('Catalog unavailable');return r.json();}).then(data=>{
    models=data.models.filter(m=>/^[a-z0-9-]+$/.test(m.id));
    ['vendor','category'].forEach(id=>[...new Set(models.map(m=>m[id]))].sort().forEach(v=>{const o=document.createElement('option');o.value=v;o.textContent=v;$(id).append(o);}));
    selected=models.find(m=>m.id===decodeURIComponent(location.hash.slice(1)))||models.find(m=>m.id==='rtx-a6000');renderList();renderSelected(selected);
  }).catch(()=>{$('name').textContent='GPU catalog unavailable';$('count').textContent='Could not load model data';});
})();
