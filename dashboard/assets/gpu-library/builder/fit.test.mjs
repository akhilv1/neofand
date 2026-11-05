import test from 'node:test';
import assert from 'node:assert/strict';
import {assess,firstFreeSlot,dimensions,PITCH} from './fit.mjs';
const models=[
 {id:'two',name:'Two-slot',length_mm:267,thickness_mm:40,slots:2},
 {id:'three',name:'Three-slot',length_mm:304,thickness_mm:61,slots:3},
 {id:'unknown',name:'Unknown width',length_mm:267,thickness_mm:null,slots:2},
 {id:'long',name:'Long card',length_mm:313,thickness_mm:40,slots:2},
 {id:'mpx',name:'MPX',category:'Apple MPX',module_class:'full',length_mm:null,thickness_mm:null}
];
const c=(id,model,slot,extra={})=>({id,model,slot,...extra});
test('double-wide connector spacing fits adjacent two-slot cards',()=>assert.equal(assess([c('a','two',1),c('b','two',2)],models).hasErrors,false));
test('three-slot card blocks next double-wide connector',()=>assert.equal(assess([c('a','three',1),c('b','two',2)],models).pairs.length,1));
test('three-slot placement in slots 1 and 3 has clearance',()=>assert.equal(assess([c('a','three',1),c('b','three',3)],models).hasErrors,false));
test('three-slot slot5 covers single-wide slots6 and7',()=>assert.equal(assess([c('a','three',5),c('b','two',6)],models).pairs.length,1));
test('rounding 60.96 to61 does not falsely collide with IO',()=>assert.equal(assess([c('a','three',5)],models).hasErrors,false));
test('slot7 dual card collides with reservedIO',()=>assert.match(assess([c('a','two',7)],models).cards[0].errors.join(),/I\/O/));
test('long card exceeds305mm bay',()=>assert.match(assess([c('a','long',1)],models).cards[0].errors.join(),/8.0 mm/));
test('MPX reserves slots1and2 independently of visual artwork',()=>assert.equal(assess([c('a','mpx',1),c('b','two',2)],models).pairs.length,1));
test('two MPX bays allowed; invalid start rejected',()=>{assert.equal(assess([c('a','mpx',1),c('b','mpx',3)],models).hasErrors,false);assert.equal(assess([c('a','mpx',2)],models).hasErrors,true);});
test('unpublished width remains uncertain until measured',()=>{assert.equal(dimensions(models[2]).estimated,true);assert.equal(dimensions(models[2],{thickness_mm:41}).estimated,false);});
test('auto placement skips collision',()=>assert.equal(firstFreeSlot(models[0],[c('a','three',1)],models),3));
test('nominal pitch remains20.32 and gap is checked',()=>{assert.equal(PITCH,20.32);assert.equal(assess([c('a','two',1),c('b','two',2)],models,{gap_mm:2}).hasErrors,true);});

test('calibrated pitch controls unknown nominal thickness and reserved width',()=>{const env=assess([c('a','unknown',1)],models,{pitch_mm:21}).cards[0].env;assert.equal(env.thickness,42);assert.equal(env.width,42);});

import {nearestConnector,checkMove} from './fit.mjs';
test('drag snapping respects double then single connector pitches',()=>{assert.equal(nearestConnector(2.8),2);assert.equal(nearestConnector(3.2),3);assert.equal(nearestConnector(8.6),6);assert.equal(nearestConnector(10.8),8);});
test('MPX snaps only to its two bays',()=>{assert.equal(nearestConnector(1.9,true),1);assert.equal(nearestConnector(2.1,true),3);assert.equal(nearestConnector(11,true),3);});
test('move validates destination without mutating original slot',()=>{const items=[c('a','three',1),c('b','two',3)];assert.ok(checkMove(items[0],2,items,models).errors.length);assert.equal(items[0].slot,1);assert.equal(checkMove(items[0],5,items,models).errors.length,0);});
test('moving a card ignores its previous occupancy but blocks reserved IO',()=>{const item=c('a','two',1);assert.equal(checkMove(item,2,[item],models).errors.length,0);assert.ok(checkMove(item,7,[item],models).errors.length);});

test('no connector is offered when all usable width is occupied',()=>{const filled=[c('a','two',1),c('b','two',2),c('c','two',3),c('d','two',4),c('e','three',5)];assert.equal(firstFreeSlot(models[0],filled,models),null);});
test('over-length cards cannot be added even to an empty chassis',()=>assert.equal(firstFreeSlot(models[3],[],models),null));
test('two MPX modules leave no third MPX placement',()=>assert.equal(firstFreeSlot(models[4],[c('a','mpx',1),c('b','mpx',3)],models),null));

test('full MPX artwork fills its reserved bay without changing connector occupancy',()=>{const env=assess([c('a','mpx',1)],models).cards[0].env;assert.equal(env.thickness,4*PITCH);assert.equal(env.width,4*PITCH);assert.equal(env.estimated,true);});

const p620={platform:'lenovo-p620'};
test('P620 prefers primary GPU connector 3 then connector 1',()=>{assert.equal(firstFreeSlot(models[0],[],models,p620),3);assert.equal(firstFreeSlot(models[0],[c('a','two',3)],models,p620),1);});
test('P620 rejects MPX independently of user length overrides',()=>{assert.equal(firstFreeSlot(models[4],[],models,p620),null);assert.match(assess([c('a','mpx',1,{length_mm:267})],models,p620).cards[0].errors.join(),/incompatible/);});
test('P620 double-width GPU blocks immediately adjacent connector',()=>assert.equal(assess([c('a','two',3),c('b','two',4)],models,p620).pairs.length,1));
test('P620 permits two GPUs in connectors 1 and 3',()=>assert.equal(assess([c('a','two',1),c('b','two',3)],models,p620).hasErrors,false));
test('P620 25W positions conservatively block unverified GPU slot power',()=>{for(const slot of [2,6])assert.match(assess([c('a','two',slot)],models,p620).cards[0].errors.join(),/25 W/);});
test('P620 capacity guard and chassis bottom reject overflow',()=>{assert.equal(firstFreeSlot(models[0],[c('a','two',1),c('b','two',3),c('c','two',5)],models,p620),null);assert.match(assess([c('a','three',5)],models,p620).cards[0].errors.join(),/beyond/);});
test('P620 snapping uses six nominal consecutive positions',()=>{assert.equal(nearestConnector(1.2,false,p620),2);assert.equal(nearestConnector(5.8,false,p620),6);});
test('P620 provisional length is checked and can be calibrated',()=>{assert.equal(firstFreeSlot(models[3],[],models,p620),null);assert.equal(firstFreeSlot(models[3],[],models,{...p620,bay_mm:320}),3);});
import {compatibility} from './platforms.mjs';
test('P620 notes preserve lack of live telemetry and cable qualification',()=>{const notes=compatibility([{model:{id:'rtx-a6000'}}],p620).join();assert.match(notes,/not connected/);assert.match(notes,/EPS12V/);});
