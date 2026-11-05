import {SLOTS,assess,firstFreeSlot,nearestConnector,checkMove} from './fit.mjs?v=p620-1';
const $=id=>document.getElementById(id);
import {PLATFORMS,platformFor,slotsFor,compatibility} from './platforms.mjs';
let platformId=document.documentElement.dataset.platform||'macpro-7-1';
const layouts=new Map();
const profile=()=>PLATFORMS[platformId];
const slots=()=>profile().slots;
const topFor=(units,thickness=0)=>profile().anchor+profile().direction*units*profile().photoPitch-(profile().direction<0?thickness*profile().photoPitch/options().pitch_mm:0);
let models=[],items=[],lastModel=decodeURIComponent(location.hash.slice(1))||'rtx-a6000';
const storage='neofand-gpu-layout-v1';
function node(tag,cls,text){const n=document.createElement(tag);if(cls)n.className=cls;if(text!==undefined)n.textContent=text;return n;}
function options(){const num=(id,fallback,min,max)=>{const n=Number($(id).value);return Number.isFinite(n)&&n>=min&&n<=max?n:fallback;};return {platform:platformId,bay_mm:num('build-bay',profile().bay,100,600),pitch_mm:num('build-pitch',20.32,10,40),gap_mm:num('build-gap',0,0,50),reserve_io:true};}
function save(){try{layouts.set(platformId,{version:1,items,options:options()});localStorage.setItem(storage,JSON.stringify({version:1,items,options:options(),layouts:Object.fromEntries(layouts)}));}catch(_){} }
function load(data){
 if(data.version!==1||!Array.isArray(data.items)||data.items.length>24)throw new Error('Unsupported layout file');
 const nextPlatform=data.options?.platform||'macpro-7-1';
 if(!PLATFORMS[nextPlatform])throw new Error('Unknown chassis platform');
 const valid=new Set(models.map(m=>m.id));
 const measured=(value,max)=>{if(value==null||value==='')return null;const n=Number(value);if(!Number.isFinite(n)||n<1||n>max)throw new Error('Measured dimensions are out of range');return n;};
 const nextItems=data.items.map((item,i)=>{if(!item||!valid.has(item.model))throw new Error('Unknown GPU in layout');const slot=Number(item.slot);if(!Number.isInteger(slot)||slot<1||slot>(nextPlatform==='lenovo-p620'?6:7))throw new Error('Choose a connector belonging to this chassis');return {id:`card-${i}-${crypto.randomUUID()}`,model:item.model,slot,length_mm:measured(item.length_mm,600),thickness_mm:measured(item.thickness_mm,160)};});
 platformId=nextPlatform;items=nextItems;applyPlatform();
 const o=data.options||{};
 for(const [id,key,min,max] of [['build-bay','bay_mm',100,600],['build-pitch','pitch_mm',10,40],['build-gap','gap_mm',0,50]])if(Number.isFinite(Number(o[key])))$(id).value=Math.max(min,Math.min(max,Number(o[key])));
 $('build-io').checked=true;
}

function applyPlatform(){
 const p=profile();$('build-platform').value=platformId;
 $('platform-name').textContent=p.name;$('platform-description').textContent=p.description;
 $('platform-source').href=p.source;$('platform-source').textContent=p.name+' specifications ↗';
 $('chassis-stage').dataset.platform=platformId;$('chassis-stage').setAttribute('aria-label',p.name+' chassis layout preview');
 window.dispatchEvent(new CustomEvent('neofand-platform-selected',{detail:{platform:platformId}}));
 for(const img of document.querySelectorAll('.chassis-base,.chassis-foreground'))img.src='builder/'+p.image;
 document.querySelector('.chassis-base').alt=p.name+' empty interior, illustrative rendering';
 document.querySelector('.builder-check').hidden=!p.mpx;
 document.querySelectorAll('[data-mac-only]').forEach(el=>el.hidden=!p.mpx);
}
$('build-platform').addEventListener('change',()=>{
 save();const id=$('build-platform').value;
 const saved=layouts.get(id)||{version:1,items:[],options:{platform:id,bay_mm:PLATFORMS[id].bay,pitch_mm:20.32,gap_mm:0}};
 load(saved);render();$('builder-message').textContent='Switched chassis. Each platform keeps its own layout.';
});

function place(item,slot){
 const candidate=checkMove(item,slot,items,models,options());
 const allowed=candidate&&!candidate.errors.length;
 if(allowed)item.slot=slot;
 $('builder-message').textContent=allowed?`${candidate.model.name} snapped to slot ${slot}.`:`Cannot place in slot ${slot}: ${candidate?.errors.join('; ')||'Unknown card'}. Card returned to slot ${item.slot}.`;
 render();
 return allowed;
}
function draggable(n,c){
 n.tabIndex=0;n.setAttribute('role','button');n.setAttribute('aria-label',`Move ${c.model.name}, slot ${c.item.slot}. Drag to a connector or use Up and Down arrows.`);
 let drag=null;
 const end=(commit=false)=>{if(!drag)return;const d=drag;drag=null;
  if(n.hasPointerCapture(d.id))n.releasePointerCapture(d.id);
  $('chassis-stage').classList.remove('is-dragging');
  if(commit&&d.inside)place(c.item,d.slot);else{render();$('builder-message').textContent=`Move cancelled. Card remains in slot ${c.item.slot}.`;}
 };
 n.addEventListener('pointerdown',e=>{if(e.button!==0)return;e.preventDefault();n.focus();const rect=$('chassis-stage').getBoundingClientRect();
  drag={id:e.pointerId,y:e.clientY,rect,units:slots().find(s=>s.id===c.item.slot).units,slot:c.item.slot,inside:true};
  n.setPointerCapture(e.pointerId);n.classList.add('dragging');$('chassis-stage').classList.add('is-dragging');
 });
 n.addEventListener('pointermove',e=>{if(!drag||drag.id!==e.pointerId)return;
  const d=drag;d.inside=e.clientX>=d.rect.left&&e.clientX<=d.rect.right&&e.clientY>=d.rect.top&&e.clientY<=d.rect.bottom;
  d.slot=nearestConnector(d.units+profile().direction*(e.clientY-d.y)/d.rect.height*100/profile().photoPitch,c.env.mpx,options());
  const target=checkMove(c.item,d.slot,items,models,options());
  n.style.top=`${topFor(slots().find(s=>s.id===d.slot).units,c.env.thickness)}%`;
  n.dataset.drop=target.errors.length||!d.inside?'blocked':'allowed';
  for(const guide of $('chassis-slots').children)guide.dataset.target=String(Number(guide.dataset.slot)===d.slot);
  $('builder-message').textContent=!d.inside?'Release outside the chassis to cancel.':target.errors.length?`Slot ${d.slot} blocked: ${target.errors.join('; ')}`:`Release to snap ${c.model.name} into slot ${d.slot}.`;
 });
 n.addEventListener('pointerup',e=>{if(drag?.id===e.pointerId)end(true);});
 n.addEventListener('pointercancel',()=>end());n.addEventListener('lostpointercapture',()=>end());
 n.addEventListener('keydown',e=>{if(e.key==='Escape'){e.preventDefault();end();return;}if(drag||!['ArrowUp','ArrowDown'].includes(e.key))return;e.preventDefault();
  const choices=slots().filter(s=>s.id!==8&&(!c.env.mpx||s.id===1||s.id===3));
  const at=choices.findIndex(s=>s.id===c.item.slot),next=choices[at+(e.key==='ArrowUp'?-profile().direction:profile().direction)];
  if(next){place(c.item,next.id);document.querySelector(`[data-card-id="${c.item.id}"]`)?.focus();}
 });
 n.dataset.cardId=c.item.id;
}

function tableSpacing(result){
 $('slot-spacing-table').replaceChildren(...slots().map((s,i)=>{const tr=node('tr');[s.id,s.reserved?'Half-length I/O':platformId==='lenovo-p620'?`PCIe 4.0 x${s.lanes} · ${s.watts} W${s.primary?' · primary GPU':''}`:s.width===2?'Double-wide':'Single-wide',`${(s.units*result.pitch).toFixed(2)} mm`,i<slots().length-1?`${((slots()[i+1].units-s.units)*result.pitch).toFixed(2)} mm`:'—'].forEach(t=>tr.append(node('td',null,t)));return tr;}));
 $('chassis-slots').replaceChildren(...slots().map(s=>{const n=node('div','chassis-slot');n.dataset.slot=s.id;n.style.top=`${topFor(s.units)}%`;n.append(node('span',null,`${s.id}${s.id===8?' · I/O':''}`));return n;}));
}
function cardFields(c){
 const box=node('section','placed-card');
 const heading=node('div','placed-heading');const names=node('div');names.append(node('h3',null,c.model.name),node('small',null,c.model.variant));
 const remove=node('button','remove-card','Remove');remove.setAttribute('aria-label',`Remove ${c.model.name}`);remove.addEventListener('click',()=>{items=items.filter(i=>i.id!==c.item.id);render();});heading.append(names,remove);box.append(heading);
 const fields=node('div','placed-fields');
 const slotLabel=node('label',null,c.env.mpx?'MPX bay':'Connector slot');const select=node('select');select.setAttribute('aria-label',`${c.model.name} slot`);
 for(const s of slots().filter(s=>s.id!==8&&(!c.env.mpx||s.id===1||s.id===3))){const o=node('option',null,c.env.mpx?s.id===1?'A · slots 1–2':'B · slots 3–4':`Slot ${s.id} · x${s.lanes}`);o.value=s.id;select.append(o);}select.value=c.item.slot;select.addEventListener('change',()=>{place(c.item,Number(select.value));});slotLabel.append(select);fields.append(slotLabel);
 for(const [key,label,known] of [['length_mm','Length · mm',c.model.length_mm],['thickness_mm','Thickness · mm',c.model.thickness_mm]]){const wrap=node('label',null,label),input=node('input');input.type='number';input.min=1;input.max=key==='length_mm'?600:160;input.step='0.1';input.placeholder=known?String(known):`~${(key==='length_mm'?c.env.length:c.env.thickness).toFixed(1)}`;input.value=c.item[key]??'';input.setAttribute('aria-label',`${c.model.name} ${label}`);input.addEventListener('change',()=>{const v=Number(input.value);c.item[key]=Number.isFinite(v)&&v>0?Math.min(Number(input.max),v):null;render();});wrap.append(input);fields.append(wrap);}box.append(fields);
 const published=c.model.slots==null?'not published':c.model.slots;
 box.append(node('p','placed-summary',`Published slots: ${published} · occupied width ${c.env.width.toFixed(1)} mm · bay remainder ${(options().bay_mm-c.env.length).toFixed(1)} mm`));
 for(const [cls,list] of [['placed-errors',c.errors],['placed-notes',c.notes]])if(list.length){const ul=node('ul',cls);for(const m of list)ul.append(node('li',null,m));box.append(ul);}return box;
}
function render(){
 $('bay-length').value=options().bay_mm;$('bay-length').dispatchEvent(new Event('input'));
 const result=assess(items,models,options());const status=$('build-status');
 status.dataset.level=result.hasErrors?'error':result.hasEstimates?'estimate':'good';
 status.textContent=!items.length?'Add a card to begin.':result.hasErrors?`${result.cards.filter(c=>c.errors.length).length} card(s) need attention. Resolve red overlaps or length limits below.`:result.hasEstimates?'No collision in the nominal layout. Measurements are still needed for the amber card envelopes.':'No collision in the modeled length and slot envelopes. Review cable and airflow clearance separately.';
 $('placed-cards').replaceChildren(...(result.cards.length?result.cards.map(cardFields):[node('div','builder-empty','Choose a GPU and add it to the chassis. Mix board families, change slots, and inspect the clearance.') ]));
 const holder=$('chassis-cards');holder.replaceChildren();
 // Anchor calibration from the reference photograph; slot pitch is a physical scale,
 // not eight equally-spaced positions. Native-body assets have no dimension arrows.
 const p=profile(),scaleY=p.photoPitch/result.pitch,right=p.right,bayWidth=p.bayWidth;
 $('platform-compatibility').replaceChildren(...compatibility(result.cards,options()).map(t=>node('p',null,t)));
 for(const c of result.cards){if(!c.env)continue;const n=node('div','builder-card');n.dataset.invalid=String(c.errors.length>0);n.dataset.estimated=String(c.env.estimated);n.style.width=`${c.env.length/result.bay*bayWidth}%`;n.style.height=`${c.env.thickness*scaleY}%`;n.style.left=`${p.direction<0?right-c.env.length/result.bay*bayWidth:p.left}%`;n.style.top=`${topFor(c.env.start/result.pitch,c.env.thickness)}%`;n.title=`${c.model.name} · slot ${c.item.slot} · ${c.env.length.toFixed(1)} × ${c.env.thickness.toFixed(1)} mm`;
 draggable(n,c);const img=node('img');img.draggable=false;img.src=(platformId==='lenovo-p620'?c.model.p620_asset:c.model.installed_asset)||`models/${c.model.id}-installed.svg`;img.alt=`${c.model.name} installed in slot ${c.item.slot}`;
 const rasterCrop=platformId==='lenovo-p620'?c.model.p620_raster_crop:c.model.raster_crop;
 if(rasterCrop){const crop=node('div','builder-raster-crop'),r=rasterCrop,[x,y,x2,y2]=r.box,w=x2-x,h=y2-y;img.className='builder-raster';img.style.width=`${r.image_width/w*100}%`;img.style.height=`${r.image_height/h*100}%`;img.style.left=`${-x/w*100}%`;img.style.top=`${-y/h*100}%`;crop.append(img);n.append(crop);}else n.append(img);
 n.append(node('span','builder-card-name',`${c.model.name} · ${c.item.slot}`));holder.append(n);}
 tableSpacing(result);save();updateAddAvailability();
}
function addAvailability(id){
 const model=models.find(m=>m.id===id);
 const slot=model&&items.length<24?firstFreeSlot(model,items,models,options()):null;
 const reason=model?.category==='Apple MPX'&&!profile().mpx?'Apple MPX modules require a Mac Pro MPX bay.':!model?'Choose a card.':slot!==null?'':model.length_mm>options().bay_mm?`${model.name} is longer than the ${options().bay_mm} mm bay.`:model.category==='Apple MPX'?'No free MPX bay has enough room. Remove a card or module first.':'No free connector has enough space for this card. Remove or reposition a card first.';
 return {model,slot,reason};
}
function updateAddAvailability(){
 const available=addAvailability($('build-model').value);
 $('build-add').disabled=available.slot===null;
 $('build-add').title=available.reason;
 $('build-availability').textContent=available.reason;
 const library=addAvailability(lastModel);
 $('add-selected').disabled=library.slot===null;
 $('add-selected').title=library.reason;
}
$('build-model').addEventListener('change',updateAddAvailability);
$('build-add').addEventListener('click',()=>{
 const {model,slot,reason}=addAvailability($('build-model').value);
 if(slot===null){$('builder-message').textContent=reason;updateAddAvailability();return;}
 items.push({id:crypto.randomUUID(),model:model.id,slot});
 $('builder-message').textContent='Placed in the next available connector.';render();
});
for(const id of ['build-bay','build-pitch','build-gap','build-io'])$(id).addEventListener('change',render);
$('build-clear').addEventListener('click',()=>{items=[];render();});
$('build-export').addEventListener('click',()=>{const blob=new Blob([JSON.stringify({version:1,items,options:options()},null,2)],{type:'application/json'});const a=node('a');a.href=URL.createObjectURL(blob);a.download=platformId+'-gpu-layout.json';a.click();setTimeout(()=>URL.revokeObjectURL(a.href),1000);});
$('build-import').addEventListener('click',()=>$('build-import-file').click());
$('build-import-file').addEventListener('change',async e=>{try{const f=e.target.files[0];if(!f)return;if(f.size>100000)throw new Error('Layout file is too large');load(JSON.parse(await f.text()));render();$('builder-message').textContent='Layout imported.';}catch(error){$('builder-message').textContent=error.message;}e.target.value='';});
window.addEventListener('neofand-add-to-build',e=>{if(models.some(m=>m.id===e.detail.id)){$('build-model').value=e.detail.id;updateAddAvailability();$('build-add').click();}});
window.addEventListener('neofand-model-selected',e=>{lastModel=e.detail.id;if(models.length){$('build-model').value=lastModel;updateAddAvailability();}});
fetch('catalog.json').then(r=>{if(!r.ok)throw new Error('Catalog unavailable');return r.json();}).then(d=>{models=d.models;const select=$('build-model');select.replaceChildren(...models.map(m=>{const o=node('option',null,`${m.name} · ${m.variant}`);o.value=m.id;return o;}));select.value=models.some(m=>m.id===lastModel)?lastModel:'rtx-a6000';try{const saved=JSON.parse(localStorage.getItem(storage)||'null');if(saved){for(const [id,l] of Object.entries(saved.layouts||{}))if(PLATFORMS[id])layouts.set(id,l);load(saved);}}catch(_){}applyPlatform();render();}).catch(e=>{$('build-status').textContent=e.message;});
