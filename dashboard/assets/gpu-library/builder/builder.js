import {SLOTS,assess,firstFreeSlot} from './fit.mjs';
const $=id=>document.getElementById(id);
const ANCHOR=79.66, PHOTO_PITCH=3.59;
let models=[],items=[],lastModel=decodeURIComponent(location.hash.slice(1))||'rtx-a6000';
const storage='neofand-gpu-layout-v1';
function node(tag,cls,text){const n=document.createElement(tag);if(cls)n.className=cls;if(text!==undefined)n.textContent=text;return n;}
function options(){const num=(id,fallback,min,max)=>{const n=Number($(id).value);return Number.isFinite(n)&&n>=min&&n<=max?n:fallback;};return {bay_mm:num('build-bay',305,100,600),pitch_mm:num('build-pitch',20.32,10,40),gap_mm:num('build-gap',0,0,50),reserve_io:true};}
function save(){try{localStorage.setItem(storage,JSON.stringify({version:1,items,options:options()}));}catch(_){} }
function load(data){
 if(data.version!==1||!Array.isArray(data.items)||data.items.length>24)throw new Error('Unsupported layout file');
 const valid=new Set(models.map(m=>m.id));
 const measured=(value,max)=>{if(value==null||value==='')return null;const n=Number(value);if(!Number.isFinite(n)||n<1||n>max)throw new Error('Measured dimensions are out of range');return n;};
 const nextItems=data.items.map((item,i)=>{if(!item||!valid.has(item.model))throw new Error('Unknown GPU in layout');const slot=Number(item.slot);if(!Number.isInteger(slot)||slot<1||slot>7)throw new Error('Choose connector slots 1–7');return {id:`card-${i}-${crypto.randomUUID()}`,model:item.model,slot,length_mm:measured(item.length_mm,600),thickness_mm:measured(item.thickness_mm,160)};});
 items=nextItems;
 const o=data.options||{};
 for(const [id,key,min,max] of [['build-bay','bay_mm',100,600],['build-pitch','pitch_mm',10,40],['build-gap','gap_mm',0,50]])if(Number.isFinite(Number(o[key])))$(id).value=Math.max(min,Math.min(max,Number(o[key])));
 $('build-io').checked=true;
}
function tableSpacing(result){
 $('slot-spacing-table').replaceChildren(...SLOTS.map((s,i)=>{const tr=node('tr');[s.id,s.id===8?'Half-length I/O':s.width===2?'Double-wide':'Single-wide',`${(s.units*result.pitch).toFixed(2)} mm`,i<7?`${((SLOTS[i+1].units-s.units)*result.pitch).toFixed(2)} mm`:'—'].forEach(t=>tr.append(node('td',null,t)));return tr;}));
 $('chassis-slots').replaceChildren(...SLOTS.map(s=>{const n=node('div','chassis-slot');n.style.top=`${ANCHOR-s.units*PHOTO_PITCH}%`;n.append(node('span',null,`${s.id}${s.id===8?' · I/O':''}`));return n;}));
}
function cardFields(c){
 const box=node('section','placed-card');
 const heading=node('div','placed-heading');const names=node('div');names.append(node('h3',null,c.model.name),node('small',null,c.model.variant));
 const remove=node('button','remove-card','Remove');remove.setAttribute('aria-label',`Remove ${c.model.name}`);remove.addEventListener('click',()=>{items=items.filter(i=>i.id!==c.item.id);render();});heading.append(names,remove);box.append(heading);
 const fields=node('div','placed-fields');
 const slotLabel=node('label',null,c.env.mpx?'MPX bay':'Connector slot');const select=node('select');select.setAttribute('aria-label',`${c.model.name} slot`);
 for(const s of SLOTS.filter(s=>s.id!==8&&(!c.env.mpx||s.id===1||s.id===3))){const o=node('option',null,c.env.mpx?s.id===1?'A · slots 1–2':'B · slots 3–4':`Slot ${s.id} · x${s.lanes}`);o.value=s.id;select.append(o);}select.value=c.item.slot;select.addEventListener('change',()=>{c.item.slot=Number(select.value);render();});slotLabel.append(select);fields.append(slotLabel);
 for(const [key,label,known] of [['length_mm','Length · mm',c.model.length_mm],['thickness_mm','Thickness · mm',c.model.thickness_mm]]){const wrap=node('label',null,label),input=node('input');input.type='number';input.min=1;input.max=key==='length_mm'?600:160;input.step='0.1';input.placeholder=known?String(known):`~${(key==='length_mm'?c.env.length:c.env.thickness).toFixed(1)}`;input.value=c.item[key]??'';input.setAttribute('aria-label',`${c.model.name} ${label}`);input.addEventListener('change',()=>{const v=Number(input.value);c.item[key]=Number.isFinite(v)&&v>0?Math.min(Number(input.max),v):null;render();});wrap.append(input);fields.append(wrap);}box.append(fields);
 const published=c.model.slots==null?'not published':c.model.slots;
 box.append(node('p','placed-summary',`Published slots: ${published} · occupied width ${c.env.width.toFixed(1)} mm · bay remainder ${(options().bay_mm-c.env.length).toFixed(1)} mm`));
 for(const [cls,list] of [['placed-errors',c.errors],['placed-notes',c.notes]])if(list.length){const ul=node('ul',cls);for(const m of list)ul.append(node('li',null,m));box.append(ul);}return box;
}
function render(){
 const result=assess(items,models,options());const status=$('build-status');
 status.dataset.level=result.hasErrors?'error':result.hasEstimates?'estimate':'good';
 status.textContent=!items.length?'Add a card to begin.':result.hasErrors?`${result.cards.filter(c=>c.errors.length).length} card(s) need attention. Resolve red overlaps or length limits below.`:result.hasEstimates?'No collision in the nominal layout. Measurements are still needed for the amber card envelopes.':'No collision in the modeled length and slot envelopes. Review cable and airflow clearance separately.';
 $('placed-cards').replaceChildren(...(result.cards.length?result.cards.map(cardFields):[node('div','builder-empty','Choose a GPU and add it to the chassis. Mix board families, change slots, and inspect the clearance.') ]));
 const holder=$('chassis-cards');holder.replaceChildren();
 // Anchor calibration from the reference photograph; slot pitch is a physical scale,
 // not eight equally-spaced positions. Native-body assets have no dimension arrows.
 const scaleY=PHOTO_PITCH/result.pitch; const right=87.4; const bayWidth=62.3;
 for(const c of result.cards){if(!c.env)continue;const n=node('div','builder-card');n.dataset.invalid=String(c.errors.length>0);n.dataset.estimated=String(c.env.estimated);n.style.width=`${c.env.length/result.bay*bayWidth}%`;n.style.height=`${c.env.thickness*scaleY}%`;n.style.left=`${right-c.env.length/result.bay*bayWidth}%`;n.style.top=`${ANCHOR-c.env.start*scaleY-c.env.thickness*scaleY}%`;n.title=`${c.model.name} · slot ${c.item.slot} · ${c.env.length.toFixed(1)} × ${c.env.thickness.toFixed(1)} mm`;
 const img=node('img');img.src=c.model.installed_asset||`models/${c.model.id}-installed.svg`;img.alt=`${c.model.name} installed in slot ${c.item.slot}`;
 if(c.model.raster_crop){const crop=node('div','builder-raster-crop'),r=c.model.raster_crop,[x,y,x2,y2]=r.box,w=x2-x,h=y2-y;img.className='builder-raster';img.style.width=`${r.image_width/w*100}%`;img.style.height=`${r.image_height/h*100}%`;img.style.left=`${-x/w*100}%`;img.style.top=`${-y/h*100}%`;crop.append(img);n.append(crop);}else n.append(img);
 n.append(node('span','builder-card-name',`${c.model.name} · ${c.item.slot}`));holder.append(n);}
 tableSpacing(result);save();
}
$('build-add').addEventListener('click',()=>{if(items.length>=24)return;const m=models.find(m=>m.id===$('build-model').value);if(!m)return;if(m.category==='Apple MPX'&&items.filter(i=>models.find(x=>x.id===i.model)?.category==='Apple MPX').length>=2){$('builder-message').textContent='Both MPX bays are occupied. Remove a module before adding another.';return;}const slot=firstFreeSlot(m,items,models,options());items.push({id:crypto.randomUUID(),model:m.id,slot:slot??(m.category==='Apple MPX'?1:1)});$('builder-message').textContent=slot?'Placed in the next available connector.':'Placed for inspection; this layout needs attention.';render();});
for(const id of ['build-bay','build-pitch','build-gap','build-io'])$(id).addEventListener('change',render);
$('build-clear').addEventListener('click',()=>{items=[];render();});
$('build-export').addEventListener('click',()=>{const blob=new Blob([JSON.stringify({version:1,items,options:options()},null,2)],{type:'application/json'});const a=node('a');a.href=URL.createObjectURL(blob);a.download='macpro-gpu-layout.json';a.click();setTimeout(()=>URL.revokeObjectURL(a.href),1000);});
$('build-import').addEventListener('click',()=>$('build-import-file').click());
$('build-import-file').addEventListener('change',async e=>{try{const f=e.target.files[0];if(!f)return;if(f.size>100000)throw new Error('Layout file is too large');load(JSON.parse(await f.text()));render();$('builder-message').textContent='Layout imported.';}catch(error){$('builder-message').textContent=error.message;}e.target.value='';});
window.addEventListener('neofand-add-to-build',e=>{if(models.some(m=>m.id===e.detail.id)){$('build-model').value=e.detail.id;$('build-add').click();}});
window.addEventListener('neofand-model-selected',e=>{lastModel=e.detail.id;if(models.length)$('build-model').value=lastModel;});
fetch('catalog.json').then(r=>{if(!r.ok)throw new Error('Catalog unavailable');return r.json();}).then(d=>{models=d.models;const select=$('build-model');select.replaceChildren(...models.map(m=>{const o=node('option',null,`${m.name} · ${m.variant}`);o.value=m.id;return o;}));select.value=models.some(m=>m.id===lastModel)?lastModel:'rtx-a6000';try{const saved=JSON.parse(localStorage.getItem(storage)||'null');if(saved)load(saved);}catch(_){}render();}).catch(e=>{$('build-status').textContent=e.message;});
