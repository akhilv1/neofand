"""Generate dimension-driven 2D GPU envelopes from manufacturer specifications."""
import json
from pathlib import Path
from xml.sax.saxutils import escape

ROOT = Path(__file__).parent
MODELS = []
NV = 'https://www.nvidia.com/en-us/products/workstations/professional-desktop-gpus/'
APPLE = 'https://support.apple.com/en-us/111907'
OLD_NV = 'https://www.nvidia.com/en-sg/products/workstations/previous-quadro-desktop-gpus/'

def add(id, name, vendor, category, length, height, thickness, slots, style, source, variant='Reference / manufacturer design', notes=''):
    MODELS.append(dict(id=id, name=name, vendor=vendor, category=category, variant=variant,
        length_mm=length, height_mm=height, thickness_mm=thickness, slots=slots,
        style=style, source=source, notes=notes, checked='2026-11-04',
        dimensional_status='body dimensions not published' if length is None else 'manufacturer length verified'))

# NVIDIA workstation reference boards. Inch dimensions are converted with 25.4 mm/in.
for id, name, length, height, slots in [
 ('rtx-a2000-12','RTX A2000 12GB',167.64,68.58,2),('rtx-a4000','RTX A4000',241.3,111.76,1),
 ('rtx-a4500','RTX A4500',266.7,111.76,2),('rtx-a5000','RTX A5000',266.7,111.76,2),
 ('rtx-a5500','RTX A5500',266.7,111.76,2),('rtx-a6000','RTX A6000',266.7,111.76,2),
 ('rtx-2000-ada','RTX 2000 Ada',167.64,68.58,2),('rtx-4000-sff-ada','RTX 4000 SFF Ada',167.64,68.58,2),
 ('rtx-4000-ada','RTX 4000 Ada',241.3,111.76,1),('rtx-4500-ada','RTX 4500 Ada',266.7,111.76,2),
 ('rtx-5000-ada','RTX 5000 Ada',266.7,111.76,2),('rtx-6000-ada','RTX 6000 Ada',266.7,111.76,2),
 ('rtx-pro-2000-blackwell','RTX PRO 2000 Blackwell',167.64,68.58,2),
 ('rtx-pro-4000-sff-blackwell','RTX PRO 4000 SFF Blackwell',167.64,68.58,2),
 ('rtx-pro-4000-blackwell','RTX PRO 4000 Blackwell',241.3,111.76,1),
 ('rtx-pro-4500-blackwell','RTX PRO 4500 Blackwell',266.7,111.76,2),
 ('rtx-pro-5000-blackwell','RTX PRO 5000 Blackwell',266.7,111.76,2),
 ('rtx-pro-6000-maxq','RTX PRO 6000 Blackwell Max-Q',266.7,111.76,2),
 ('rtx-pro-6000-workstation','RTX PRO 6000 Blackwell Workstation',304.8,137.16,2),
]:
    add(id,name,'NVIDIA','Workstation',length,height,None,slots,'flow' if id.endswith('workstation') else 'blower',NV,
        notes='Slot count is published; exact cooler thickness is not stated in the source. Installed-edge thickness uses a labeled nominal slot envelope.')
for id,name in [('quadro-p6000','Quadro P6000'),('quadro-rtx-5000','Quadro RTX 5000'),('quadro-rtx-6000','Quadro RTX 6000'),('quadro-rtx-8000','Quadro RTX 8000')]:
    add(id,name,'NVIDIA','Workstation',266.7,111.76,None,2,'blower',OLD_NV)

add('rtx-3060-ventus','GeForce RTX 3060 12GB','NVIDIA','Consumer',235,124,42,2,'dual','https://fr.msi.com/Graphics-Card/GeForce-RTX-3060-VENTUS-2X-12G-OC/Specification','MSI VENTUS 2X 12G OC')
add('rtx-4060ti-ventus','GeForce RTX 4060 Ti 16GB','NVIDIA','Consumer',199,120,42,2,'dual','https://fr.msi.com/Graphics-Card/GeForce-RTX-4060-Ti-VENTUS-2X-BLACK-16G-OC/Specification','MSI VENTUS 2X BLACK 16G OC')
add('rtx-5060ti-ventus','GeForce RTX 5060 Ti 16GB','NVIDIA','Consumer',227,126,41,2,'dual','https://www.msi.com/Graphics-Card/GeForce-RTX-5060-Ti-16G-VENTUS-2X-OC-PLUS/Specification','MSI VENTUS 2X OC PLUS','Current specification page says 126 mm; one regional PDF says 127 mm. Verify the shipping revision.')
add('rtx-4070tis-ventus','GeForce RTX 4070 Ti SUPER','NVIDIA','Consumer',308,120,50,None,'triple','https://my.msi.com/Graphics-Card/GeForce-RTX-4070-Ti-SUPER-16G-VENTUS-3X-OC/Specification','MSI VENTUS 3X OC')
add('rtx-5070ti-ventus','GeForce RTX 5070 Ti','NVIDIA','Consumer',303,121,49,None,'triple','https://www.msi.com/Graphics-Card/GeForce-RTX-5070-Ti-16G-VENTUS-3X-OC/Specification','MSI VENTUS 3X OC')
add('rtx-3080-fe','GeForce RTX 3080','NVIDIA','Consumer',285,112,None,2,'flow','https://www.nvidia.com/en-us/geforce/graphics-cards/30-series/rtx-3080-3080ti/','Founders Edition','Published dimensions and two-slot class; exact cooler thickness is not stated in the cited page.')
for id,name in [('rtx-3090-fe','GeForce RTX 3090'),('rtx-3090ti-fe','GeForce RTX 3090 Ti')]:
    add(id,name,'NVIDIA','Consumer',313,138,None,3,'flow','https://www.nvidia.com/en-us/geforce/graphics-cards/30-series/rtx-3090-3090ti/','Founders Edition')
add('rtx-4080s-fe','GeForce RTX 4080 SUPER','NVIDIA','Consumer',304,137,61,3,'flow','https://www.nvidia.com/content/geforce-gtx/GEFORCE_RTX_4080_SUPER_User_Guide_Rev1.pdf','Founders Edition')
add('rtx-4080-fe','GeForce RTX 4080','NVIDIA','Consumer',304,137,61,3,'flow','https://www.nvidia.com/en-us/geforce/graphics-cards/40-series/rtx-4080-family/','Founders Edition')
add('rtx-4090-fe','GeForce RTX 4090','NVIDIA','Consumer',304,137,61,3,'flow','https://www.nvidia.com/en-us/geforce/graphics-cards/40-series/rtx-4090/','Founders Edition')
for id,name in [('rtx-5080-fe','GeForce RTX 5080'),('rtx-5090-fe','GeForce RTX 5090')]:
    add(id,name,'NVIDIA','Consumer',304,137,40,2,'flow','https://www.nvidia.com/en-us/geforce/news/gfecnt/20246/small-form-factor-sff-ready/','Founders Edition','Dimensions follow NVIDIA SFF table; other clearance guidance on product pages is inconsistent. Cable clearance is additional.')

add('tesla-p40','Tesla P40','NVIDIA','Server PCIe',266.7,111.1504,None,2,'passive','https://images.nvidia.com/content/tesla/pdf/Tesla-P40-Product-Brief.pdf')
add('tesla-p100-pcie','Tesla P100 PCIe','NVIDIA','Server PCIe',266.7,111.76,None,2,'passive','https://images.nvidia.com/content/grid/pdf/DA-09018-001_v05.pdf')
add('tesla-v100-pcie','Tesla V100 PCIe','NVIDIA','Server PCIe',266.7,111.76,None,2,'passive','https://images.nvidia.com/content/grid/pdf/DA-09018-001_v10.pdf')
add('a100-80-pcie','A100 80GB PCIe','NVIDIA','Server PCIe',268.6,111.15,None,2,'passive','https://www.nvidia.com/content/dam/en-zz/Solutions/Data-Center/a100/pdf/PB-10577-001_v02.pdf',notes='Length includes I/O bracket; drawing lists 267.7 mm without bracket.')
add('h100-pcie','H100 PCIe','NVIDIA','Server PCIe',268.6,111.15,None,2,'passive','https://www.nvidia.com/content/dam/en-zz/Solutions/gtcs22/data-center/h100/PB-11133-001_v01.pdf',notes='Length includes I/O bracket; drawing lists 267.7 mm without bracket.')
add('l4','L4','NVIDIA','Server PCIe',169.418,68.834,None,1,'passive','https://images.nvidia.com/aem-dam/Solutions/Data-Center/l4/nvidia-ada-gpu-architecture-whitepaper-v2.1.pdf')
add('l40s','L40S','NVIDIA','Server PCIe',266.7,111.76,None,2,'passive','https://www.nvidia.com/en-us/data-center/l40s/')

add('rx-7900xt','Radeon RX 7900 XT','AMD','Consumer',276,None,None,2.5,'triple','https://www.amd.com/en/products/specifications/graphics.html',notes='Published length and slot count only; face height is illustrative.')
add('rx-7900xtx','Radeon RX 7900 XTX','AMD','Consumer',287,None,None,2.5,'triple','https://www.amd.com/en/products/graphics/desktops/radeon/7000-series/amd-radeon-rx-7900xtx.html',notes='Published length and slot count only; face height is illustrative.')
add('radeon-pro-w6800','Radeon PRO W6800','AMD','Workstation',267,None,None,2,'blower','https://www.amd.com/en/products/graphics/workstations/radeon-pro/w6800.html',notes='Full-height class is specified, not an exact millimeter height.')
add('radeon-pro-w7800','Radeon PRO W7800','AMD','Workstation',280,None,None,2,'blower','https://www.amd.com/en/products/graphics/workstations/radeon-pro/w7800.html',notes='Full-height class is specified, not an exact millimeter height.')
add('radeon-pro-w7900-dual','Radeon PRO W7900 Dual Slot','AMD','Workstation',280,None,None,2,'blower','https://www.amd.com/content/dam/amd/en/documents/partner-hub/radeon-pro/amd-radeon-pro-w7900-dual-slot-datasheet-competitive.pdf')
add('radeon-ai-pro-r9700','Radeon AI PRO R9700','AMD','Workstation',266.7,111.2,40,2,'blower','https://www.powercolor.com/product-detail258.htm','PowerColor AI PRO R9700 32G-B')
add('instinct-mi210','Instinct MI210','AMD','Server PCIe',266.7,114.3,None,2,'passive','https://www.amd.com/content/dam/amd/en/documents/instinct-business-docs/product-briefs/instinct-mi210-brochure.pdf')

add('arc-a770-le','Arc A770','Intel','Consumer',279.9,126.36,42,2,'dual','https://www.intel.com/content/www/us/en/support/articles/000092554.html','Limited Edition','Envelope includes I/O bracket and PCIe connector; body-only length 268.6 mm and shroud thickness 40.81 mm.')
add('arc-b580-le','Arc B580','Intel','Consumer',272,115,44,2,'dual','https://www.intel.com/content/www/us/en/support/articles/000091128/processors.html','Limited Edition')
add('arc-pro-b60-blower','Arc Pro B60 24GB','Intel','Workstation',289,120,42,2,'blower','https://www.sparkle.com.tw/files/20250618145718157.pdf','SPARKLE Blower')
add('arc-pro-b60-passive','Arc Pro B60 24GB','Intel','Server PCIe',190,112,19,1,'passive','https://www.asrock.com/Graphics-Card/Intel/Intel%20Arc%20Pro%20B60%20Passive%2024GB/index.asp','ASRock Passive','Manufacturer recommends 30 mm additional cable clearance.')

for id,name,kind in [
 ('mpx-580x','Radeon Pro 580X','half'),('mpx-w5500x','Radeon Pro W5500X','half'),('mpx-w6600x','Radeon Pro W6600X','half'),
 ('mpx-w5700x','Radeon Pro W5700X','full'),('mpx-vega-ii','Radeon Pro Vega II','full'),('mpx-vega-ii-duo','Radeon Pro Vega II Duo','full'),
 ('mpx-w6800x','Radeon Pro W6800X','full'),('mpx-w6800x-duo','Radeon Pro W6800X Duo','full'),('mpx-w6900x','Radeon Pro W6900X','full'),
]:
    add(id,name,'Apple / AMD','Apple MPX',None,None,None,None,'mpx-'+kind,APPLE,
        variant=f'{kind.title()}-height MPX module',notes='Apple publishes the module class, not mechanical body dimensions in the cited specification. Template is illustrative and requires measurement before dimensional placement. MPX is not a standard PCIe card.')
    MODELS[-1]['module_class'] = kind
    if kind == 'full':
        MODELS[-1]['chassis_reference'] = 'macpro-dual-mpx-v1.png'
        MODELS[-1]['installation_layout'] = {
            'mount': 'Mac Pro 7,1 MPX bay', 'max_modules': 2,
            'visual_slot_span': 3, 'slot_span_basis': 'user-provided chassis reference',
            'exclude_afterburner': True,
        }
VISUAL_PROFILES = [
    dict(id='geforce-installed-thin',label='Thin installed profile',
         asset='geforce-installed-thin-concept-v1.png',
         models=['rtx-3080-fe','rtx-4080-fe','rtx-4080s-fe']),
    dict(id='geforce-installed-thick',label='Thick installed profile',
         asset='rtx-4090-fe-installed-v2.png',
         models=['rtx-3090-fe','rtx-3090ti-fe','rtx-4090-fe']),
]
for profile in VISUAL_PROFILES:
    profile['basis'] = 'User-approved visual family, separate from manufacturer dimensions'
    for model in MODELS:
        if model['id'] in profile['models']:
            model['visual_profile'] = profile['id']
            model['render_preview'] = profile['asset']
            model['render_preview_label'] = profile['label'] + ' ↗'
            model['visual_profile_note'] = 'Illustrative family profile; source artwork is a 4090 study. Model-specific shroud, connector and markings are not yet finalized. Fit calculations use the published board dimensions, not this raster silhouette.'
            if model['id'] in {'rtx-4080-fe','rtx-4080s-fe'}:
                model['visual_profile_note'] += ' Thin is the chosen visual family; this Founders Edition is physically a three-slot, 61 mm card.'

# Keep partner-board variants in an optional data archive, not the OEM builder.
PARTNER_IDS={'rtx-3060-ventus','rtx-4060ti-ventus','rtx-5060ti-ventus','rtx-4070tis-ventus','rtx-5070ti-ventus','radeon-ai-pro-r9700','arc-pro-b60-blower','arc-pro-b60-passive'}
PARTNER_MODELS=[m for m in MODELS if m['id'] in PARTNER_IDS]
MODELS=[m for m in MODELS if m['id'] not in PARTNER_IDS]
for model in MODELS: model['board_origin']='OEM / reference'
for profile in VISUAL_PROFILES: profile['models']=[id for id in profile['models'] if id not in PARTNER_IDS]

from appearance import FAMILIES, LABELS, draw
from PIL import Image
RASTER_MAP = {
 'rtx-a6000':'rtx-a6000.png',
 'rtx-pro-6000-workstation':'rtx-pro6000-workstation.png',
 'rtx-3080-fe':'geforce-clean.png','rtx-3090-fe':'geforce-clean.png','rtx-3090ti-fe':'geforce-clean.png',
 'rtx-4080-fe':'geforce-clean.png','rtx-4080s-fe':'geforce-clean.png','rtx-4090-fe':'geforce-clean.png',
 'rtx-5080-fe':'geforce-blackwell.png','rtx-5090-fe':'geforce-blackwell.png',
 'rtx-4060ti-ventus':'msi-ventus-black.png',
 'radeon-pro-w7800':'radeon-pro-w7800.png',
 'rx-7900xt':'radeon-rx7900.png','rx-7900xtx':'radeon-rx7900.png',
 'arc-a770-le':'arc-a770.png','arc-b580-le':'arc-b580.png',
}


def geometry(model, edge=False):
    length=model['length_mm'] or 305
    known=model['thickness_mm'] if edge else model['height_mm']
    if edge:
        height=known or ((3 if model.get('module_class')=='full' else 2)*20.32 if model['category']=='Apple MPX' else (model['slots'] or 2)*20.32)
    else:
        height=known or (145 if model['category']=='Apple MPX' else 111.76)
    return length,height

def svg(model,edge=False,installed=False):
    length,height=geometry(model,edge)
    body=draw(model,length,height,edge)
    title=escape(model['name'])
    if installed:
        return f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {length} {height}" role="img" aria-label="{title} installed side">{body}</svg>'
    label=f'{model["length_mm"]:g} mm' if model['length_mm'] else 'MPX body dimensions uncalibrated'
    note='Individual board-family artwork · published envelope'
    if not model['thickness_mm'] and edge:note+=' · nominal slot thickness'
    return f'''<svg xmlns="http://www.w3.org/2000/svg" viewBox="-16 -10 {length+32} {height+45}" role="img" aria-label="{title} dimensioned view">{body}<path d="M0 {height+7}V{height+13} M0 {height+10}H{length} M{length} {height+7}V{height+13}" stroke="#448c77" fill="none" stroke-width=".6"/><text x="{length/2}" y="{height+19}" text-anchor="middle" fill="#335d50" font-family="Arial,sans-serif" font-size="6">{label}</text><text x="{length/2}" y="{height+26}" text-anchor="middle" fill="#708278" font-family="Arial,sans-serif" font-size="4">{escape(note)}</text></svg>'''

(ROOT/'models').mkdir(parents=True,exist_ok=True)
for model in MODELS:
    assert model['id'] in FAMILIES, f"Appearance required: {model['id']}"
    model['artwork_family']=FAMILIES[model['id']]
    model['artwork_description']=LABELS[model['artwork_family']]
    model['installed_asset']=f'models/{model["id"]}-installed.svg'
    model['appearance_source']=model['source']
    model['artwork_status']='family-specific illustration'
    if model['id'] in RASTER_MAP:
        asset='artwork/'+RASTER_MAP[model['id']]
        image=Image.open(ROOT/asset)
        box=image.getchannel('A').point(lambda alpha:255 if alpha>=128 else 0).getbbox() if 'A' in image.getbands() else (0,0,*image.size)
        # Ignore near-transparent edge noise, and keep the A6000 mounting tab behind the rail.
        if RASTER_MAP[model['id']]=='rtx-a6000.png': box=(box[0],box[1],box[2]-round((box[2]-box[0])*.047),box[3])
        model['installed_asset']=asset
        model['raster_crop']={'image_width':image.width,'image_height':image.height,'box':box}
        model['artwork_status']='rendered family profile'
        model['render_preview']=asset
        model['render_preview_label']='Clean installed render ↗'
    for view in ['face','edge']:
        (ROOT/'models'/f'{model["id"]}-{view}.svg').write_text(svg(model,view=='edge'))
    (ROOT/'models'/f'{model["id"]}-installed.svg').write_text(svg(model,True,True))
lines=['# OEM card dimensions', '', 'All lengths and widths below are millimeters. A slot count is not a measured cooler width. Nominal width is used only where exact thickness is unpublished; it remains uncertain in the builder. MPX reserves an entire connector pair independently of its visual body estimate.', '', '| Board / variant | Length | Board height | Cooler thickness | Published slots | Nominal fallback width | Source |', '| --- | ---: | ---: | ---: | ---: | ---: | --- |']
for m in MODELS:
    val=lambda n:'Unpublished' if n is None else f'{n:g}'
    fallback='—' if m['thickness_mm'] is not None else f'{geometry(m,True)[1]:.2f} (estimate)'
    lines.append(f"| {m['name']} · {m['variant']} | {val(m['length_mm'])} | {val(m['height_mm'])} | {val(m['thickness_mm'])} | {val(m['slots'])} | {fallback} | [Manufacturer]({m['source']}) |")
(ROOT/'builder/CARD-DIMENSIONS.md').write_text('\n'.join(lines)+'\n')
(ROOT/'partner-catalog.json').write_text(json.dumps({'version':1,'scope':'Partner variants excluded from the OEM builder','models':PARTNER_MODELS},indent=2)+'\n')
(ROOT/'catalog.json').write_text(json.dumps({'version':3,'checked':'2026-11-05','visual_profiles':VISUAL_PROFILES,'models':MODELS},indent=2)+'\n')
print(f'Generated {len(MODELS)} board profiles across {len({m["artwork_family"] for m in MODELS})} cooler families; {len(MODELS)*3} views')
