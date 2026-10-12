# C3-C event delivery matrix — LOCAL ONLY

Source base `464bd5db2d876ac7e9da688d41b22bc398794647`, unchanged production source from827990c. Planner c0_recon independently inspected all33 variants and actual senders. Costs describe source work, not measured timings. Dynamic fixtures exercise only an audited subset; inventory coverage does not claim execution coverage.

F = LauncherApp::new watcher wiring -> gui::watch::watch_file_with_wake callback; FileWatcher origin, successful enqueue then ViewportWake. R = gui::send_event registry fanout; Registry origin (original caller provenance lost), enqueue under registry mutex then wake callbacks outside. P = direct RadialProvider sender, worker egui repaint (capacity helper root/designer). G = direct default Gui; C = explicit CommandHost. Direct egui repaint is distinct from ViewportWake.

|Variant|Production producer / origin and wake|Reducer effects, ordering and acknowledgment obligations|Source cost, not measured duration|
|---|---|---|---|
|Actions|F: actions file|Typed transaction load; missing/malformed keeps last-good/diagnostic; equal clears diagnostic without publication; changed publishes latest custom-prefix/index-tail and bumps version; preserve order|File I/O/parse; changed full cache/query publication|
|IndexReady|IndexingOwner::attach notifier; IndexCoordinator, send then wake|Match config/generation, reject stale, acknowledge, merge latest custom; last-good on failure; payload-free wake is not result identity|Coordinator reduction; possible full publication|
|Folders|F: folders file|Last-good on missing/error; success replaces alias maps, invalidates geometry, requests query refresh|File load/map construction|
|Bookmarks|F: bookmarks file|Last-good on missing/error; success replaces aliases, requests query refresh|File load/map construction|
|Clipboard|F: clipboard file|DashboardRefreshRequest::Clipboard; transport delivery distinct from downstream reduction|Small reducer; asynchronous backend excluded|
|Snippets|F: snippets file|DashboardRefreshRequest::Snippets|Small reducer; backend excluded|
|Notes|F: notes directory|DashboardRefreshRequest::Notes|Small reducer; backend excluded|
|Todos|F: todos file|DashboardRefreshRequest::Todos|Small reducer; backend excluded|
|Favorites|F: favorites file|DashboardRefreshRequest::Favorites|Small reducer; backend excluded|
|Gestures|F: gestures config|DashboardRefreshRequest::Gestures|Small reducer; backend excluded|
|Dashboard|LauncherApp::new dashboard_event_cb passed to Dashboard::new; Dashboard origin, send then ctx repaint|Reload dashboard; warnings/toasts; payload does not narrow reload|Configuration/presentation|
|Recycle|actions::system::recycle_clean; R|Success toast or error report; producer sends optimistic success immediately after spawning, not cleanup completion acknowledgment; NEVER invoke in fixtures|Small reducer; cleanup excluded|
|ExecuteAction|mouse_gestures::service::worker_loop prepared/ordinary action branches; R|activate_action Gesture; potentially irreversible, never drop/merge/reorder|Arbitrary command/native/persistence|
|RadialDispatch|main both ControllerEvent::DispatchRequested loops; R|Current lease/tombstone check before resolution; consume valid identity once; preserve confirmation/fallback/execution policy|Cheap rejection or arbitrary execution|
|RadialPrepare|main both PrepareRequested branches; R|Preparation identity, immutable frames/catalog, correlated RadialPrepareReply|Potential broad catalog/layout|
|RadialResolveDeferred|main both DeferredResolutionRequested branches; R|Cancellation/current identity, correlated reply or bounded provider work; reply obligations on stale/error|Resolution/search submission|
|RadialDeferredSearchReady|resolve_deferred_radial radial-query-provider-search worker; P send then root repaint|Cancellation/current envelope/revisions; result or query fallback, correlated reply|Search-result projection/resolution|
|RadialAuthoringSearchReady|start_next_authoring_provider_search worker success; P send then root/designer repaint|Reject cancelled/obsolete editor/test identity; check generation; current result or retry/pending|Search projection/editor|
|RadialAuthoringSearchFailed|Same worker caught failure; P same wake|Cancelled/obsolete cannot overwrite current; current failure/retry handling|Small failure reduction|
|AuthoringProviderCapacityAvailable|notify_provider_search_capacity_available via release_provider_search_capacity_and_wake; also start_next_authoring_provider_search spawn-failure; P|If free retire deferred catalog/resume; always attempt next request; cancellation releases capacity. Helper repaints root/designer; spawn-failure send has no dedicated immediate repaint|Small plus possible submission|
|RadialInvalidate|main document/resource reload, settings/restart, authoring publication/migration; R|Invalidate appearance/diagnostics/leases/pending confirmation before dependent new generation|Retained-state disposal|
|RadialConfigDiagnostic|main external document reload success/failure; R|Some reports reload failure, None currently no-op; valid document retained upstream|Small diagnostic|
|RadialRuntimeDiagnostic|main startup migration, controller Error/LayoutFailed, settings/runtime failures; R|Ordered radial.runtime error feedback; no transport dedup policy|Diagnostic/toast|
|RadialDiagnostic|main both ControllerEvent::Diagnostic and startup/runtime forwarding; R|Expected layout bounded fingerprint retention/repaint; non-info error, info log; reducer dedup does not authorize transport coalescing|Bounded diagnostics|
|RadialSubmenuPlacementFailure|main both SubmenuPlacementFailed branches; R|Correlated session/frame notice; root and stable placement viewport repaint|Small state/repaint|
|RadialPlacementActionResult|main placement action commit success/failure; R|Apply only matching session/frame; stale cannot alter another notice|Small correlated reduction|
|RadialMigrationNotice|main startup/later migration success; R|Info toast; maintain publication/notice order|Small toast|
|RadialMigrationState|main startup/later migration publication; R|Install receipt/default in launcher/settings editor, ordered publication|Small state|
|ScreenDrawStart|main::dispatch_screen_draw_trigger_actions; R plus explicit root repaint|Start/focus, failure report, reconcile recovery; domain idempotence does not permit transport collapse|May start native/capture work|
|ScreenDrawRecover|Same function LauncherToggle recovery; R plus root repaint|Epoch/scope/duplicate checks; exact controller/launcher restore; stale returns before native restore|Cheap stale or native recovery|
|ScreenDrawEmergency|Same function emergency/emergency-kind branch; R plus root repaint|Producer calls emergency_pause before queue; reducer recovery/restore; preserve urgent identity; queue timing excludes earlier native pause|Cheap stale or native recovery|
|ClipboardModify|No production queue enqueue found; render::poll_clipboard_modify_runtime and actions::drain_clipboard_modify_immediate direct reducers|No queue age/origin; immediate complete/failed no-op; config success diagnostic/search/cache refresh; failure reports; startup diagnostic replacement. History/root effects precede direct reduction|Variant-dependent cache/search; not queue-measured|
|VirtualDesktop|spawn_virtual_desktop_command_with_history_query early active-window failure G; background completion C; ctx repaint|Token/query/visibility or preserve-root check; success outcome/history, stale toast suppression; current error report/stale log; preserved root restore|Outcome/history; native desktop work excluded|

Active app queue remains unbounded std::sync::mpsc; no new queue introduced. EventSinkRegistration::queued is registry wake bookkeeping, not actual app depth. Envelope depth includes send reservations and its sampled maximum is not exact concurrent peak. Pre-owner capacity256 drops oldest and replays to first owner only; age begins at replay and excludes pre-owner residence. After an owner has existed, no-sink events are discarded. FIFO describes accepted channel/per-sender order, not a fabricated total order between racing producers.

EventDrain includes post-loop completion-index rebuilding; whole-arm handler timers include early continues. Their sum need not equal drain time. Successful send acknowledgment follows send, so dequeue may precede acknowledgment. Disabled observation has no timestamp/metadata/depth Arc. All test receipt vectors have bounded capacity and fail on overflow.

Safety: no recycle cleanup/query, clipboard operation, desktop operation, real emergency producer or action execution. File refresh variants may start native dashboard work despite dashboard.enabled=false; they are inventoried, not dynamically injected without an inert backend. Set dashboard_initial_refresh_queued=true before FIRST root frame. Stale emergency fixture must mint then set_active(false) with unwind-safe global exclusive-owner cleanup; radial fixture uses missing lease rejection before resolution. Neither proves successful native recovery/dispatch. Callback release precedes workspace/environment restoration.

C3-D/E decisions remain gated on independently accepted actual queue/handler/frame/wake evidence. There is no whitelist or drain-budget proposal in this matrix.

Idempotence classification is conditional domain behavior, not a transport whitelist:

- Actions/Folders/Bookmarks read latest persisted state, but intervening typed errors, version/publication and dependent actions make blind collapse unproven. Equal Actions is publication-idempotent, not a claim that arbitrary notifications are interchangeable.
- Clipboard/Snippets/Notes/Todos/Favorites/Gestures request downstream refresh; that downstream reducer may combine requests. Exact transport ordering and asynchronous effects have not been proved coalescible. Dashboard reload/warnings likewise has no blanket idempotence proof.
- IndexReady may be a no-op when no current result exists; coordinator result identity, acknowledgment and last-good transitions still govern every dispatch.
- ExecuteAction is potentially irreversible. Recycle is ordered optimistic feedback. Neither is a coalescing candidate here.
- RadialDispatch/RadialPrepare/RadialResolveDeferred/RadialDeferredSearchReady/RadialAuthoringSearchReady/RadialAuthoringSearchFailed/AuthoringProviderCapacityAvailable carry exactly-once/current-identity/reply or capacity obligations; stale/duplicate rejection does not authorize transport dropping.
- RadialInvalidate/RadialMigrationState are ordered state publication. RadialConfigDiagnostic(None) currently does nothing, but Some reports an error. RadialRuntimeDiagnostic/RadialDiagnostic/RadialSubmenuPlacementFailure/RadialPlacementActionResult/RadialMigrationNotice are ordered diagnostic/correlated feedback; local fingerprint/session suppression does not prove arbitrary queue idempotence.
- ScreenDrawStart/ScreenDrawRecover/ScreenDrawEmergency have duplicate/stale guards, plus lifecycle and urgent restoration effects. Preserve every accepted intent in channel order.
- ClipboardModify is currently direct, with immediate-complete/failed no-op subvariants; config/startup subvariants mutate diagnostics/search/cache. VirtualDesktop combines identity-sensitive outcome/history/feedback and preserved-root restoration. Neither has a blanket queued idempotence proof.
