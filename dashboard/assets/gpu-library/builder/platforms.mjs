export const PLATFORMS = {
 'macpro-7-1': {id:'macpro-7-1',name:'Mac Pro 7,1',bay:305,units:12,direction:-1,anchor:79.66,photoPitch:3.59,left:25.1,right:87.4,bayWidth:62.3,image:'macpro-empty.png',mpx:true,
  slots:[{id:1,units:0,lanes:16,width:2},{id:2,units:2,lanes:8,width:2},{id:3,units:4,lanes:16,width:2},{id:4,units:6,lanes:16,width:2},{id:5,units:8,lanes:16,width:1},{id:6,units:9,lanes:8,width:1},{id:7,units:10,lanes:8,width:1},{id:8,units:11,lanes:4,width:1,reserved:true}],
  source:'https://support.apple.com/en-my/101640',description:'Apple double-wide connector positions 1–4, single-wide 5–7; slot 8 reserved for I/O. Bay length is your 305 mm measurement.'},
 'lenovo-p620': {id:'lenovo-p620',name:'Lenovo ThinkStation P620',bay:312,units:6,direction:1,anchor:47.5,photoPitch:3.5,left:13,right:78,bayWidth:65,image:'p620-empty.png',mpx:false,psuWatts:1000,
  slots:[{id:1,units:0,lanes:16,width:1,watts:75},{id:2,units:1,lanes:8,width:1,watts:25},{id:3,units:2,lanes:16,width:1,watts:75,primary:true},{id:4,units:3,lanes:16,width:1,watts:75},{id:5,units:4,lanes:16,width:1,watts:75},{id:6,units:5,lanes:8,width:1,watts:25}],
  source:'https://psref.lenovo.com/syspool/Sys/PDF/ThinkStation/ThinkStation_P620/ThinkStation_P620_Spec.pdf',description:'Six PCIe 4.0 connectors numbered top to bottom. Slot 3 is the primary GPU position. Preview uses nominal 20.32 mm pitch and a provisional 312 mm full-length envelope; measure the actual bay and cable clearance before confirming fit.'}
};
export const platformFor=(options={})=>PLATFORMS[options.platform]||PLATFORMS['macpro-7-1'];
export const slotsFor=(options={})=>platformFor(options).slots;
export function compatibility(cards,options={}){
 const p=platformFor(options);if(p.id!=='lenovo-p620')return [];
 const notes=['P620 preview only · Ubuntu telemetry not connected.','1000 W PSU rating is for the whole system, not a GPU power budget. Check the exact Lenovo cable kit and power configuration.'];
 if(cards.length)notes.push('Mechanical fit does not confirm firmware, driver, power or cooling support. GPU height, connector bends and drive-cage clearance need measurement.');
 if(cards.some(c=>c.model?.id==='rtx-a6000'))notes.push('Lenovo documents up to two RTX A6000 cards with the specified EPS12V adapter wiring. Do not substitute a PCIe 8-pin lead for EPS12V.');
 if(cards.filter(c=>c.model?.id==='rtx-a6000').length>2)notes.push('This exceeds Lenovo’s documented dual-A6000 configuration.');
 if(cards.filter(c=>c.model?.id==='rtx-6000-ada').length>1)notes.push('Lenovo’s power guide supports only one RTX 6000 Ada in this platform.');
 if(cards.filter(c=>(c.model?.slots??2)>=2).length>2)notes.push('More than two multi-slot GPUs is outside the configurations established by Lenovo’s power guide; do not infer power support from physical fit.');
 const vendors=new Set(cards.map(c=>c.model?.vendor).filter(Boolean));
 if(vendors.has('AMD')&&vendors.has('NVIDIA'))notes.push('Mixed NVIDIA / AMD graphics is outside Lenovo’s documented supported configurations.');
 if(cards.some(c=>c.model?.category==='Consumer'||c.model?.category==='Server PCIe'))notes.push('Consumer / datacenter boards are not established as Lenovo-qualified by this preview. Verify individually.');
 return notes;
}
