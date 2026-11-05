// Distances are mechanical preview coordinates, measured upward from slot 1.
// Apple: slots 1–4 double-wide, 5–7 single-wide, 8 half-length I/O.
export const PITCH = 20.32;
import {platformFor,slotsFor} from './platforms.mjs';
export const SLOTS=slotsFor();
export const APPLE_SOURCE='https://support.apple.com/en-my/101640';
const validNumber=n=>Number.isFinite(Number(n))&&Number(n)>0;
export function dimensions(model,item={},pitch=PITCH) {
  const manualLength=validNumber(item.length_mm), manualThickness=validNumber(item.thickness_mm);
  const mpx=model.category==='Apple MPX';
  const length=manualLength?Number(item.length_mm):model.length_mm??305;
  const thickness=manualThickness?Number(item.thickness_mm):model.thickness_mm??(mpx?(model.module_class==='full'?4:2)*pitch:(model.slots??2)*pitch);
  const lengthKnown=manualLength||Number.isFinite(model.length_mm);
  const thicknessKnown=manualThickness||Number.isFinite(model.thickness_mm);
  const bracket=mpx?0:(model.slots??0)*pitch;
  return {length,thickness,lengthKnown,thicknessKnown,mpx,
    reservedWidth:mpx?4*pitch:Math.max(thickness,bracket),
    estimated:!lengthKnown||!thicknessKnown};
}
export function envelope(model,item,options={}) {
  const pitch=validNumber(options.pitch_mm)?Number(options.pitch_mm):PITCH;
  const platform=platformFor(options);
  const slot=slotsFor(options).find(s=>s.id===Number(item.slot));
  const d=dimensions(model,item,pitch);
  const start=(slot?.units??0)*pitch;
  const width=d.mpx?4*pitch:Math.max(d.thickness,(model.slots??0)*pitch);
  return {...d,start,end:start+width,width,slot:slot?.id,validSlot:Boolean(slot)&&!slot.reserved&&(!d.mpx||(platform.mpx&&(slot.id===1||slot.id===3)))};
}
export function assess(items,models,options={}) {
  const platform=platformFor(options);
  const byId=new Map(models.map(m=>[m.id,m]));
  const pitch=validNumber(options.pitch_mm)?Number(options.pitch_mm):PITCH;
  const bay=validNumber(options.bay_mm)?Number(options.bay_mm):platform.bay;
  const gap=Number.isFinite(Number(options.gap_mm))?Math.max(0,Number(options.gap_mm)):0;
  const io=platform.mpx&&options.reserve_io!==false;
  const tolerance=0.1; // Published dimensions may round 60.96 mm to 61 mm.
  const cards=items.map(item=>{const model=byId.get(item.model);return {item,model,env:model?envelope(model,item,{...options,pitch_mm:pitch}):null,errors:[],notes:[]};});
  for(const c of cards) {
    if(!c.model){c.errors.push('Unknown card model');continue;}
    if(c.env.mpx&&!platform.mpx)c.errors.push('Apple MPX modules are incompatible with the P620');
    if(!c.env.validSlot)c.errors.push(c.env.mpx?'MPX modules must start in bay A (slot 1) or bay B (slot 3)':`Choose a usable ${platform.name} connector`);
    if(c.env.length>bay+tolerance)c.errors.push(`${(c.env.length-bay).toFixed(1)} mm longer than the bay`);
    if(c.env.end>platform.units*pitch+tolerance)c.errors.push('Extends beyond the expansion area');
    if(io&&c.env.end>11*pitch+tolerance)c.errors.push('Overlaps the Apple I/O card in slot 8');
    if(platform.id==='lenovo-p620'){
      const slot=platform.slots.find(s=>s.id===c.item.slot);
      if(slot?.watts===25)c.errors.push('25 W x8 connector: GPU slot-power compatibility is unverified; use a 75 W x16 position');
      c.notes.push('P620 bay dimensions are provisional; verify length, height and cable clearance on the host');
    }
    if(c.env.estimated)c.notes.push('Unpublished dimensions use a nominal envelope; enter measured dimensions to confirm');
    if(c.env.mpx)c.notes.push('Reserves the complete MPX connector pair');
    if(c.model.style==='passive')c.notes.push('Passive card requires directed system airflow');
  }
  const pairs=[];
  for(let i=0;i<cards.length;i++)for(let j=i+1;j<cards.length;j++) {
    const a=cards[i],b=cards[j]; if(!a.env||!b.env)continue;
    const overlap=Math.min(a.env.end,b.env.end)-Math.max(a.env.start,b.env.start);
    const separation=Math.max(a.env.start,b.env.start)-Math.min(a.env.end,b.env.end);
    if(overlap>tolerance) {
      const message=`Collides with ${b.model.name} in slot ${b.item.slot} (${overlap.toFixed(1)} mm overlap)`;
      a.errors.push(message);b.errors.push(`Collides with ${a.model.name} in slot ${a.item.slot} (${overlap.toFixed(1)} mm overlap)`);
      pairs.push({a:a.item.id,b:b.item.id,overlap_mm:overlap});
    } else if(separation<gap-tolerance) {
      a.errors.push(`Less than ${gap} mm clearance to slot ${b.item.slot}`);
      b.errors.push(`Less than ${gap} mm clearance to slot ${a.item.slot}`);
    }
  }
  const mpx=cards.filter(c=>c.env?.mpx);
  if(mpx.length>2)for(const c of mpx)c.errors.push('Maximum two MPX modules');
  return {cards,pairs,hasErrors:cards.some(c=>c.errors.length),hasEstimates:cards.some(c=>c.env?.estimated),pitch,bay,gap};
}
export function firstFreeSlot(model,items,models,options={}) {
  const p=platformFor(options);
  if(model.category==='Apple MPX'&&!p.mpx)return null;
  const choices=model.category==='Apple MPX'?[1,3]:p.id==='lenovo-p620'?[3,1,4,5,2,6]:[1,2,3,4,5,6,7];
  for(const slot of choices) {
    const probe={id:'candidate',model:model.id,slot};
    const result=assess([...items,probe],models,options);
    if(result.cards.at(-1).errors.length===0)return slot;
  }
  return null;
}

// Snap by physical connector offset, never by equally spaced slot numbers.
export function nearestConnector(units,mpx=false,options={}) {
  const choices=slotsFor(options).filter(s=>!mpx||s.id===1||s.id===3);
  return choices.reduce((best,s)=>Math.abs(s.units-units)<Math.abs(best.units-units)?s:best).id;
}
export function checkMove(item,slot,items,models,options={}) {
  return assess(items.map(i=>i.id===item.id?{...i,slot}:i),models,options).cards.find(c=>c.item.id===item.id);
}
