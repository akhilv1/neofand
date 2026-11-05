"""Board-family artwork for the two-dimensional GPU library.

Every catalog entry has an explicit family assignment. Shared coolers remain
shared; there is deliberately no fallback to a workstation NVIDIA shroud.
"""
from html import escape
import math

FAMILIES = {}
def assign(family, ids):
    for id in ids.split(): FAMILIES[id] = family
assign('ampere-compact', 'rtx-a2000-12')
assign('ampere-single', 'rtx-a4000')
assign('ampere-gold', 'rtx-a4500 rtx-a5000 rtx-a5500 rtx-a6000')
assign('ada-compact', 'rtx-2000-ada rtx-4000-sff-ada')
assign('ada-single', 'rtx-4000-ada')
assign('ada-gold', 'rtx-4500-ada rtx-5000-ada rtx-6000-ada')
assign('blackwell-compact', 'rtx-pro-2000-blackwell rtx-pro-4000-sff-blackwell')
assign('blackwell-single', 'rtx-pro-4000-blackwell')
assign('blackwell-blower', 'rtx-pro-4500-blackwell rtx-pro-5000-blackwell rtx-pro-6000-maxq')
assign('blackwell-flow', 'rtx-pro-6000-workstation')
assign('quadro-silver', 'quadro-p6000')
assign('quadro-green', 'quadro-rtx-5000 quadro-rtx-6000 quadro-rtx-8000')
assign('ventus-silver', 'rtx-3060-ventus')
assign('ventus-black', 'rtx-4060ti-ventus')
assign('ventus-modern', 'rtx-5060ti-ventus rtx-5070ti-ventus')
assign('ventus-triple', 'rtx-4070tis-ventus')
assign('fe-ampere', 'rtx-3080-fe rtx-3090-fe rtx-3090ti-fe')
assign('fe-ada', 'rtx-4080-fe rtx-4080s-fe rtx-4090-fe')
assign('fe-blackwell', 'rtx-5080-fe rtx-5090-fe')
assign('tesla-pascal', 'tesla-p40 tesla-p100-pcie')
assign('tesla-volta', 'tesla-v100-pcie')
assign('datacenter-ampere', 'a100-80-pcie')
assign('datacenter-hopper', 'h100-pcie')
assign('datacenter-low', 'l4')
assign('datacenter-ada', 'l40s')
assign('radeon-rdna3', 'rx-7900xt rx-7900xtx')
assign('radeon-pro-blue', 'radeon-pro-w6800')
assign('radeon-pro-black', 'radeon-pro-w7800 radeon-pro-w7900-dual')
assign('radeon-ai', 'radeon-ai-pro-r9700')
assign('instinct', 'instinct-mi210')
assign('arc-alchemist', 'arc-a770-le')
assign('arc-battlemage', 'arc-b580-le')
assign('arc-sparkle', 'arc-pro-b60-blower')
assign('arc-passive', 'arc-pro-b60-passive')
assign('mpx-half', 'mpx-580x mpx-w5500x mpx-w6600x')
assign('mpx-full', 'mpx-w5700x mpx-vega-ii mpx-vega-ii-duo mpx-w6800x mpx-w6800x-duo mpx-w6900x')

LABELS={
 'ampere-compact':'NVIDIA Ampere compact blower','ampere-single':'NVIDIA Ampere single-slot blower','ampere-gold':'NVIDIA Ampere black / gold blower',
 'ada-compact':'NVIDIA Ada low-profile blower','ada-single':'NVIDIA Ada single-slot blower','ada-gold':'NVIDIA Ada workstation blower',
 'blackwell-compact':'NVIDIA Blackwell compact blower','blackwell-single':'NVIDIA Blackwell single-slot blower','blackwell-blower':'NVIDIA Blackwell workstation blower','blackwell-flow':'NVIDIA Blackwell double flow-through',
 'quadro-silver':'Quadro Pascal silver / green','quadro-green':'Quadro RTX black / green',
 'ventus-silver':'MSI VENTUS silver twin-fan','ventus-black':'MSI VENTUS BLACK twin-fan','ventus-modern':'MSI VENTUS modern angular','ventus-triple':'MSI VENTUS triple-fan',
 'fe-ampere':'GeForce Ampere Founders Edition','fe-ada':'GeForce Ada Founders Edition','fe-blackwell':'GeForce Blackwell Founders Edition',
 'tesla-pascal':'Tesla Pascal passive','tesla-volta':'Tesla Volta passive','datacenter-ampere':'A100 passive enclosure','datacenter-hopper':'H100 passive enclosure','datacenter-low':'L4 low-profile passive','datacenter-ada':'L40S passive',
 'radeon-rdna3':'AMD RDNA 3 reference triple-fan','radeon-pro-blue':'Radeon PRO blue blower','radeon-pro-black':'Radeon PRO W7000 dark blower','radeon-ai':'PowerColor Radeon AI PRO blower','instinct':'AMD Instinct passive',
 'arc-alchemist':'Intel Arc A770 Limited Edition','arc-battlemage':'Intel Arc B580 Limited Edition','arc-sparkle':'SPARKLE Arc Pro enclosed black blower','arc-passive':'ASRock Arc Pro single-slot passive','mpx-half':'Apple half-height MPX','mpx-full':'Apple full-height MPX'
}

def rect(x,y,w,h,fill,rx=0,stroke=None,sw=.5):
    return f'<rect x="{x:g}" y="{y:g}" width="{max(0,w):g}" height="{max(0,h):g}" rx="{rx:g}" fill="{fill}"'+(f' stroke="{stroke}" stroke-width="{sw:g}"' if stroke else '')+'/>'
def path(d,fill='none',stroke=None,sw=.5):
    return f'<path d="{d}" fill="{fill}"'+(f' stroke="{stroke}" stroke-width="{sw:g}" stroke-linejoin="round"' if stroke else '')+'/>'
def text(x,y,label,size=5,color='#e2e4e3',anchor='start'):
    return f'<text x="{x:g}" y="{y:g}" fill="{color}" font-family="Arial,Helvetica,sans-serif" font-size="{size:g}" font-weight="600" text-anchor="{anchor}">{escape(label)}</text>'
def fins(x,y,w,h,color='#8c9494',pitch=2):
    s=rect(x,y,w,h,'#111719',1)
    for i in range(int(w/pitch)):
        xx=x+(i+.5)*pitch
        s+=path(f'M{xx:g} {y:g}V{y+h:g}',stroke=color,sw=.65)
    return s

def fan(cx,cy,r,color='#343d44',hub=''):
    s=f'<circle cx="{cx:g}" cy="{cy:g}" r="{r:g}" fill="#0c1114" stroke="#697178" stroke-width=".65"/>'
    for angle in range(0,360,36):
        s+=f'<path d="M{cx:g} {cy-r*.24:g} Q{cx+r*.94:g} {cy-r*.94:g} {cx+r*.9:g} {cy+r*.08:g} L{cx+r*.2:g} {cy+r*.25:g}Z" transform="rotate({angle} {cx:g} {cy:g})" fill="{color}" stroke="#0b1114" stroke-width=".3"/>'
    s+=f'<circle cx="{cx:g}" cy="{cy:g}" r="{r*.23:g}" fill="#232b30" stroke="#656e73" stroke-width=".4"/>'
    if hub:s+=text(cx,cy+r*.05,hub,r*.13,'#a6afb3','middle')
    return s

def connector(x,y,count=1,pins=8,scale=1):
    # Sockets are omitted from clean presentation artwork; fit excludes cables.
    return ''


def colors(f,m):
    if f=='radeon-pro-blue':return '#164890','#0b2547','#405d90'
    if f=='arc-sparkle':return '#303639','#161c1e','#818780'
    if f=='radeon-ai':return '#232b2e','#101719','#a22726'
    if f=='quadro-silver':return '#929b9f','#555f63','#78aa32'
    if f.startswith('mpx'):return '#303538','#191e21','#474d51'
    if f=='fe-ampere':return '#333b3e','#1d2427','#afa89a'
    if f=='fe-ada':return '#30383a','#171e21','#343b3c' if m['id']=='rtx-4080s-fe' else '#929794'
    if f=='fe-blackwell':return '#242b2e','#121719','#6d7273'
    if f=='datacenter-hopper':return '#9e9270','#5c5b4e','#cdc19a'
    if f=='instinct':return '#383d40','#171c1f','#bc3841'
    if f=='arc-passive' or f=='datacenter-low':return '#9da8ac','#667276','#c2c9c8'
    return '#343e43','#10181d','#6c767c'

def defs(f,m):
    a,b,c=colors(f,m)
    return f'<defs><linearGradient id="shell" x2="0" y2="1"><stop stop-color="{a}"/><stop offset=".5" stop-color="{b}"/><stop offset="1" stop-color="{a}"/></linearGradient><linearGradient id="metal" x2="0" y2="1"><stop stop-color="{c}"/><stop offset=".4" stop-color="{a}"/><stop offset="1" stop-color="{c}"/></linearGradient></defs>'

def edge(m,w,h):
    f=FAMILIES[m['id']];a,b,c=colors(f,m);mid=h*.52
    s=rect(.5,.5,w-1,h-1,'url(#shell)',min(3,h*.12),'#5d696e',.6)
    label=m['name'].replace('GeForce ','').replace('Radeon ','').replace('Blackwell','').strip()
    if f.startswith('mpx'):
        s+=path(f'M3 5H{w-3:g} M3 {h-5:g}H{w-3:g}',stroke='#0a1013',sw=3)
        s+=text(8,h*.36,m['name'],min(5,h*.15),'#939c9f')+text(8,h*.36+5,'MPX Module',3.1,'#8c9598')
        s+=f'<circle cx="{w-12:g}" cy="{h*.72:g}" r="2.1" fill="none" stroke="#9ba1a1" stroke-width=".5"/>'
    elif f.startswith('fe-') or f=='blackwell-flow':
        left=w*.23;right=w*.84
        s+=fins(left,h*.18,right-left,h*.76,'#8d9595',2.6)
        s+=rect(1,1,left-2,h-2,'url(#metal)',min(4,h*.2))
        s+=rect(right,1,w-right-1,h-2,'url(#metal)',min(3,h*.15))
        s+=path(f'M2 {h*.17:g}H{w-2:g} M2 {h*.92:g}H{w-2:g}',stroke=c,sw=1)
        s+=text(left*.49,h*.57,'NVIDIA RTX PRO' if f=='blackwell-flow' else 'GEFORCE RTX',min(5.4,h*.15),'#e8ecea','middle')
        s+=connector(w*.63,h*.1,1,12,.8)
        if f=='fe-ampere':s+=path(f'M{w*.2:g} 2L{w*.25:g} {h*.2:g} M{w*.8:g} 2L{w*.75:g} {h*.2:g}',stroke=c,sw=1.6)
        if f=='fe-blackwell':s+=path(f'M{w*.27:g} 3L{w*.4:g} {h*.17:g}H{w*.7:g}',stroke='#737b7c',sw=1.8)
    elif f.startswith('ventus'):
        s+=fins(6,h*.24,w-12,h*.64,'#59676d',2)
        silver=f in ('ventus-silver','ventus-modern','ventus-triple')
        trim='#8f9b9e' if silver else '#343d42'
        s+=path(f'M2 {h*.12:g}H{w*.23:g}L{w*.28:g} {h*.35:g}H{w*.72:g}L{w*.78:g} {h*.12:g}H{w-2:g}V{h*.38:g}H{w*.8:g}L{w*.75:g} {h*.5:g}H{w*.25:g}L{w*.2:g} {h*.38:g}H2Z',trim)
        s+=rect(w*.32,h*.2,w*.37,h*.32,'#1a252c',1)+text(w*.505,h*.44,'GEFORCE RTX',min(6,h*.2),'#d6dcdd','middle')
        s+=text(9,h*.25,'MSI',min(5,h*.17),'#d4dcde')
        s+=connector(w*.76,h*.04,1,12 if m['slots'] is None or '5070' in m['id'] else 8,.65)
    elif f=='radeon-rdna3':
        s+=fins(5,h*.2,w-10,h*.63,'#777e7d',1.9)
        s+=path(f'M1 3H{w*.1:g}L{w*.15:g} {h*.15:g}H{w*.85:g}L{w*.9:g} 3H{w-1:g}V{h*.37:g}H{w*.86:g}L{w*.8:g} {h*.47:g}H{w*.2:g}L{w*.14:g} {h*.37:g}H1Z','#232c31',stroke='#737c7e',sw=.5)
        s+=text(w*.49,h*.31,'RADEON',min(7,h*.23),'#f0f0ec','middle')
        for i in range(3):s+=rect(w*.13+i*3,h*.25,1.5,h*.55,'#c74442',.2)
        s+=connector(w*.73,h*.04,2,8,.7)
    elif f=='radeon-pro-blue':
        s+=rect(4,h*.13,w-8,h*.66,'#16488e',2)
        s+=text(12,h*.55,'AMD RADEON PRO',min(6.2,h*.19),'#eef2f7')
        s+=rect(9,h*.82,w-18,h*.08,'#0d1a29',1)
        s+=connector(w*.78,h*.06,2,8,.65)
        s+=path(f'M{w*.79:g} {h*.18:g}L{w*.9:g} {h*.82:g}',stroke='#4f75a5',sw=1.6)
    elif f=='radeon-pro-black' or f=='radeon-ai':
        s+=path(f'M3 {h*.12:g}H{w*.65:g}L{w*.72:g} {h*.3:g}H{w-4:g}',stroke='#5e6e83' if f=='radeon-pro-black' else '#aa4040',sw=1.4)
        s+=path(f'M4 {h*.86:g}H{w-4:g}',stroke='#9fa7a8',sw=.8)
        s+=text(11,h*.59,'RADEON AI PRO' if f=='radeon-ai' else 'AMD RADEON PRO',min(6.2,h*.18),'#c8d0d2')
        s+=connector(w*.81,h*.06,1,12,.65)
    elif f in ('arc-alchemist','arc-battlemage'):
        if f=='arc-alchemist':
            s+=rect(3,h*.12,w-6,h*.74,'none',5,'#7473a0',.7)
            s+=path(f'M6 {h*.9:g}H{w-6:g}',stroke='#8f82bc',sw=1.5)
            s+=fins(w*.6,h*.5,w*.35,h*.35,'#454e5b',2.2)
            s+=text(w*.43,h*.59,'intel ARC',min(7.5,h*.25),'#e2e9f4','middle')
            s+=connector(w*.77,h*.06,2,8,.6)
        else:
            s+=fins(w*.24,h*.18,w*.65,h*.7,'#59636c',2.2)
            s+=path(f'M2 2H{w*.22:g}L{w*.27:g} {h*.85:g}H{w*.9:g}L{w-2:g} {h*.48:g}V{h-2:g}H2Z','#222b31',stroke='#5d656b',sw=.6)
            s+=text(w*.135,h*.56,'intel ARC',min(6.1,h*.2),'#e3e7ec','middle')
            s+=connector(w*.76,h*.05,1,8,.7)
    elif f=='arc-sparkle':
        s+=rect(3,h*.14,w-6,h*.7,'#252e31',2)+text(12,h*.55,'intel ARC PRO',min(6.2,h*.21),'#e2eafb')
        s+=text(w-10,h*.55,'SPARKLE',min(4.5,h*.16),'#cbd6f2','end')
        s+=rect(9,h*.85,w-18,h*.07,'#293f69',1)+connector(w*.76,h*.04,1,8,.6)
    elif f=='arc-passive':
        s+=fins(5,h*.1,w-10,h*.7,'#b8c0be',1.7)
        s+=rect(5,h*.65,w-10,h*.27,'#303b43',1)+text(10,h*.86,'ASRock · intel ARC PRO',min(4.3,h*.22),'#cdd7da')
    elif f.startswith('quadro'):
        s+=rect(4,h*.17,w-8,h*.18,'#79ad37',1)
        s+=fins(w*.5,h*.47,w*.45,h*.38,'#797f78',2.1)
        s+=text(9,h*.68,label,min(6,h*.2),'#e7ece9')
    elif f in ('ampere-gold','ada-gold','blackwell-blower'):
        gold='#b8a57b' if f=='ampere-gold' else '#a89564' if f=='ada-gold' else '#a48f54'
        s+=fins(5,h*.4,w-10,h*.49,gold,1.55)
        s+=rect(5,h*.12,w-10,h*.24,'#263038',.5)
        s+=text(8,h*.3,'NVIDIA',min(6,h*.16),'#d4c5a0')+text(w-8,h*.3,label,min(4.8,h*.14),'#d4c5a0','end')
        if f=='blackwell-blower':s+=path(f'M{w*.7:g} 3L{w*.77:g} {h*.4:g}',stroke='#c0ad78',sw=1.6)
    elif f.endswith('compact') or f.endswith('single'):
        s+=fins(w*.42,h*.45,w*.53,h*.42,'#7d837d' if 'ampere' in f else '#9d8a5e',1.9)
        s+=text(8,h*.56,'NVIDIA',min(5,h*.23),'#c2b58f')
        s+=path(f'M4 {h*.84:g}H{w*.35:g}L{w*.43:g} {h*.5:g}',stroke='#747b7a',sw=.8)
    else:
        palette={'tesla-pascal':'#aab1aa','tesla-volta':'#b4aa7f','datacenter-ampere':'#747c78','datacenter-hopper':'#b6ae87','datacenter-low':'#aeb9b8','datacenter-ada':'#bcab70','instinct':'#a3a9aa'}
        s+=fins(5,h*.14,w-10,h*.69,palette[f],1.7)
        s+=rect(6,h*.63,w*.62,h*.26,'#1a2625',.8)+text(10,h*.82,m['name'].upper(),min(6,h*.2),'#d9e1db')
        if f=='instinct':s+=rect(w-10,1,8,h-2,'#a52d34',1)
        elif f=='datacenter-hopper':s+=rect(w-12,2,10,h-4,'#ab9f73',1)
        elif f=='datacenter-low':s+=rect(w-9,h*.05,7,h*.9,'#bbc2bc',1)
        elif f=='tesla-pascal':s+=rect(2,h*.7,w-4,h*.18,'#315e42',.5)
    return s

def face(m,w,h):
    f=FAMILIES[m['id']];a,b,c=colors(f,m)
    s=rect(.5,.5,w-1,h-1,'url(#shell)',min(6,h*.08),'#68737a',.6)
    label=m['name'].replace('GeForce ','')
    if f.startswith('mpx'):
        s+=path(f'M4 7H{w-4:g} M4 {h-7:g}H{w-4:g}',stroke='#11191d',sw=4)
        s+=text(10,h*.3,label,min(6,h*.12),'#929a9d')+text(10,h*.3+6,'MPX Module',3.5,'#858e92')
    elif f.startswith('fe-') or f=='blackwell-flow':
        count=2 if f in ('fe-blackwell','blackwell-flow') else 1
        if count==1:
            s+=fins(w*.53,5,w*.44,h-10,'#666f72',2.2)+fan(w*.245,h*.5,h*.425)
            s+=path(f'M4 3H{w*.39:g}L{w*.61:g} {h*.5:g}L{w*.39:g} {h-3:g}H4 M{w-4:g} 3H{w*.69:g}L{w*.48:g} {h*.5:g}L{w*.69:g} {h-3:g}H{w-4:g}',stroke=c,sw=4)
        else:
            for x in (.255,.745):s+=fan(w*x,h*.5,min(h*.43,w*.22))
            s+=path(f'M{w*.5:g} 3V{h-3:g}',stroke=c,sw=3)
    elif f.startswith('ventus') or f in ('radeon-rdna3','arc-alchemist','arc-battlemage'):
        count=3 if f=='radeon-rdna3' or m['style']=='triple' else 2
        if f.startswith('ventus'):
            trim='#a4adb1' if f in ('ventus-silver','ventus-triple') else '#535e65'
            for x in (0,w):s+=path(f'M{x:g} 4L{abs(w*.13-x):g} 4L{abs(w*.2-x):g} {h*.3:g}L{abs(w*.08-x):g} {h*.4:g}L{x:g} {h*.2:g}Z',trim)
        elif f=='radeon-rdna3':
            s+=path(f'M3 3H{w-3:g}V{h-3:g}H3Z',stroke='#778183',sw=1.5)
            for x in (.12,.14,.16):s+=rect(w*x,1,1.5,h*.18,'#bd373c')
        elif f=='arc-alchemist':s+=rect(3,3,w-6,h-6,'none',8,'#8078a8',1.5)
        for i in range(count):s+=fan(w*(i+.5)/count,h*.5,min(h*.40,w/count*.43),hub='intel' if f.startswith('arc') else 'MSI' if f.startswith('ventus') else 'R')
    elif f in ('arc-passive','tesla-pascal','tesla-volta','datacenter-ampere','datacenter-hopper','datacenter-low','datacenter-ada','instinct'):
        s+=fins(6,5,w-12,h-10,'#b9b8a7' if f=='datacenter-hopper' else '#909e9f',2)
        s+=rect(w*.12,h*.35,w*.7,h*.25,'#293530',1)+text(w*.47,h*.51,label,7,'#d8e3dc','middle')
    else:
        # Blowers are circular only in the broad face; their installed side is separate.
        if f=='radeon-pro-blue':s+=rect(2,2,w-4,h-4,'#194b91',4)
        elif f=='arc-sparkle':s+=rect(2,2,w-4,h-4,'#252e31',4)
        elif f=='quadro-silver':s+=rect(2,2,w-4,h-4,'url(#metal)',4)
        elif f.startswith('quadro'):s+=rect(4,4,w-8,h-8,'none',3,'#7ca643',2)
        r=min(h*.39,w*.18);cx=w*.19
        s+=fan(cx,h*.5,r,hub='intel' if f=='arc-sparkle' else '')
        if f in ('ampere-gold','ada-gold','blackwell-blower'):
            s+=fins(w*.42,h*.18,w*.53,h*.62,'#b6a16e',1.8)
        elif f.startswith('radeon'):
            s+=path(f'M{w*.34:g} {h*.07:g}H{w-7:g}L{w-7:g} {h*.93:g}H{w*.34:g}L{w*.27:g} {h*.76:g}V{h*.24:g}Z',stroke='#5f6b7c',sw=1.6)
        s+=text(w*.45,h*.5,label,min(7,h*.12),'#d2dadb')
    return s


def draw(model,length,height,edge_view):
    family=FAMILIES[model['id']]
    return defs(family,model)+(edge(model,length,height) if edge_view else face(model,length,height))
