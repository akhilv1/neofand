# Mac Pro 7,1 slot mechanics

This is a two-dimensional length/slot-clearance preview. It uses the user's 305 mm bay measurement and an editable nominal PCI bracket pitch of 20.32 mm (0.8 in). It is not a measured mechanical drawing of this chassis.

## Connector numbering and card width are different

An Apple numbered slot identifies a connector position. A GPU's two-slot or three-slot width describes bracket-pitch units. Apple spaces connectors 1–4 double-wide, followed by single-wide positions 5–7. A three-slot card at connector 1 therefore blocks connector 2; the next clear double-wide connector is normally 3.

[Apple's Mac Pro 2019 slot map](https://support.apple.com/en-my/101640) establishes the single/double-wide classes and MPX connector pairs. The millimeter offsets below are derived from those classes and the configurable nominal pitch; Apple does not provide these caliper measurements on that page.

| Connector | Width class | Offset from connector 1 | Distance to next connector |
| --- | --- | ---: | ---: |
| 1 | Double-wide, x16 | 0.00 mm | 40.64 mm |
| 2 | Double-wide, x8 | 40.64 mm | 40.64 mm |
| 3 | Double-wide, x16 | 81.28 mm | 40.64 mm |
| 4 | Double-wide, x16 | 121.92 mm | 40.64 mm |
| 5 | Single-wide, x16 | 162.56 mm | 20.32 mm |
| 6 | Single-wide, x8 | 182.88 mm | 20.32 mm |
| 7 | Single-wide, x8 | 203.20 mm | 20.32 mm |
| 8 | Half-length x4, Apple I/O reserved | 223.52 mm | — |

## Collision envelope

For a standard PCIe card:

- Start = its connector's offset, measured upward from connector 1.
- Occupied width = max(published or manually measured cooler thickness, published slot count × pitch).
- If thickness is unpublished, use slot count × pitch. If both are missing, use an explicitly uncertain two-slot estimate.
- End = start + occupied width. Intersecting intervals are reported as overlaps.
- An optional extra gap adds a separation requirement between cards.
- A 0.1 mm comparison tolerance handles rounded 61 mm dimensions versus 3 × 20.32 = 60.96 mm. This is numerical rounding tolerance, not assembly clearance.
- Length is compared with the user-entered bay measurement. Red marks indicate an overlap or over-length condition; amber marks indicate missing mechanical measurements.

The manual thickness and length inputs describe the user's measured board. They resolve otherwise unknown dimensions; published bracket occupancy remains a lower bound. The visual's positions and rendered surface details remain approximate even when its envelope dimensions are known.

Examples: two two-slot cards fit at connectors 1 and 2 with no extra gap. A three-slot 4090 at connector 1 overlaps a card at connector 2 by about 20.4 mm. A three-slot card at connector 5 covers positions 6 and 7 but ends at the I/O boundary, subject to rounding and physical clearance checks.

## MPX

Apple permits MPX modules in connector pairs 1–2 and 3–4. Each module reserves the complete pair (4 nominal bracket pitches = 81.28 mm), even if its visible body is thinner. The builder admits at most two MPX modules and only offers bay A / connector 1 or bay B / connector 3. The user supplied a three-slot full-height body reference; that is a visual estimate, not a published thickness. Full-height and half-height MPX body dimensions remain uncalibrated until measured. Reserving the whole bay prevents another card from being placed in a connector consumed by MPX.

## Rendering and clipping

The empty-chassis reference is the user's GPU-side Mac Pro photograph. Connector 1 is visually anchored near y = 79.66% of that image; one bracket pitch is about 3.59% of image height. These photo anchors are illustrative. The 305 mm bay spans approximately x = 25.1%–87.4%. Actual dimensions drive the collision model, not pixel comparisons.

Card images are scaled to their mechanical envelopes using the bounds of opaque pixels. The retained originals are unchanged; near-transparent background noise is ignored for placement. A clipping mask bounds the card layer at the right edge, and a foreground copy of the chassis retention rail covers mounting hardware. The A6000's protruding mounting tab is also excluded from its artwork bounds. No metal bracket can paint over the frontmost retention rail.

Power sockets/connectors are intentionally absent from the clean presentation artwork. That does not remove their real-world clearance requirement. Afterburner is omitted. The Apple I/O card remains in the photograph and its position stays reserved.

## What is not checked

The preview does not certify driver/OS compatibility, lane allocation, PSU power or connector capacity, airflow, cable bend radius, or perpendicular board-height clearance. A card with no drawn overlap can still need additional room. Confirm clearances on the actual machine before moving hardware.

See [each OEM card's dimensions and sources](CARD-DIMENSIONS.md). The JSON manifest is `../catalog.json`. NVIDIA workstation lengths deliberately vary by model: compact cards are about 167.6 mm, A4000/RTX 4000 are 241.3 mm, many full-size models are 266.7 mm, and RTX PRO 6000 Blackwell Workstation is 304.8 mm. [NVIDIA comparison](https://www.nvidia.com/en-us/products/workstations/professional-desktop-gpus/).
