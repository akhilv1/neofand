# Dimensioned 2D GPU library

This is a curated set of common local-AI board families, professional GPUs, used PCIe accelerators, and all nine Apple MPX graphics module variants in Apple's Mac Pro specification. It is not a popularity ranking or a promise of host/driver compatibility.

`catalog.json` records the exact board variant, primary manufacturer source, verified dimensions, and missing data. `models/` contains two reusable SVG projections per profile. Run `python3 build.py` to regenerate the manifest and SVG files. The webpage filters and previews them, shows occupancy relative to a configurable bay length, and links each source.

Known millimeter dimensions control the SVG envelope. Inch values use 25.4 mm/inch. Styling is schematic; it does not claim a mechanically exact fan or shroud design. The installed-edge view uses actual cooler thickness when published. Otherwise its thickness is an illustrative nominal envelope of 20.32 mm per reported slot, labeled on the artwork. A slot count is not a measured cooler thickness. Missing face height is also illustrative.

MPX modules have a proprietary mechanical and electrical envelope. Apple documents half/full-height classifications but does not give millimeter body dimensions in the cited specification. Their templates therefore remain explicitly not to scale and cannot report a bay-fit ratio until measured or backed by a mechanical drawing. Their placeholder drawing envelope is not a specification.

Manufacturer pages sometimes disagree. The RTX 5060 Ti VENTUS 2X page says 126 mm high, while one regional PDF says 127 mm. NVIDIA's SFF table lists 40 mm thickness for the 5080/5090 FE, while some product-page clearance text describes larger clearance. These conflicts are noted by model; verify the shipping revision and allow separate cable, airflow, and mounting clearance.

The A6000 3D preview is a generated perspective illustration of the user's approved system layout, not a dimensionally verified mesh. The active sensor dashboard retains its approved flat view for callout alignment.

Full-height MPX entries also carry `installation_layout` metadata for up to two modules in the Mac Pro 7,1. The three-slot visual span comes from the user's chassis reference and is deliberately separate from manufacturer `slots` and millimeter dimensions. The clean dual-MPX reference omits Afterburner and the overlaid add-in plate. It is illustrative, not a mechanical drawing. Half-height entries do not inherit the full-height visual span.

Rendered samples are `rtx-4090-fe-installed-v2.png` and `macpro-dual-mpx-v1.png`, generated with the built-in ImageGen tool. The former is an installed-edge illustration matching the approved A6000 camera; the latter follows the user-provided chassis photo. See `render-prompts.md` and `rtx-4090-installed-prompts.md` for the exact prompts. The initial broad fan-face sample was rejected and is archived locally as an earlier draft. Neither is a verified CAD model. The live A6000 dashboard artwork is unchanged.

## Approved installed-side visual families

The user selected the thinner installed-side render as the 3080/4080 visual family and the thicker render as the 3090/4090 visual family. `catalog.json` records these under `visual_profiles`, separately from manufacturer dimensions. The 3080 and 4080 Founders Edition records have been added explicitly. This brings the catalog to 63 profiles and 126 SVG views.

The thin source is `geforce-installed-thin-concept-v1.png`; the thick source is `rtx-4090-fe-installed-v2.png`. Both originate from the 4090 view study and are shape references, with model-specific markings and connectors still to be finalized. They retain the common approved camera orientation. Fit math and dimensioned SVGs use model specifications: the 3080 FE is 285 × 112 mm and two slots; the 4080 FE remains 304 × 137 × 61 mm and three slots despite the user's thin visual-family choice. The new mapping does not change measured slot spacing or claim a slim 4080 FE exists.
