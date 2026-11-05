import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
import test from 'node:test';
const html=readFileSync(new URL('./dashboard.html',import.meta.url),'utf8');
const source=html.slice(html.indexOf('const GAP_MIN_SECS'),html.indexOf('/* ------------------------------------------------------------------ *\n * Chart.'));
function helpers(step=10){return runInNewContext(source+';({zeroFillGaps,isSynthetic})',{stepSecs:()=>step});}
test('heartbeat sampling does not create artificial outages',()=>{
 const {zeroFillGaps}=helpers();const points=[[0,50,49,51],[300,50,49,51],[600,50,49,51]];
 assert.equal(zeroFillGaps([{points}])[0].points.length,3);
});
test('outage markers preserve observed min/max and sample count',()=>{
 const {zeroFillGaps,isSynthetic}=helpers();const points=[[0,50,49,51],[1800,60,59,61]];
 const drawn=zeroFillGaps([{points}])[0].points;
 assert.equal(drawn.length,4);assert.equal(points.length,2);
 const observed=drawn.filter(p=>!isSynthetic(p));
 assert.equal(observed.length,2);assert.equal(Math.min(...observed.map(p=>p[1])),50);
 assert.equal(Math.max(...observed.map(p=>p[1])),60);
 assert.ok(drawn.filter(isSynthetic).every(p=>p.length===5&&p[1]===0));
});
test('coarse buckets retain chronological order at the gap boundary',()=>{
 const {zeroFillGaps}=helpers(1000);const drawn=zeroFillGaps([{points:[[0,10,9,11],[1501,12,11,13]]}])[0].points;
 assert.equal(drawn.length,4);
 assert.ok(drawn.every((p,i)=>i===0||p[0]>=drawn[i-1][0]));
 assert.ok(drawn.slice(1,-1).every(p=>p[0]>0&&p[0]<1501));
});
