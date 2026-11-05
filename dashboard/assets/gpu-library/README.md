# OEM GPU library and Mac Pro system builder

The library contains 55 OEM/reference board profiles: NVIDIA reference/Founders Edition, AMD reference cards, Intel Limited Edition, and Apple MPX. Eight previously cataloged partner variants are archived in `partner-catalog.json` and excluded from the main picker.

The system builder places combinations in the user's Mac Pro chassis image. It offers numbered connector placement, manual measured dimensions, bay-length and bracket-pitch calibration, extra gap, collision/length feedback, local persistence, and JSON export/import. The current preview reserves the pictured Apple I/O card and omits Afterburner. It does not change the server or hardware.

## Dimensions and fit

`catalog.json` preserves exact board variants and primary specification sources. Published length, height, cooler thickness and slot count remain independent values. Missing thickness uses a clearly marked nominal slot envelope; missing MPX body dimensions stay uncalibrated. Amber envelopes require measurements. No overall-fit guarantee is derived from a length-only comparison.

[Slot mechanics](builder/SLOT-MECHANICS.md) documents all connector offsets, the distinction between Apple numbered connectors and GPU slot width, the 20.32 mm nominal pitch, double-wide 40.64 mm spacing, MPX pair reservations, rounding tolerance and limits. [Card dimensions](builder/CARD-DIMENSIONS.md) lists every OEM board's width and source.

The collision engine is a pure module, `builder/fit.mjs`. A 3-slot card at connector 1 blocks connector 2; automatic placement skips that connector. MPX uses pairs 1–2 or 3–4 and is limited to two modules. Known body and published bracket widths are respected. A measured override can resolve unpublished dimensions.

## Appearance

`appearance.py` assigns every OEM model to an explicit board/cooler family. There is no fallback to an NVIDIA professional shroud. The 29 families distinguish GeForce Founders frames, Radeon reference triple fans, blue and dark Radeon PRO blowers, Intel A770/B580, legacy Quadro, passive accelerators, and MPX modules. Boards sharing an actual cooler can share geometry; colors, proportions and markings follow their class.

Popular OEM families use clean raster renders in `artwork/`; the remaining entries use family-specific illustrated geometry. The broad face and installed edge are separate views, with installed edge as the default. All models also have a body-only installed SVG for composition and two dimensioned projections. Rendered views are illustrations, not CAD meshes or mechanically verified shroud replicas.

The chassis compositor scales each card from its own dimensions. It crops the display bounds of the original alpha image, masks card ends behind the retention rail, and removes power connectors from clean renders. Actual cable clearance is still required. The exact built-in ImageGen prompts are in [artwork/PROMPTS.md](artwork/PROMPTS.md); manufacturer appearance sources remain linked in the catalog. The earlier perspective A6000 system preview remains available.

## Source and checks

Run `python3 build.py` (Python 3.9+ with Pillow) to regenerate metadata, the card-dimension document, and SVGs. PNGs are preserved source assets. Pillow is used only to inspect alpha bounds for browser placement; the generated PNGs are not edited by Python.

Run `node --test builder/fit.test.mjs` for meaningful collision cases: mixed widths, double/single-wide connector spacing, 3-slot overlap, I/O reservation, bay over-length, MPX pairs, measured overrides and extra gap. UI verification should cover mixed OEM layouts, visible red collisions, restored clearances, the two-MPX limit, export/import, persistence and narrow screens.

The builder saves layouts in the current browser under `neofand-gpu-layout-v1`. JSON imports accept catalog model IDs, slots 1–7, and bounded numeric controls and measured overrides; files are limited to 100 KB / 24 cards. Layouts are not uploaded to the server.


NVIDIA workstation installed views now share the user-requested A6000 photographic base with crisp per-model shroud lettering. Generation and variant suffixes remain in the picker but are omitted from the artwork. These are shared visual-family previews scaled to each catalog envelope, not separate mechanical replicas. Full-height MPX artwork fills its reserved bay to match the supplied two-module reference. Both add controls disable when no valid connector placement remains.

## Platform layouts

The builder now includes Mac Pro 7,1 and Lenovo ThinkStation P620 with separate saved layouts. P620 is a provisional visualization with documented slot-power restrictions and no connected host telemetry. See [P620 compatibility and validation](builder/P620.md). Every non-MPX profile includes a P620-oriented installed asset with readable lettering; the Mac Pro assets remain intact. RTX 3090 FE now has its own photographic study rather than sharing the 4090 asset.
