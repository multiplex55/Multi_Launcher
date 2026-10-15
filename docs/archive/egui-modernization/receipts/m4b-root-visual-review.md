# Bounded ROOT rendered-output review after M4-B

Source checkpoint `a4fbebed`. Existing `qa-framebuffer-capture` feature build passed/native0 in106.8015s, reused target, no source/framework/backend change. Input/source/result/Cargo JSON/logs under target/egui-modernization/m4b/visual-build-001/build; preserved QA executable C95E1A434FDD76F08C860DFBB40BED74F22C7851481E279B47377B52739C58A0 and build-receipt.json. M3-D normal C53 binary was preserved separately before target reuse.

Same QA binary produced two verified852x460 PNGs, scale1/native1, saved receipt status with no failure:

- Dark: target/egui-modernization/native-themed/run-m4b-dark-002/framebuffer-output/framebuffer.png, SHA2566be6fc05d1fb2c49da9a700289dbf95fe1b9b81cabc8bb02c0ab388dfc910068.
- Light: target/egui-modernization/native-themed/run-m4b-light-001/framebuffer-output/framebuffer.png, SHA2568f8629005a16ccd009385dc094c1a51d4b39588dbe0ac58dc821dff4e42c9ca8.

Parent inspected both against preserved original ROOT. Brand/header, vector search icon, rounded native query boundary and primary/secondary result hierarchy are visible/readable; no horizontal clipping in these captured viewports. Lower partial row is clipped by the scroll region. Selected/hover states, live dialog interaction, IME/DPI/monitor and OS composition are not verified by these images. Native focus/hotkey qualification gaps remain.

First Dark001 retained: parent fixture used invalid enum `Dark`; parser correctly rejected it without replacing the file and rendered defaults400x220/recovery banner/dashboard. This is not intended themed visual acceptance. Fresh002 uses existing serialized lowercase `dark`; Light001 `light`. No numbers/evidence overwritten, no binary rebuilt. The narrow fallback recovery banner/header crowding in001 merits the planned M5-C recovery-surface review; do not infer a data-loss defect or discard that image.

All three owned fixture processes closed normally via Alt+F4, exit confirmed. Per-run source/request/process/capture/provenance/visual/close evidence retained. These are rendered framebuffer pixels with opt-in instrumentation, not performance/native-display latency evidence. No benchmark launched. Continue directly M4-C and full settings/plugin rollout.
